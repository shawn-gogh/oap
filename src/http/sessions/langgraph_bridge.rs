use std::collections::{HashMap, HashSet};

use futures_util::StreamExt;
use serde_json::{json, Map, Value};
use sqlx::PgPool;

use crate::{
    db::managed_agents::{artifacts, runtime_events, session_control, sessions},
    errors::GatewayError,
    managed_agents::adapters::{
        artifacts::DatabaseArtifactAdapter, types::ArtifactReference, ArtifactAdapter,
    },
    proxy::state::AppState,
};

use super::external_bridge::TraceHeaders;

#[derive(Default)]
struct LangGraphStreamState {
    latest_state: Option<Value>,
    updates: Map<String, Value>,
    text: String,
    paused: bool,
    run_id: Option<String>,
    last_event_id: Option<String>,
    child_invocations: HashMap<String, String>,
}

struct LangGraphConfig<'a> {
    assistant_id: &'a str,
    input_field: &'a str,
    output_path: &'a str,
    base: &'a str,
}

pub(super) async fn invoke(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    input: &Value,
    prompt: &str,
    trace: &TraceHeaders,
) -> Result<Option<Value>, GatewayError> {
    let config = config(source, &credential.api_base)?;
    let thread_id = ensure_thread(state, pool, row, credential, &config, trace).await?;
    let mapped = graph_input(input, config.input_field, prompt);
    stream_run(
        state,
        pool,
        row,
        credential,
        &config,
        &thread_id,
        json!({"input": mapped}),
        trace,
    )
    .await
}

pub(super) async fn resume(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    binding: &session_control::schema::SessionInvocationRow,
    input: &Value,
    trace: &TraceHeaders,
) -> Result<Option<Value>, GatewayError> {
    let config = config(source, &credential.api_base)?;
    let thread_id = binding.remote_session_id.as_deref().ok_or_else(|| {
        GatewayError::InvalidConfig("LangGraph continuation is missing thread_id".to_owned())
    })?;
    let resume_input = resolve_pending_interrupt(pool, row, binding, input).await?;
    stream_run(
        state,
        pool,
        row,
        credential,
        &config,
        thread_id,
        json!({"command": {"resume": resume_input}}),
        trace,
    )
    .await
}

pub(super) async fn cancel(
    state: &AppState,
    _row: &sessions::schema::SessionRow,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    binding: &session_control::schema::SessionInvocationRow,
    trace: &TraceHeaders,
) -> Result<(), GatewayError> {
    let Some(thread_id) = binding.remote_session_id.as_deref() else {
        return Ok(());
    };
    let Some(run_id) = binding.remote_task_id.as_deref() else {
        return Ok(());
    };
    let config = config(source, &credential.api_base)?;
    let response = authenticated(
        trace.apply(state.http.post(cancel_url(config.base, thread_id, run_id))),
        &credential.api_key,
    )
    .send()
    .await
    .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    ensure_status(response).await
}

#[allow(clippy::too_many_arguments)]
async fn stream_run(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    config: &LangGraphConfig<'_>,
    thread_id: &str,
    request: Value,
    trace: &TraceHeaders,
) -> Result<Option<Value>, GatewayError> {
    let mut body = json!({
        "assistant_id": config.assistant_id,
        "stream_mode": ["updates", "values", "messages-tuple", "custom"],
        "stream_subgraphs": true,
        "stream_resumable": true,
        "on_disconnect": "continue"
    });
    if let Some(object) = request.as_object() {
        for (key, value) in object {
            body[key] = value.clone();
        }
    }
    let request = authenticated(
        trace.apply(
            state
                .http
                .post(format!("{}/threads/{thread_id}/runs/stream", config.base)),
        ),
        &credential.api_key,
    )
    .header("accept", "text/event-stream")
    .json(&body);
    let response = request
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    consume_stream(state, pool, row, credential, config, thread_id, response).await
}

async fn consume_stream(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    config: &LangGraphConfig<'_>,
    thread_id: &str,
    response: reqwest::Response,
) -> Result<Option<Value>, GatewayError> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(GatewayError::SandboxError(format!(
            "LangGraph returned HTTP {}: {}",
            status.as_u16(),
            error_message(&body)
        )));
    }
    let is_stream = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/event-stream"));
    if !is_stream {
        return Err(GatewayError::SandboxError(
            "LangGraph streaming endpoint did not return text/event-stream".to_owned(),
        ));
    }

    let mut response = response;
    let mut decoder = SseDecoder::default();
    let mut run = LangGraphStreamState::default();
    let mut reconnects = 0;
    loop {
        let mut stream = response.bytes_stream();
        let mut stream_error = None;
        while let Some(chunk) = stream.next().await {
            if state.external_bridge_cancellations.is_cancelled(&row.id) {
                return Err(GatewayError::SandboxError(
                    "LangGraph invocation cancelled".to_owned(),
                ));
            }
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    stream_error = Some(error.to_string());
                    break;
                }
            };
            for frame in decoder.push(&chunk)? {
                handle_frame(state, pool, row, credential, thread_id, &mut run, frame).await?;
            }
        }
        let Some(error) = stream_error else {
            break;
        };
        let (Some(run_id), Some(cursor)) = (run.run_id.as_deref(), run.last_event_id.as_deref())
        else {
            return Err(GatewayError::SandboxError(format!(
                "LangGraph stream disconnected before it became resumable: {error}"
            )));
        };
        if reconnects >= 3 {
            return Err(GatewayError::SandboxError(format!(
                "LangGraph stream reconnect limit exceeded: {error}"
            )));
        }
        response = authenticated(
            state.http.get(format!(
                "{}/threads/{thread_id}/runs/{run_id}/stream",
                config.base
            )),
            &credential.api_key,
        )
        .header("accept", "text/event-stream")
        .header("last-event-id", cursor)
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
        if !response.status().is_success() {
            return Err(GatewayError::SandboxError(format!(
                "LangGraph stream reconnect returned HTTP {}",
                response.status().as_u16()
            )));
        }
        reconnects += 1;
    }
    for frame in decoder.finish()? {
        handle_frame(state, pool, row, credential, thread_id, &mut run, frame).await?;
    }
    if run.paused {
        return Ok(None);
    }
    let state_value = run
        .latest_state
        .unwrap_or_else(|| Value::Object(run.updates));
    persist_langgraph_artifacts(state, pool, row, credential, &state_value).await?;
    if config.output_path.is_empty() {
        return Ok(Some(state_value));
    }
    if let Some(result) = state_value.pointer(config.output_path).cloned() {
        return Ok(Some(result));
    }
    if !run.text.is_empty() {
        return Ok(Some(Value::String(run.text)));
    }
    Err(GatewayError::SandboxError(format!(
        "LangGraph response did not contain mapped field {}",
        config.output_path
    )))
}

async fn handle_frame(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    thread_id: &str,
    run: &mut LangGraphStreamState,
    frame: SseFrame,
) -> Result<(), GatewayError> {
    if let Some(cursor) = frame.id.as_deref() {
        run.last_event_id = Some(cursor.to_owned());
    }
    let (event, namespace) = split_event(&frame.event);
    if event == "metadata" {
        let run_id = frame.data.get("run_id").and_then(Value::as_str);
        run.run_id = run_id.map(str::to_owned);
        bind_remote(pool, row, thread_id, run_id, frame.id.as_deref()).await?;
        append_control_event(
            pool,
            row,
            None,
            event_key("metadata", &frame),
            "provider.event",
            json!({"provider_event": "metadata", "raw": frame.data}),
        )
        .await?;
        return Ok(());
    }
    bind_remote(pool, row, thread_id, None, frame.id.as_deref()).await?;
    match event {
        "messages" | "messages-tuple" => {
            if let Some(text) = message_text(&frame.data) {
                run.text.push_str(&text);
                append_control_event(
                    pool,
                    row,
                    None,
                    event_key("message", &frame),
                    "message.appended",
                    json!({"text": text}),
                )
                .await?;
                publish_runtime_event(
                    state,
                    pool,
                    &row.id,
                    json!({"type": "agent.message.delta", "text": text}),
                )
                .await?;
            }
        }
        "updates" => {
            if let Some(interrupts) = interrupts(&frame.data) {
                park_for_interrupt(pool, row, thread_id, &frame, interrupts).await?;
                run.paused = true;
            } else if let Some(object) = frame.data.as_object() {
                for (node, update) in object {
                    run.updates.insert(node.clone(), update.clone());
                    project_node(pool, row, run, node, namespace, update, &frame).await?;
                }
            }
        }
        "values" => {
            if let Some(interrupts) = interrupts(&frame.data) {
                park_for_interrupt(pool, row, thread_id, &frame, interrupts).await?;
                run.paused = true;
            }
            run.latest_state = Some(frame.data);
        }
        "custom" => {
            persist_langgraph_artifacts(state, pool, row, credential, &frame.data).await?;
            append_control_event(
                pool,
                row,
                None,
                event_key("custom", &frame),
                "provider.event",
                json!({"provider_event": "custom", "namespace": namespace, "raw": frame.data}),
            )
            .await?;
        }
        "error" => {
            return Err(GatewayError::SandboxError(error_message(
                &frame.data.to_string(),
            )));
        }
        "end" => {}
        _ => {
            append_control_event(
                pool,
                row,
                None,
                event_key(event, &frame),
                "provider.event",
                json!({"provider_event": event, "namespace": namespace, "raw": frame.data}),
            )
            .await?;
        }
    }
    Ok(())
}

async fn project_node(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    run: &mut LangGraphStreamState,
    node: &str,
    namespace: Option<&str>,
    update: &Value,
    frame: &SseFrame,
) -> Result<(), GatewayError> {
    let Some(snapshot) = session_control::repository::active_turn(pool, &row.id).await? else {
        return Ok(());
    };
    let Some(primary) = snapshot.invocations.first() else {
        return Ok(());
    };
    let key = namespace.map_or_else(
        || node.to_owned(),
        |namespace| format!("{namespace}|{node}"),
    );
    let child_id = if let Some(child_id) = run.child_invocations.get(&key) {
        child_id.clone()
    } else {
        let child = session_control::repository::create_child_invocation(
            pool,
            &snapshot.turn.id,
            session_control::repository::NewChildInvocation {
                parent_invocation_id: &primary.id,
                agent_id: primary.agent_id.as_deref(),
                agent_revision: primary.agent_revision,
                runtime: Some("langgraph_assistant"),
                protocol: "langgraph",
                protocol_version: "agent-server-v1",
                adapter_id: "langgraph_assistant",
                role: "workflow",
                metadata: &json!({"node": node, "namespace": namespace}),
            },
        )
        .await?;
        session_control::repository::transition_invocation(pool, &child.id, "running", None)
            .await?;
        append_step(
            pool,
            row,
            &child.id,
            &key,
            node,
            "step.started",
            "running",
            update,
            frame,
        )
        .await?;
        run.child_invocations.insert(key.clone(), child.id.clone());
        child.id
    };
    session_control::repository::transition_invocation(pool, &child_id, "completed", None).await?;
    append_step(
        pool,
        row,
        &child_id,
        &key,
        node,
        "step.completed",
        "completed",
        update,
        frame,
    )
    .await?;
    append_control_event(
        pool,
        row,
        Some(&child_id),
        format!("langgraph:progress:{}", event_identity(frame)),
        "turn.progress",
        json!({"mode": "steps", "label": format!("LangGraph 节点 {node} 已完成")}),
    )
    .await
}

async fn resolve_pending_interrupt(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    binding: &session_control::schema::SessionInvocationRow,
    input: &Value,
) -> Result<Value, GatewayError> {
    let events = session_control::repository::events_for_turn(pool, &binding.turn_id).await?;
    let Some(request) = events
        .iter()
        .rev()
        .find(|event| event.event_type == "input.requested")
    else {
        return Ok(input.clone());
    };
    let request_id = request
        .event_json
        .get("request_id")
        .and_then(Value::as_str)
        .unwrap_or("langgraph-interrupt");
    let resume_input = coerce_resume_input(input, request.event_json.get("raw"));
    append_control_event(
        pool,
        row,
        Some(&binding.id),
        format!("langgraph:interrupt:{request_id}:resolved"),
        "input.resolved",
        json!({"request_id": request_id}),
    )
    .await?;
    Ok(resume_input)
}

fn coerce_resume_input(input: &Value, raw: Option<&Value>) -> Value {
    let Some(input_object) = input.as_object() else {
        return input.clone();
    };
    let Some(raw) = raw else {
        return input.clone();
    };
    let Some(raw_object) = raw.as_object() else {
        return input_object
            .get("resume")
            .cloned()
            .unwrap_or_else(|| input.clone());
    };
    let mut coerced = input_object.clone();
    for (key, value) in &mut coerced {
        let Some(expected) = raw_object.get(key) else {
            continue;
        };
        let Value::String(text) = value else {
            continue;
        };
        *value = match expected {
            Value::Bool(_) => text
                .parse::<bool>()
                .map(Value::Bool)
                .unwrap_or_else(|_| value.clone()),
            Value::Number(_) => text
                .parse::<serde_json::Number>()
                .map(Value::Number)
                .unwrap_or_else(|_| value.clone()),
            Value::Array(_) | Value::Object(_) => {
                serde_json::from_str(text).unwrap_or_else(|_| value.clone())
            }
            _ => value.clone(),
        };
    }
    Value::Object(coerced)
}

#[allow(clippy::too_many_arguments)]
async fn append_step(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    invocation_id: &str,
    id: &str,
    label: &str,
    event_type: &str,
    status: &str,
    update: &Value,
    frame: &SseFrame,
) -> Result<(), GatewayError> {
    append_control_event(
        pool,
        row,
        Some(invocation_id),
        format!("langgraph:{event_type}:{id}:{}", event_identity(frame)),
        event_type,
        json!({
            "id": id,
            "label": label,
            "status": status,
            "metadata": {"update": update}
        }),
    )
    .await
}

async fn park_for_interrupt(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    thread_id: &str,
    frame: &SseFrame,
    interrupts: &[Value],
) -> Result<(), GatewayError> {
    let Some(snapshot) = session_control::repository::active_turn(pool, &row.id).await? else {
        return Ok(());
    };
    let interrupt = interrupts.first().cloned().unwrap_or(Value::Null);
    let request_id = interrupt
        .pointer("/ns/0")
        .and_then(Value::as_str)
        .unwrap_or("langgraph-interrupt")
        .to_owned();
    let value = interrupt.get("value").cloned().unwrap_or(interrupt);
    let fields = interrupt_fields(&value);
    session_control::repository::bind_active_invocation(
        pool,
        &row.id,
        Some(thread_id),
        checkpoint_id(&frame.data),
        None,
        frame.id.as_deref(),
    )
    .await?;
    append_control_event(
        pool,
        row,
        snapshot.invocations.first().map(|item| item.id.as_str()),
        format!("langgraph:interrupt:{request_id}"),
        "input.requested",
        json!({
            "request_id": request_id,
            "prompt": interrupt_prompt(&value),
            "fields": fields,
            "schema": interrupt_schema(&fields),
            "provider": {"thread_id": thread_id, "checkpoint_id": checkpoint_id(&frame.data)},
            "raw": value
        }),
    )
    .await?;
    session_control::repository::transition(pool, &snapshot.turn.id, "waiting_input", None).await?;
    sessions::repository::set_status(pool, &row.id, "waiting_input").await
}

fn interrupt_fields(value: &Value) -> Vec<Value> {
    let mut fields = value
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| !matches!(key.as_str(), "message" | "prompt" | "description"))
        .map(|(key, value)| {
            json!({
                "id": key,
                "label": key,
                "kind": if value.is_boolean() {"choice"} else {"text"},
                "required": true,
                "choices": if value.is_boolean() {json!(["true", "false"])} else {Value::Null}
            })
        })
        .collect::<Vec<_>>();
    if fields.is_empty() {
        fields.push(json!({
            "id": "resume",
            "label": "输入",
            "kind": "text",
            "required": true
        }));
    }
    fields
}

fn interrupt_schema(fields: &[Value]) -> Value {
    let properties = fields
        .iter()
        .filter_map(|field| {
            Some((
                field.get("id")?.as_str()?.to_owned(),
                json!({"type": ["string", "number", "boolean", "object", "array"]}),
            ))
        })
        .collect::<Map<_, _>>();
    let required = fields
        .iter()
        .filter_map(|field| field.get("id").cloned())
        .collect::<Vec<_>>();
    json!({"type": "object", "properties": properties, "required": required})
}

fn interrupt_prompt(value: &Value) -> String {
    value
        .get("prompt")
        .or_else(|| value.get("message"))
        .or_else(|| value.get("description"))
        .and_then(Value::as_str)
        .unwrap_or("LangGraph 执行已暂停，需要补充输入")
        .to_owned()
}

fn interrupts(value: &Value) -> Option<&[Value]> {
    value
        .get("__interrupt__")
        .or_else(|| value.get("interrupts"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .filter(|values| !values.is_empty())
}

async fn ensure_thread(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    config: &LangGraphConfig<'_>,
    trace: &TraceHeaders,
) -> Result<String, GatewayError> {
    if let Some(thread_id) = row.provider_session_id.as_deref() {
        return Ok(thread_id.to_owned());
    }
    let response = authenticated(
        trace.apply(state.http.post(format!("{}/threads", config.base))),
        &credential.api_key,
    )
    .json(&json!({}))
    .send()
    .await
    .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    let payload = json_response(response).await?;
    let thread_id = payload
        .get("thread_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            GatewayError::SandboxError("LangGraph did not return thread_id".to_owned())
        })?;
    sessions::repository::set_runtime_refs(
        pool,
        &row.id,
        row.runtime_agent_ref_id.as_deref().unwrap_or(&row.harness),
        Some(thread_id),
        None,
        "running",
    )
    .await?;
    session_control::repository::bind_active_invocation(
        pool,
        &row.id,
        Some(thread_id),
        None,
        None,
        None,
    )
    .await?;
    Ok(thread_id.to_owned())
}

async fn bind_remote(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    thread_id: &str,
    run_id: Option<&str>,
    cursor: Option<&str>,
) -> Result<(), GatewayError> {
    session_control::repository::bind_active_invocation(
        pool,
        &row.id,
        Some(thread_id),
        None,
        run_id,
        cursor,
    )
    .await?;
    if let Some(run_id) = run_id {
        sessions::repository::set_provider_run(pool, &row.id, run_id, "running").await?;
    }
    Ok(())
}

async fn append_control_event(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    invocation_id: Option<&str>,
    event_key: String,
    event_type: &str,
    event: Value,
) -> Result<(), GatewayError> {
    let Some(snapshot) = session_control::repository::active_turn(pool, &row.id).await? else {
        return Ok(());
    };
    session_control::repository::append_event(
        pool,
        session_control::repository::NewControlEvent {
            session_id: &row.id,
            turn_id: Some(&snapshot.turn.id),
            invocation_id,
            request_id: Some(&snapshot.turn.request_id),
            event_key: &event_key,
            event_type,
            event,
        },
    )
    .await?;
    Ok(())
}

async fn publish_runtime_event(
    state: &AppState,
    pool: &PgPool,
    session_id: &str,
    event: Value,
) -> Result<(), GatewayError> {
    runtime_events::repository::append(pool, session_id, event.clone()).await?;
    state.local_session_events.publish(session_id, event);
    Ok(())
}

async fn persist_langgraph_artifacts(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    value: &Value,
) -> Result<(), GatewayError> {
    let Some(snapshot) = session_control::repository::active_turn(pool, &row.id).await? else {
        return Ok(());
    };
    let mut files = Vec::new();
    collect_files(value, &mut files);
    let mut seen = HashSet::new();
    let adapter = DatabaseArtifactAdapter::new(pool.clone(), state.object_storage.clone());
    for file in files {
        let Some(raw_uri) = file
            .get("url")
            .or_else(|| file.get("uri"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let uri = absolute_uri(&credential.api_base, raw_uri);
        if !seen.insert(uri.clone()) {
            continue;
        }
        let reference = ArtifactReference {
            id: file.get("id").and_then(Value::as_str).map(str::to_owned),
            invocation_id: snapshot.invocations.first().map(|item| item.id.clone()),
            name: file
                .get("filename")
                .or_else(|| file.get("name"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            media_type: file
                .get("mime_type")
                .or_else(|| file.get("media_type"))
                .and_then(Value::as_str)
                .unwrap_or("application/octet-stream")
                .to_owned(),
            digest: None,
            size_bytes: file.get("size").and_then(Value::as_u64),
            uri: Some(uri),
            data_base64: None,
            metadata: json!({"provider": "langgraph", "raw": file}),
        };
        if let Ok(persisted) = adapter
            .persist(&row.id, &snapshot.turn.id, &reference)
            .await
        {
            if let Some(artifact_id) = persisted.id.as_deref() {
                if let Some(artifact) =
                    artifacts::repository::get(pool, &row.id, artifact_id).await?
                {
                    append_control_event(
                        pool,
                        row,
                        artifact.invocation_id.as_deref(),
                        format!("langgraph:artifact:{}", artifact.id),
                        "artifact.available",
                        json!({"artifact": artifact}),
                    )
                    .await?;
                }
            }
        }
    }
    Ok(())
}

fn collect_files<'a>(value: &'a Value, files: &mut Vec<&'a Map<String, Value>>) {
    match value {
        Value::Array(values) => values.iter().for_each(|value| collect_files(value, files)),
        Value::Object(object) => {
            let has_uri = object
                .get("url")
                .or_else(|| object.get("uri"))
                .and_then(Value::as_str)
                .is_some();
            let has_media = object.get("mime_type").is_some()
                || object.get("media_type").is_some()
                || object
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| {
                        matches!(kind, "file" | "image" | "document" | "audio" | "video")
                    });
            if has_uri && has_media {
                files.push(object);
            } else {
                object
                    .values()
                    .for_each(|value| collect_files(value, files));
            }
        }
        _ => {}
    }
}

fn config<'a>(
    source: &'a Value,
    credential_base: &'a str,
) -> Result<LangGraphConfig<'a>, GatewayError> {
    let assistant_id = source
        .get("external_agent_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            GatewayError::InvalidConfig("LangGraph 来源缺少 assistant_id。".to_owned())
        })?;
    let mapping = source.pointer("/raw/x-lap-runtime").ok_or_else(|| {
        GatewayError::InvalidConfig(
            "LangGraph 来源必须提供经过确认的输入/输出映射后才能执行。".to_owned(),
        )
    })?;
    Ok(LangGraphConfig {
        assistant_id,
        input_field: mapping
            .get("input_field")
            .and_then(Value::as_str)
            .unwrap_or("input"),
        output_path: mapping
            .get("output_path")
            .and_then(Value::as_str)
            .unwrap_or("/output"),
        base: mapping
            .get("base_url")
            .and_then(Value::as_str)
            .unwrap_or(credential_base)
            .trim_end_matches('/'),
    })
}

fn graph_input(input: &Value, input_field: &str, fallback_prompt: &str) -> Value {
    if input
        .as_object()
        .is_some_and(|object| object.contains_key(input_field))
    {
        return input.clone();
    }
    let mapped = if input_field == "messages" {
        json!([{"role": "user", "content": fallback_prompt}])
    } else {
        Value::String(fallback_prompt.to_owned())
    };
    json!({input_field: mapped})
}

fn authenticated(request: reqwest::RequestBuilder, api_key: &str) -> reqwest::RequestBuilder {
    if api_key.is_empty() {
        request
    } else {
        request.bearer_auth(api_key).header("x-api-key", api_key)
    }
}

async fn ensure_status(response: reqwest::Response) -> Result<(), GatewayError> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(())
    } else {
        Err(GatewayError::SandboxError(format!(
            "LangGraph returned HTTP {}: {}",
            status.as_u16(),
            error_message(&body)
        )))
    }
}

async fn json_response(response: reqwest::Response) -> Result<Value, GatewayError> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(GatewayError::SandboxError(format!(
            "LangGraph returned HTTP {}: {}",
            status.as_u16(),
            error_message(&body)
        )));
    }
    serde_json::from_str(&body)
        .map_err(|error| GatewayError::SandboxError(format!("invalid LangGraph JSON: {error}")))
}

fn message_text(value: &Value) -> Option<String> {
    let message = value
        .as_array()
        .and_then(|values| values.first())
        .unwrap_or(value);
    match message.get("content")? {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<String>();
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

fn checkpoint_id(value: &Value) -> Option<&str> {
    value
        .pointer("/checkpoint/checkpoint_id")
        .or_else(|| value.get("checkpoint_id"))
        .and_then(Value::as_str)
}

fn split_event(event: &str) -> (&str, Option<&str>) {
    event
        .split_once('|')
        .map_or((event, None), |(event, namespace)| (event, Some(namespace)))
}

fn event_key(kind: &str, frame: &SseFrame) -> String {
    format!("langgraph:{kind}:{}", event_identity(frame))
}

fn event_identity(frame: &SseFrame) -> String {
    frame
        .id
        .clone()
        .unwrap_or_else(|| crate::db::managed_agents::id("langgraph-event"))
}

fn error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .or_else(|| value.get("error"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.chars().take(500).collect())
}

fn absolute_uri(base: &str, uri: &str) -> String {
    reqwest::Url::parse(uri)
        .or_else(|_| {
            reqwest::Url::parse(&format!("{}/", base.trim_end_matches('/')))
                .and_then(|base| base.join(uri))
        })
        .map(|url| url.to_string())
        .unwrap_or_else(|_| uri.to_owned())
}

fn cancel_url(base: &str, thread_id: &str, run_id: &str) -> String {
    format!("{base}/threads/{thread_id}/runs/{run_id}/cancel?wait=true")
}

struct SseFrame {
    event: String,
    id: Option<String>,
    data: Value,
}

#[derive(Default)]
struct SseDecoder {
    buffer: String,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseFrame>, GatewayError> {
        let text = std::str::from_utf8(chunk).map_err(|error| {
            GatewayError::SandboxError(format!("invalid LangGraph SSE UTF-8: {error}"))
        })?;
        self.buffer.push_str(text);
        self.drain(false)
    }

    fn finish(&mut self) -> Result<Vec<SseFrame>, GatewayError> {
        self.drain(true)
    }

    fn drain(&mut self, finish: bool) -> Result<Vec<SseFrame>, GatewayError> {
        self.buffer = self.buffer.replace("\r\n", "\n");
        let mut frames = Vec::new();
        while let Some(index) = self.buffer.find("\n\n") {
            let raw = self.buffer[..index].to_owned();
            self.buffer.drain(..index + 2);
            if let Some(frame) = parse_frame(&raw)? {
                frames.push(frame);
            }
        }
        if finish && !self.buffer.trim().is_empty() {
            let raw = std::mem::take(&mut self.buffer);
            if let Some(frame) = parse_frame(&raw)? {
                frames.push(frame);
            }
        }
        Ok(frames)
    }
}

fn parse_frame(raw: &str) -> Result<Option<SseFrame>, GatewayError> {
    let event = raw
        .lines()
        .find_map(|line| line.strip_prefix("event:"))
        .map(str::trim)
        .unwrap_or("message")
        .to_owned();
    let id = raw
        .lines()
        .find_map(|line| line.strip_prefix("id:"))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned);
    let data = raw
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() || data == "[DONE]" {
        return Ok(None);
    }
    let data = serde_json::from_str(&data).map_err(|error| {
        GatewayError::SandboxError(format!("invalid LangGraph SSE JSON: {error}"))
    })?;
    Ok(Some(SseFrame { event, id, data }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        cancel_url, coerce_resume_input, collect_files, graph_input, interrupt_fields,
        message_text, split_event, SseDecoder,
    };

    #[test]
    fn decodes_fragmented_named_sse_frames() {
        let mut decoder = SseDecoder::default();
        assert!(decoder.push(b"id: cursor-1\nevent: up").unwrap().is_empty());
        let frames = decoder
            .push(b"dates\ndata: {\"research\":{\"answer\":\"done\"}}\n\n")
            .unwrap();
        assert_eq!(frames[0].event, "updates");
        assert_eq!(frames[0].id.as_deref(), Some("cursor-1"));
        assert_eq!(frames[0].data["research"]["answer"], "done");
    }

    #[test]
    fn extracts_message_tuple_text_and_subgraph_namespace() {
        assert_eq!(
            message_text(&json!([{"content": "token"}, {"langgraph_node": "writer"}])),
            Some("token".to_owned())
        );
        assert_eq!(
            split_event("updates|research:run-1"),
            ("updates", Some("research:run-1"))
        );
    }

    #[test]
    fn maps_interrupt_objects_to_generic_fields() {
        let fields = interrupt_fields(&json!({"prompt": "Approve?", "decision": ""}));
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0]["id"], "decision");
    }

    #[test]
    fn only_collects_file_shaped_values() {
        let value = json!({
            "source": {"url": "https://example.com"},
            "report": {"uri": "https://example.com/report.pdf", "media_type": "application/pdf"}
        });
        let mut files = Vec::new();
        collect_files(&value, &mut files);
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn preserves_structured_graph_input_and_coerces_resume_types() {
        assert_eq!(
            graph_input(&json!({"topic": "agents", "depth": 3}), "topic", "fallback"),
            json!({"topic": "agents", "depth": 3})
        );
        assert_eq!(
            coerce_resume_input(
                &json!({"approved": "true", "score": "2"}),
                Some(&json!({"approved": false, "score": 0}))
            ),
            json!({"approved": true, "score": 2})
        );
        assert_eq!(
            coerce_resume_input(&json!({"resume": "edited"}), Some(&json!("original"))),
            json!("edited")
        );
        assert_eq!(
            cancel_url("https://graph.example", "thread-1", "run-1"),
            "https://graph.example/threads/thread-1/runs/run-1/cancel?wait=true"
        );
    }
}
