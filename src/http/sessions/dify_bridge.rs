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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DifyAppMode {
    Chat,
    Completion,
    Workflow,
}

impl DifyAppMode {
    fn from_source(source: &Value) -> Self {
        match source
            .pointer("/raw/mode")
            .and_then(Value::as_str)
            .unwrap_or("chat")
        {
            mode if mode.contains("workflow") => Self::Workflow,
            mode if mode.contains("completion") => Self::Completion,
            _ => Self::Chat,
        }
    }

    fn invoke_path(self) -> &'static str {
        match self {
            Self::Chat => "/chat-messages",
            Self::Completion => "/completion-messages",
            Self::Workflow => "/workflows/run",
        }
    }

    fn stop_path(self, task_id: &str) -> String {
        match self {
            Self::Chat => format!("/chat-messages/{task_id}/stop"),
            Self::Completion => format!("/completion-messages/{task_id}/stop"),
            Self::Workflow => format!("/workflows/tasks/{task_id}/stop"),
        }
    }
}

#[derive(Default)]
struct DifyStreamState {
    text: String,
    result: Option<Value>,
    paused: bool,
    child_invocations: HashMap<String, String>,
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
    let mode = DifyAppMode::from_source(source);
    let user = dify_user(row);
    let mut body = json!({
        "inputs": input,
        "response_mode": "streaming",
        "user": user,
    });
    if mode != DifyAppMode::Workflow {
        body["query"] = Value::String(prompt.to_owned());
    }
    if mode == DifyAppMode::Chat {
        if let Some(conversation_id) = row.provider_session_id.as_deref() {
            body["conversation_id"] = Value::String(conversation_id.to_owned());
        }
    }
    let response = trace
        .apply(
            state
                .http
                .post(format!(
                    "{}{}",
                    credential.api_base.trim_end_matches('/'),
                    mode.invoke_path()
                ))
                .bearer_auth(&credential.api_key)
                .header("accept", "text/event-stream")
                .json(&body),
        )
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    consume_response(state, pool, row, credential, mode, response).await
}

pub(super) async fn cancel(
    state: &AppState,
    row: &sessions::schema::SessionRow,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    binding: &session_control::schema::SessionInvocationRow,
    trace: &TraceHeaders,
) -> Result<(), GatewayError> {
    let Some(task_id) = binding.remote_task_id.as_deref() else {
        return Ok(());
    };
    let path = DifyAppMode::from_source(source).stop_path(task_id);
    let response = trace
        .apply(
            state
                .http
                .post(format!(
                    "{}{}",
                    credential.api_base.trim_end_matches('/'),
                    path
                ))
                .bearer_auth(&credential.api_key)
                .json(&json!({"user": dify_user(row)})),
        )
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    ensure_status(response).await
}

pub(super) async fn resume(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    binding: &session_control::schema::SessionInvocationRow,
    input: &Value,
    trace: &TraceHeaders,
) -> Result<Option<Value>, GatewayError> {
    let form_token = binding.resume_cursor.as_deref().ok_or_else(|| {
        GatewayError::InvalidConfig("Dify continuation is missing form_token".to_owned())
    })?;
    let workflow_run_id = binding.remote_context_id.as_deref().ok_or_else(|| {
        GatewayError::InvalidConfig("Dify continuation is missing workflow_run_id".to_owned())
    })?;
    let mut values = input
        .as_object()
        .cloned()
        .ok_or_else(|| GatewayError::BadRequest("Dify Human Input must be an object".to_owned()))?;
    let action = values
        .remove("action")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| {
            GatewayError::BadRequest("Dify Human Input requires an action".to_owned())
        })?;
    normalize_human_input_values(pool, binding, &mut values).await?;
    let user = dify_user(row);
    let response = trace
        .apply(
            state
                .http
                .post(format!(
                    "{}/form/human_input/{}",
                    credential.api_base.trim_end_matches('/'),
                    form_token
                ))
                .bearer_auth(&credential.api_key)
                .json(&json!({"inputs": values, "action": action, "user": user})),
        )
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    ensure_status(response).await?;

    let response = trace
        .apply(
            state
                .http
                .get(format!(
                    "{}/workflow/{}/events",
                    credential.api_base.trim_end_matches('/'),
                    workflow_run_id
                ))
                .bearer_auth(&credential.api_key)
                .header("accept", "text/event-stream")
                .query(&[
                    ("user", user),
                    ("include_state_snapshot", "false"),
                    ("continue_on_pause", "false"),
                ]),
        )
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    consume_response(
        state,
        pool,
        row,
        credential,
        DifyAppMode::Workflow,
        response,
    )
    .await
}

async fn consume_response(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    mode: DifyAppMode,
    response: reqwest::Response,
) -> Result<Option<Value>, GatewayError> {
    let status = response.status();
    let is_stream = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/event-stream"));
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(GatewayError::SandboxError(format!(
            "Dify returned HTTP {}: {}",
            status.as_u16(),
            error_message(&body)
        )));
    }
    if !is_stream {
        let payload: Value = response
            .json()
            .await
            .map_err(|error| GatewayError::SandboxError(format!("invalid Dify JSON: {error}")))?;
        bind_remote_ids(pool, row, &payload).await?;
        return blocking_result(mode, payload).map(Some);
    }

    let mut decoder = SseDecoder::default();
    let mut stream = response.bytes_stream();
    let mut run = DifyStreamState::default();
    while let Some(chunk) = stream.next().await {
        if state.external_bridge_cancellations.is_cancelled(&row.id) {
            return Err(GatewayError::SandboxError(
                "Dify invocation cancelled".to_owned(),
            ));
        }
        let chunk = chunk.map_err(|error| GatewayError::SandboxError(error.to_string()))?;
        for payload in decoder.push(&chunk)? {
            handle_event(state, pool, row, credential, mode, &mut run, payload).await?;
        }
    }
    for payload in decoder.finish()? {
        handle_event(state, pool, row, credential, mode, &mut run, payload).await?;
    }
    if run.paused {
        return Ok(None);
    }
    if let Some(result) = run.result {
        return Ok(Some(result));
    }
    if !run.text.is_empty() {
        return Ok(Some(Value::String(run.text)));
    }
    Err(GatewayError::SandboxError(
        "Dify stream ended without a result".to_owned(),
    ))
}

#[allow(clippy::too_many_arguments)]
async fn handle_event(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    mode: DifyAppMode,
    run: &mut DifyStreamState,
    payload: Value,
) -> Result<(), GatewayError> {
    let event = payload
        .get("event")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if event == "ping" || event.is_empty() {
        return Ok(());
    }
    bind_remote_ids(pool, row, &payload).await?;
    match event {
        "message" | "agent_message" | "text_chunk" => {
            if let Some(text) = payload
                .get("answer")
                .or_else(|| payload.pointer("/data/text"))
                .and_then(Value::as_str)
            {
                run.text.push_str(text);
                append_control_event(
                    pool,
                    row,
                    None,
                    format!("dify:text:{}", remote_event_id(&payload)),
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
        "workflow_started" => {
            append_progress(pool, row, &payload, 0.0, None, "Dify 工作流已启动").await?;
        }
        "node_started" => {
            start_node(pool, row, run, &payload).await?;
        }
        "node_finished" => {
            finish_node(pool, row, run, &payload).await?;
        }
        "workflow_finished" => {
            let data = payload.get("data").cloned().unwrap_or(Value::Null);
            let status = data
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("succeeded");
            if matches!(status, "failed" | "stopped") {
                return Err(GatewayError::SandboxError(
                    data.get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("Dify workflow failed")
                        .to_owned(),
                ));
            }
            persist_dify_artifacts(state, pool, row, credential, &data).await?;
            run.result = data
                .get("outputs")
                .cloned()
                .or_else(|| (!run.text.is_empty()).then(|| Value::String(run.text.clone())));
            append_progress(
                pool,
                row,
                &payload,
                data.get("total_steps")
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0),
                data.get("total_steps").and_then(Value::as_f64),
                "Dify 工作流已完成",
            )
            .await?;
        }
        "message_end" => {
            if run.result.is_none() && !run.text.is_empty() {
                run.result = Some(Value::String(run.text.clone()));
            }
        }
        "human_input_required" => {
            park_for_human_input(pool, row, &payload).await?;
            run.paused = true;
        }
        "workflow_paused" => {
            run.paused = true;
        }
        "human_input_form_filled" => {
            append_control_event(
                pool,
                row,
                None,
                format!("dify:human-input-filled:{}", remote_event_id(&payload)),
                "input.resolved",
                json!({"request_id": payload.pointer("/data/form_id")}),
            )
            .await?;
        }
        "human_input_form_timeout" => {
            return Err(GatewayError::SandboxError(
                "Dify Human Input form expired".to_owned(),
            ));
        }
        "error" => {
            return Err(GatewayError::SandboxError(
                payload
                    .get("message")
                    .or_else(|| payload.pointer("/data/error"))
                    .and_then(Value::as_str)
                    .unwrap_or("Dify stream returned an error")
                    .to_owned(),
            ));
        }
        _ => {
            append_control_event(
                pool,
                row,
                None,
                format!("dify:event:{event}:{}", remote_event_id(&payload)),
                "provider.event",
                json!({"provider_event": event, "raw": payload}),
            )
            .await?;
        }
    }
    if mode == DifyAppMode::Workflow && run.paused {
        sessions::repository::set_status(pool, &row.id, "waiting_input").await?;
    }
    Ok(())
}

async fn start_node(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    run: &mut DifyStreamState,
    payload: &Value,
) -> Result<(), GatewayError> {
    let Some(snapshot) = session_control::repository::active_turn(pool, &row.id).await? else {
        return Ok(());
    };
    let Some(primary) = snapshot.invocations.first() else {
        return Ok(());
    };
    let data = payload.get("data").unwrap_or(&Value::Null);
    let remote_id = node_execution_id(data);
    let child = session_control::repository::create_child_invocation(
        pool,
        &snapshot.turn.id,
        session_control::repository::NewChildInvocation {
            parent_invocation_id: &primary.id,
            agent_id: primary.agent_id.as_deref(),
            agent_revision: primary.agent_revision,
            runtime: Some("dify_app"),
            protocol: "dify",
            protocol_version: "service-api-v1",
            adapter_id: "dify_app",
            role: "workflow",
            metadata: &json!({
                "remote_node_execution_id": remote_id,
                "node_id": data.get("node_id"),
                "node_type": data.get("node_type"),
                "title": data.get("title")
            }),
        },
    )
    .await?;
    session_control::repository::transition_invocation(pool, &child.id, "running", None).await?;
    run.child_invocations
        .insert(remote_id.to_owned(), child.id.clone());
    append_step_event(
        pool,
        row,
        Some(&child.id),
        payload,
        "step.started",
        "running",
    )
    .await
}

async fn finish_node(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    run: &mut DifyStreamState,
    payload: &Value,
) -> Result<(), GatewayError> {
    let data = payload.get("data").unwrap_or(&Value::Null);
    let remote_id = node_execution_id(data);
    let status = data
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("succeeded");
    let terminal = if matches!(status, "failed" | "error") {
        "failed"
    } else {
        "completed"
    };
    let invocation_id = run.child_invocations.get(remote_id).cloned();
    if let Some(invocation_id) = invocation_id.as_deref() {
        session_control::repository::transition_invocation(
            pool,
            invocation_id,
            terminal,
            (terminal == "failed").then(|| {
                json!({"message": data.get("error").and_then(Value::as_str).unwrap_or("Dify node failed")})
            }),
        )
        .await?;
    }
    append_step_event(
        pool,
        row,
        invocation_id.as_deref(),
        payload,
        if terminal == "failed" {
            "step.failed"
        } else {
            "step.completed"
        },
        terminal,
    )
    .await?;
    let current = data.get("index").and_then(Value::as_f64).unwrap_or(1.0);
    append_progress(pool, row, payload, current, None, "Dify 节点执行中").await
}

async fn park_for_human_input(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    payload: &Value,
) -> Result<(), GatewayError> {
    let data = payload.get("data").unwrap_or(&Value::Null);
    let form_token = data
        .get("form_token")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            GatewayError::SandboxError(
                "Dify Human Input requires WebApp delivery and a form_token".to_owned(),
            )
        })?;
    let workflow_run_id = payload.get("workflow_run_id").and_then(Value::as_str);
    session_control::repository::bind_active_invocation(
        pool,
        &row.id,
        None,
        workflow_run_id,
        payload.get("task_id").and_then(Value::as_str),
        Some(form_token),
    )
    .await?;
    let Some(snapshot) = session_control::repository::active_turn(pool, &row.id).await? else {
        return Ok(());
    };
    let request_id = data
        .get("form_id")
        .and_then(Value::as_str)
        .unwrap_or(form_token);
    let fields = human_input_fields(data);
    append_control_event(
        pool,
        row,
        snapshot.invocations.first().map(|item| item.id.as_str()),
        format!("dify:human-input:{request_id}"),
        "input.requested",
        json!({
            "request_id": request_id,
            "prompt": data.get("form_content").or_else(|| data.get("node_title")),
            "fields": fields,
            "schema": human_input_schema(&fields),
            "provider": {"form_token": form_token, "workflow_run_id": workflow_run_id}
        }),
    )
    .await?;
    session_control::repository::transition(pool, &snapshot.turn.id, "waiting_input", None).await?;
    sessions::repository::set_status(pool, &row.id, "waiting_input").await?;
    Ok(())
}

fn human_input_fields(data: &Value) -> Vec<Value> {
    let mut fields = data
        .get("inputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|field| {
            let id = field.get("output_variable_name")?.as_str()?;
            let source_kind = field.get("type").and_then(Value::as_str).unwrap_or("text");
            let kind = if source_kind.contains("file") {
                "file"
            } else if source_kind.contains("select") {
                "choice"
            } else {
                "text"
            };
            Some(json!({
                "id": id,
                "label": field.get("label").and_then(Value::as_str).unwrap_or(id),
                "kind": kind,
                "required": field.get("required").and_then(Value::as_bool).unwrap_or(false),
                "choices": field.get("options")
            }))
        })
        .collect::<Vec<_>>();
    let actions = data
        .get("actions")
        .or_else(|| data.get("user_actions"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !actions.is_empty() {
        fields.push(json!({
            "id": "action",
            "label": "操作",
            "kind": "choice",
            "required": true,
            "choices": actions.iter().filter_map(|action| action.get("id").cloned()).collect::<Vec<_>>()
        }));
    }
    fields
}

fn human_input_schema(fields: &[Value]) -> Value {
    let properties = fields
        .iter()
        .filter_map(|field| {
            let id = field.get("id")?.as_str()?.to_owned();
            let property = match field.get("kind").and_then(Value::as_str) {
                Some("file") => json!({"type": ["object", "array", "string"]}),
                _ => json!({"type": "string"}),
            };
            Some((id, property))
        })
        .collect::<Map<_, _>>();
    let required = fields
        .iter()
        .filter(|field| field.get("required").and_then(Value::as_bool) == Some(true))
        .filter_map(|field| field.get("id").cloned())
        .collect::<Vec<_>>();
    json!({"type": "object", "properties": properties, "required": required})
}

async fn normalize_human_input_values(
    pool: &PgPool,
    binding: &session_control::schema::SessionInvocationRow,
    values: &mut Map<String, Value>,
) -> Result<(), GatewayError> {
    let events = session_control::repository::events_for_turn(pool, &binding.turn_id).await?;
    let file_fields = events
        .iter()
        .rev()
        .find(|event| event.event_type == "input.requested")
        .and_then(|event| event.event_json.get("fields"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|field| field.get("kind").and_then(Value::as_str) == Some("file"))
        .filter_map(|field| field.get("id").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<HashSet<_>>();

    for field in file_fields {
        let Some(value) = values.get_mut(&field) else {
            continue;
        };
        *value = normalize_file_input(value.take())?;
    }
    Ok(())
}

fn normalize_file_input(value: Value) -> Result<Value, GatewayError> {
    match value {
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                return serde_json::from_str(trimmed).map_err(|error| {
                    GatewayError::BadRequest(format!(
                        "Dify file input must be valid JSON or an HTTP(S) URL: {error}"
                    ))
                });
            }
            if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                return Ok(json!({
                    "type": "custom",
                    "transfer_method": "remote_url",
                    "url": trimmed
                }));
            }
            Err(GatewayError::BadRequest(
                "Dify file input must be a remote HTTP(S) URL or a Dify file object".to_owned(),
            ))
        }
        Value::Object(_) | Value::Array(_) => Ok(value),
        _ => Err(GatewayError::BadRequest(
            "Dify file input must be an object, array, or URL string".to_owned(),
        )),
    }
}

async fn append_step_event(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    invocation_id: Option<&str>,
    payload: &Value,
    event_type: &str,
    status: &str,
) -> Result<(), GatewayError> {
    let data = payload.get("data").unwrap_or(&Value::Null);
    let id = node_execution_id(data);
    append_control_event(
        pool,
        row,
        invocation_id,
        format!("dify:{event_type}:{id}"),
        event_type,
        json!({
            "id": id,
            "label": data.get("title").and_then(Value::as_str).unwrap_or(id),
            "status": status,
            "index": data.get("index"),
            "metadata": {"node_id": data.get("node_id"), "node_type": data.get("node_type")}
        }),
    )
    .await
}

async fn append_progress(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    payload: &Value,
    current: f64,
    total: Option<f64>,
    label: &str,
) -> Result<(), GatewayError> {
    append_control_event(
        pool,
        row,
        None,
        format!("dify:progress:{}", remote_event_id(payload)),
        "turn.progress",
        json!({"mode": if total.is_some() {"steps"} else {"status"}, "label": label, "current": current, "total": total}),
    )
    .await
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

async fn bind_remote_ids(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    payload: &Value,
) -> Result<(), GatewayError> {
    let conversation_id = payload.get("conversation_id").and_then(Value::as_str);
    let workflow_run_id = payload.get("workflow_run_id").and_then(Value::as_str);
    let task_id = payload.get("task_id").and_then(Value::as_str);
    if conversation_id.is_none() && workflow_run_id.is_none() && task_id.is_none() {
        return Ok(());
    }
    session_control::repository::bind_active_invocation(
        pool,
        &row.id,
        conversation_id,
        workflow_run_id,
        task_id,
        None,
    )
    .await?;
    if let Some(remote_id) = task_id.or(workflow_run_id).or(conversation_id) {
        sessions::repository::set_provider_run(pool, &row.id, remote_id, "running").await?;
    }
    Ok(())
}

fn blocking_result(mode: DifyAppMode, payload: Value) -> Result<Value, GatewayError> {
    match mode {
        DifyAppMode::Workflow => {
            let data = payload.get("data").unwrap_or(&payload);
            if data.get("status").and_then(Value::as_str) == Some("failed") {
                return Err(GatewayError::SandboxError(
                    data.get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("Dify workflow failed")
                        .to_owned(),
                ));
            }
            data.get("outputs").cloned().ok_or_else(|| {
                GatewayError::SandboxError("Dify workflow did not contain outputs".to_owned())
            })
        }
        _ => payload.get("answer").cloned().ok_or_else(|| {
            GatewayError::SandboxError("Dify response did not contain answer".to_owned())
        }),
    }
}

async fn persist_dify_artifacts(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    data: &Value,
) -> Result<(), GatewayError> {
    let Some(snapshot) = session_control::repository::active_turn(pool, &row.id).await? else {
        return Ok(());
    };
    let mut files = Vec::new();
    collect_files(data, &mut files);
    let mut seen = HashSet::new();
    let adapter = DatabaseArtifactAdapter::new(pool.clone(), state.object_storage.clone());
    for file in files {
        let Some(raw_uri) = file.get("url").and_then(Value::as_str) else {
            continue;
        };
        let uri = absolute_uri(&credential.api_base, raw_uri);
        if !seen.insert(uri.clone()) {
            continue;
        }
        let source_id = file
            .get("id")
            .or_else(|| file.get("related_id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| uri.clone());
        let reference = ArtifactReference {
            id: Some(source_id),
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
            metadata: json!({"provider": "dify", "raw": file}),
        };
        match adapter
            .persist(&row.id, &snapshot.turn.id, &reference)
            .await
        {
            Ok(persisted) => {
                if let Some(artifact_id) = persisted.id.as_deref() {
                    if let Some(artifact) =
                        artifacts::repository::get(pool, &row.id, artifact_id).await?
                    {
                        append_control_event(
                            pool,
                            row,
                            artifact.invocation_id.as_deref(),
                            format!("dify:artifact:{}", artifact.id),
                            "artifact.available",
                            json!({"artifact": artifact}),
                        )
                        .await?;
                    }
                }
            }
            Err(error) => {
                tracing::warn!(session_id = %row.id, %error, "failed to persist Dify artifact")
            }
        }
    }
    Ok(())
}

fn collect_files<'a>(value: &'a Value, files: &mut Vec<&'a Map<String, Value>>) {
    match value {
        Value::Array(values) => values.iter().for_each(|value| collect_files(value, files)),
        Value::Object(object) => {
            let looks_like_file = object.get("url").and_then(Value::as_str).is_some()
                && (object.get("mime_type").is_some()
                    || object.get("media_type").is_some()
                    || object
                        .get("type")
                        .and_then(Value::as_str)
                        .is_some_and(|kind| {
                            matches!(kind, "image" | "document" | "audio" | "video" | "custom")
                        }));
            if looks_like_file {
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

async fn ensure_status(response: reqwest::Response) -> Result<(), GatewayError> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(())
    } else {
        Err(GatewayError::SandboxError(format!(
            "Dify returned HTTP {}: {}",
            status.as_u16(),
            error_message(&body)
        )))
    }
}

fn error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.chars().take(500).collect())
}

fn dify_user(row: &sessions::schema::SessionRow) -> &str {
    row.owner_id.as_deref().unwrap_or("lap-user")
}

fn node_execution_id(data: &Value) -> &str {
    data.get("id")
        .or_else(|| data.get("node_id"))
        .and_then(Value::as_str)
        .unwrap_or("dify-node")
}

fn remote_event_id(payload: &Value) -> String {
    payload
        .get("id")
        .or_else(|| payload.pointer("/data/id"))
        .or_else(|| payload.pointer("/data/node_id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| crate::db::managed_agents::id("dify-event"))
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

#[derive(Default)]
struct SseDecoder {
    buffer: String,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<Value>, GatewayError> {
        let text = std::str::from_utf8(chunk).map_err(|error| {
            GatewayError::SandboxError(format!("invalid Dify SSE UTF-8: {error}"))
        })?;
        self.buffer.push_str(text);
        self.drain(false)
    }

    fn finish(&mut self) -> Result<Vec<Value>, GatewayError> {
        self.drain(true)
    }

    fn drain(&mut self, finish: bool) -> Result<Vec<Value>, GatewayError> {
        self.buffer = self.buffer.replace("\r\n", "\n");
        let mut frames = Vec::new();
        while let Some(index) = self.buffer.find("\n\n") {
            let frame = self.buffer[..index].to_owned();
            self.buffer.drain(..index + 2);
            if let Some(value) = parse_frame(&frame)? {
                frames.push(value);
            }
        }
        if finish && !self.buffer.trim().is_empty() {
            let frame = std::mem::take(&mut self.buffer);
            if let Some(value) = parse_frame(&frame)? {
                frames.push(value);
            }
        }
        Ok(frames)
    }
}

fn parse_frame(frame: &str) -> Result<Option<Value>, GatewayError> {
    let data = frame
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() || data == "[DONE]" {
        return Ok(None);
    }
    serde_json::from_str(&data)
        .map(Some)
        .map_err(|error| GatewayError::SandboxError(format!("invalid Dify SSE JSON: {error}")))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{collect_files, human_input_fields, normalize_file_input, DifyAppMode, SseDecoder};

    #[test]
    fn detects_app_modes() {
        assert_eq!(
            DifyAppMode::from_source(&json!({"raw": {"mode": "workflow"}})),
            DifyAppMode::Workflow
        );
        assert_eq!(
            DifyAppMode::from_source(&json!({"raw": {"mode": "advanced-chat"}})),
            DifyAppMode::Chat
        );
        assert_eq!(
            DifyAppMode::from_source(&json!({"raw": {"mode": "completion"}})),
            DifyAppMode::Completion
        );
        assert_eq!(
            DifyAppMode::Workflow.stop_path("task-1"),
            "/workflows/tasks/task-1/stop"
        );
        assert_eq!(
            DifyAppMode::Chat.stop_path("task-1"),
            "/chat-messages/task-1/stop"
        );
    }

    #[test]
    fn decodes_fragmented_sse_frames() {
        let mut decoder = SseDecoder::default();
        assert!(decoder
            .push(b"data: {\"event\":\"text_")
            .unwrap()
            .is_empty());
        let events = decoder
            .push(b"chunk\",\"data\":{\"text\":\"hello\"}}\n\n")
            .unwrap();
        assert_eq!(events[0]["event"], "text_chunk");
        assert_eq!(events[0]["data"]["text"], "hello");
    }

    #[test]
    fn maps_human_input_fields_and_actions() {
        let fields = human_input_fields(&json!({
            "inputs": [{"type": "paragraph", "output_variable_name": "comment", "required": true}],
            "actions": [{"id": "approve", "title": "Approve"}, {"id": "reject", "title": "Reject"}]
        }));
        assert_eq!(fields[0]["id"], "comment");
        assert_eq!(fields[1]["id"], "action");
        assert_eq!(fields[1]["choices"], json!(["approve", "reject"]));
    }

    #[test]
    fn only_collects_file_shaped_urls() {
        let value = json!({
            "website": {"url": "https://example.com"},
            "files": [{"url": "https://example.com/report.pdf", "mime_type": "application/pdf"}]
        });
        let mut files = Vec::new();
        collect_files(&value, &mut files);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["mime_type"], "application/pdf");
    }

    #[test]
    fn converts_remote_url_file_inputs_to_dify_objects() {
        assert_eq!(
            normalize_file_input(json!("https://files.example/report.pdf")).unwrap(),
            json!({
                "type": "custom",
                "transfer_method": "remote_url",
                "url": "https://files.example/report.pdf"
            })
        );
        assert!(normalize_file_input(json!("artifact-123")).is_err());
    }
}
