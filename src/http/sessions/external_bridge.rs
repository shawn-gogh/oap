use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;

use crate::{
    db::managed_agents::{registry, runtime_events, session_control, sessions},
    errors::GatewayError,
    proxy::state::AppState,
};

use super::{runtime_lifecycle, storage::persist_message};

const A2A_SPEC: &str = "a2a_v1";
const DIFY_SPEC: &str = "dify_app";
const OPENAPI_SPEC: &str = "openapi_rest";
const LANGGRAPH_SPEC: &str = "langgraph_assistant";
const ACP_SPEC: &str = "acp_legacy";

pub(crate) fn supports(runtime: &str) -> bool {
    matches!(
        runtime,
        A2A_SPEC | DIFY_SPEC | OPENAPI_SPEC | LANGGRAPH_SPEC | ACP_SPEC
    )
}

pub(super) async fn execute_prompt(
    state: Arc<AppState>,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    prompt: &str,
) -> Result<(), GatewayError> {
    // Preparation failures (missing agent, unresolved credential, …) must
    // surface in the chat stream like any downstream error: without the
    // `session.error` event and the status reset the UI spins on a busy
    // session forever while the turn silently fails in the background.
    let prep = async {
        let agent = load_agent(pool, row).await?;
        agent_source(&agent)?;
        let credential =
            crate::http::runtime_resolution::imported_agent_credential(pool, &state, &agent, row)
                .await?;
        let trace = active_trace_headers(pool, &row.id).await?;
        Ok::<_, GatewayError>((agent, credential, trace))
    }
    .await;
    let (agent, credential, trace) = match prep {
        Ok(prep) => prep,
        Err(error) => {
            append_event(
                &state,
                pool,
                &row.id,
                json!({"type": "session.error", "error": {"message": error.to_string()}}),
            )
            .await?;
            runtime_lifecycle::mark_session_error(&state, pool, &row.id, error.to_string()).await?;
            return Err(error);
        }
    };
    let source = agent_source(&agent)?;
    let spec = source
        .get("api_spec")
        .and_then(Value::as_str)
        .or(row.runtime.as_deref())
        .unwrap_or_default();
    state.external_bridge_cancellations.clear(&row.id);
    sessions::repository::set_status(pool, &row.id, "running").await?;
    append_event(
        &state,
        pool,
        &row.id,
        json!({
            "type": "user.message",
            "content": [{"type": "text", "text": prompt}]
        }),
    )
    .await?;

    let response = match spec {
        A2A_SPEC => {
            invoke_a2a(
                &state,
                pool,
                row,
                source,
                &credential,
                prompt,
                &agent.name,
                &trace,
            )
            .await
        }
        DIFY_SPEC => invoke_dify(&state, pool, row, source, &credential, prompt, &trace)
            .await
            .map(Some),
        OPENAPI_SPEC => invoke_openapi(&state, source, &credential, prompt, &trace)
            .await
            .map(Some),
        LANGGRAPH_SPEC => invoke_langgraph(&state, source, &credential, prompt, &trace)
            .await
            .map(Some),
        ACP_SPEC => Err(GatewayError::InvalidConfig(
            "ACP 接入必须选择并验证具体兼容配置后才能执行。".to_owned(),
        )),
        other => Err(GatewayError::InvalidConfig(format!(
            "unsupported external bridge: {other}"
        ))),
    };

    match response {
        // A2A task paused on `input-required`/`auth-required`: an
        // `a2a_continuation` inbox item was already created and the turn
        // already moved to `waiting_approval` inside `poll_a2a_task`. Nothing
        // left to do here — `resolve_continuation` picks it back up once the
        // approval is decided.
        Ok(None) => Ok(()),
        Ok(Some(reply)) => {
            persist_message(pool, &row.id, "assistant", &reply, Some("stop")).await?;
            append_event(
                &state,
                pool,
                &row.id,
                json!({
                    "type": "agent.message",
                    "content": [{"type": "text", "text": reply}]
                }),
            )
            .await?;
            append_event(
                &state,
                pool,
                &row.id,
                json!({"type": "session.status_idle", "stop_reason": {"type": "end_turn"}}),
            )
            .await?;
            runtime_lifecycle::mark_session_status(&state, pool, &row.id, "idle", None).await
        }
        Err(error) => {
            if let Some(snapshot) = session_control::repository::active_turn(pool, &row.id).await? {
                if snapshot.turn.status == "cancelling" {
                    session_control::repository::transition(
                        pool,
                        &snapshot.turn.id,
                        "cancelled",
                        Some(
                            json!({"code": "cancelled", "message": "remote invocation cancelled"}),
                        ),
                    )
                    .await?;
                    sessions::repository::set_status(pool, &row.id, "idle").await?;
                    return Ok(());
                }
            }
            append_event(
                &state,
                pool,
                &row.id,
                json!({"type": "session.error", "error": {"message": error.to_string()}}),
            )
            .await?;
            runtime_lifecycle::mark_session_error(&state, pool, &row.id, error.to_string()).await?;
            Err(error)
        }
    }
}

pub(super) async fn cancel(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
) -> Result<(), GatewayError> {
    // Signal the detached poller first, unconditionally: it has no
    // `JoinHandle` to abort, so this flag is the only way to stop it before
    // it rides out the full remote poll timeout. Set even if the invocation
    // was never bound to a remote task yet (see `execute_prompt`'s in-flight
    // `message/send` case).
    state.external_bridge_cancellations.cancel(&row.id);
    let agent = load_agent(pool, row).await?;
    let source = agent_source(&agent)?;
    let spec = source
        .get("api_spec")
        .and_then(Value::as_str)
        .or(row.runtime.as_deref())
        .unwrap_or_default();
    let credential =
        crate::http::runtime_resolution::imported_agent_credential(pool, state, &agent, row)
            .await?;
    let binding = session_control::repository::active_turn(pool, &row.id)
        .await?
        .and_then(|snapshot| snapshot.invocations.into_iter().next());
    let Some(binding) = binding else {
        return Ok(());
    };
    let trace = TraceHeaders::from_metadata(&binding.metadata);
    match spec {
        A2A_SPEC => {
            let Some(task_id) = binding.remote_task_id.as_deref() else {
                return Ok(());
            };
            let rpc_url = validated_endpoint(&a2a_rpc_url(source, &credential.api_base)).await?;
            let response = trace
                .apply(
                    state
                        .http
                        .post(rpc_url)
                        .bearer_auth(&credential.api_key)
                        .json(&json!({
                            "jsonrpc": "2.0",
                            "id": crate::db::managed_agents::id("rpc"),
                            "method": "tasks/cancel",
                            "params": {"id": task_id}
                        })),
                )
                .send()
                .await
                .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
            ensure_success(response).await.map(|_| ())
        }
        DIFY_SPEC => {
            let Some(task_id) = binding.remote_task_id.as_deref() else {
                return Ok(());
            };
            let response = trace
                .apply(
                    state
                        .http
                        .post(format!(
                            "{}/chat-messages/{}/stop",
                            credential.api_base.trim_end_matches('/'),
                            task_id
                        ))
                        .bearer_auth(&credential.api_key)
                        .json(&json!({"user": row.owner_id.as_deref().unwrap_or("lap-user")})),
                )
                .send()
                .await
                .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
            ensure_success(response).await.map(|_| ())
        }
        _ => Ok(()),
    }
}

/// Outcome of an A2A invocation or continuation. `AwaitingApproval` means the
/// task hit `input-required`/`auth-required`: an `a2a_continuation` inbox
/// item was created and the turn parked at `waiting_approval` — the caller
/// has nothing further to do until `resolve_continuation` runs. `Cancelled`
/// means the user aborted the turn while the poller was running — the
/// turn/session state was already finalized by `abort_session_internal`, so
/// the caller has nothing further to do either.
enum A2aOutcome {
    Completed(String),
    AwaitingApproval,
    Cancelled,
}

#[allow(clippy::too_many_arguments)]
async fn invoke_a2a(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    prompt: &str,
    agent_name: &str,
    trace: &TraceHeaders,
) -> Result<Option<String>, GatewayError> {
    let request_id = crate::db::managed_agents::id("rpc");
    let rpc_url = validated_endpoint(&a2a_rpc_url(source, &credential.api_base)).await?;
    let response = trace
        .apply(
            state
                .http
                .post(&rpc_url)
                .bearer_auth(&credential.api_key)
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "method": "message/send",
                    "params": {
                        "message": {
                            "kind": "message",
                            "role": "user",
                            "messageId": crate::db::managed_agents::id("msg"),
                            "parts": [{"kind": "text", "text": prompt}]
                        }
                    }
                })),
        )
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    let payload = ensure_success(response).await?;
    if payload.get("error").is_some() {
        return Err(GatewayError::SandboxError("A2A request failed".to_owned()));
    }
    let result = payload.get("result").cloned().unwrap_or(Value::Null);
    let task_id = result.get("id").and_then(Value::as_str);
    let context_id = result
        .get("contextId")
        .or_else(|| result.get("context_id"))
        .and_then(Value::as_str);
    session_control::repository::bind_active_invocation(
        pool, &row.id, None, context_id, task_id, None,
    )
    .await?;
    if let Some(text) = extract_text(&result) {
        return Ok(Some(text));
    }
    let task_id = task_id.ok_or_else(|| {
        GatewayError::SandboxError("A2A response did not contain a message or task".to_owned())
    })?;
    match poll_a2a_task(
        state, pool, row, agent_name, credential, &rpc_url, task_id, context_id, trace,
    )
    .await?
    {
        A2aOutcome::Completed(text) => Ok(Some(text)),
        A2aOutcome::AwaitingApproval => Ok(None),
        A2aOutcome::Cancelled => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
async fn poll_a2a_task(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    agent_name: &str,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    rpc_url: &str,
    task_id: &str,
    context_id: Option<&str>,
    trace: &TraceHeaders,
) -> Result<A2aOutcome, GatewayError> {
    for _ in 0..120 {
        if state.external_bridge_cancellations.is_cancelled(&row.id) {
            return Ok(A2aOutcome::Cancelled);
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if state.external_bridge_cancellations.is_cancelled(&row.id) {
            return Ok(A2aOutcome::Cancelled);
        }
        let response = trace
            .apply(
                state
                    .http
                    .post(rpc_url)
                    .bearer_auth(&credential.api_key)
                    .json(&json!({
                        "jsonrpc": "2.0",
                        "id": crate::db::managed_agents::id("rpc"),
                        "method": "tasks/get",
                        "params": {"id": task_id}
                    })),
            )
            .send()
            .await
            .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
        let payload = ensure_success(response).await?;
        if payload.get("error").is_some() {
            return Err(GatewayError::SandboxError(
                "A2A task lookup failed".to_owned(),
            ));
        }
        let task = payload.get("result").unwrap_or(&Value::Null);
        let state_name = task
            .pointer("/status/state")
            .and_then(Value::as_str)
            .unwrap_or("working");
        match state_name {
            "completed" => {
                return extract_text(task)
                    .map(A2aOutcome::Completed)
                    .ok_or_else(|| {
                        GatewayError::SandboxError(
                            "completed A2A task did not contain a text result".to_owned(),
                        )
                    });
            }
            "failed" | "canceled" | "cancelled" | "rejected" => {
                return Err(GatewayError::SandboxError(format!(
                    "A2A task ended with state {state_name}"
                )));
            }
            "input-required" | "auth-required" => {
                let question = extract_text(task);
                pause_for_continuation(
                    state,
                    pool,
                    row,
                    agent_name,
                    task_id,
                    context_id,
                    state_name,
                    question.as_deref(),
                )
                .await?;
                return Ok(A2aOutcome::AwaitingApproval);
            }
            _ => {}
        }
    }
    Err(GatewayError::SandboxError(
        "A2A task did not reach a terminal state before the bridge deadline".to_owned(),
    ))
}

/// Creates the `a2a_continuation` inbox approval that lets a human resolve a
/// task paused on `input-required`/`auth-required`, and pushes the
/// chat-visible `approval.asked` event the same way `tool_approvals` does
/// for a runtime permission request. The turn itself is moved to
/// `waiting_approval` by `create_approval`'s active-turn binding.
#[allow(clippy::too_many_arguments)]
async fn pause_for_continuation(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    agent_name: &str,
    task_id: &str,
    context_id: Option<&str>,
    state_name: &str,
    question: Option<&str>,
) -> Result<(), GatewayError> {
    let kind_label = if state_name == "auth-required" {
        "需要重新鉴权"
    } else {
        "需要补充信息"
    };
    let title = format!("远程 A2A 任务{kind_label}：{agent_name}");
    let body = question.map(str::to_owned).unwrap_or_else(|| {
        format!("远程智能体的任务进入了 {state_name} 状态，需要人工提供后续输入才能继续。")
    });
    let item = crate::db::managed_agents::inbox::repository::create_approval(
        pool,
        "a2a_continuation",
        title,
        Some(row.id.clone()),
        Some(agent_name.to_owned()),
        Some(body),
        Some(json!({
            "task_id": task_id,
            "context_id": context_id,
            "state": state_name,
        })),
    )
    .await?;
    append_event(
        state,
        pool,
        &row.id,
        json!({
            "type": "approval.asked",
            "approval": {
                "id": item.id,
                "kind": item.kind,
                "title": item.title,
                "session_id": item.session_id,
                "args_json": item.args_json,
                "created_at": item.created_at,
            }
        }),
    )
    .await
}

/// Resumes a task paused on `input-required`/`auth-required` by re-sending
/// `message/send` against the same `taskId` with the human's reply. May pause
/// again (multi-round clarification) — `poll_a2a_task` handles that the same
/// way as the first round.
#[allow(clippy::too_many_arguments)]
async fn resume_a2a_task(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    agent_name: &str,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    rpc_url: &str,
    task_id: &str,
    context_id: Option<&str>,
    reply_text: &str,
    trace: &TraceHeaders,
) -> Result<A2aOutcome, GatewayError> {
    let response = trace
        .apply(
            state
                .http
                .post(rpc_url)
                .bearer_auth(&credential.api_key)
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": crate::db::managed_agents::id("rpc"),
                    "method": "message/send",
                    "params": {
                        "taskId": task_id,
                        "message": {
                            "kind": "message",
                            "role": "user",
                            "messageId": crate::db::managed_agents::id("msg"),
                            "parts": [{"kind": "text", "text": reply_text}]
                        }
                    }
                })),
        )
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    let payload = ensure_success(response).await?;
    if payload.get("error").is_some() {
        return Err(GatewayError::SandboxError(
            "A2A continuation request failed".to_owned(),
        ));
    }
    let result = payload.get("result").cloned().unwrap_or(Value::Null);
    if let Some(text) = extract_text(&result) {
        return Ok(A2aOutcome::Completed(text));
    }
    let next_task_id = result.get("id").and_then(Value::as_str).unwrap_or(task_id);
    let next_context_id = result
        .get("contextId")
        .or_else(|| result.get("context_id"))
        .and_then(Value::as_str)
        .or(context_id);
    poll_a2a_task(
        state,
        pool,
        row,
        agent_name,
        credential,
        rpc_url,
        next_task_id,
        next_context_id,
        trace,
    )
    .await
}

/// Entry point for `inbox::approvals::deliver()` when an `a2a_continuation`
/// approval is decided. This is the platform-side half of the runtime
/// contract's `approval_terminal_result` guarantee for the A2A bridge:
/// acceptance resumes the task, rejection cancels it — either way the turn
/// converges to a terminal state instead of being left in `waiting_approval`.
pub(crate) async fn resolve_continuation(
    state: &Arc<AppState>,
    pool: &PgPool,
    item: &crate::db::managed_agents::inbox::schema::InboxItemRow,
    accepted: bool,
) -> Result<(), GatewayError> {
    let session_id = item
        .session_id
        .as_deref()
        .ok_or_else(|| GatewayError::BadRequest("a2a continuation missing session".to_owned()))?;
    let turn_id = item
        .turn_id
        .as_deref()
        .ok_or_else(|| GatewayError::BadRequest("a2a continuation missing turn".to_owned()))?;
    let args: Value = item
        .args_json
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or(Value::Null);
    let task_id = args
        .get("task_id")
        .and_then(Value::as_str)
        .ok_or_else(|| GatewayError::BadRequest("a2a continuation missing task_id".to_owned()))?
        .to_owned();
    let context_id = args
        .get("context_id")
        .and_then(Value::as_str)
        .map(str::to_owned);

    let row = sessions::repository::get(pool, session_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("session not found".to_owned()))?;
    let agent = load_agent(pool, &row).await?;
    let source = agent_source(&agent)?;
    let credential =
        crate::http::runtime_resolution::imported_agent_credential(pool, state, &agent, &row)
            .await?;
    let rpc_url = validated_endpoint(&a2a_rpc_url(source, &credential.api_base)).await?;
    let trace = active_trace_headers(pool, session_id).await?;

    if !accepted {
        // Best-effort: the platform-side terminal transition below is what
        // actually converges the session, regardless of whether the remote
        // agent acknowledges the cancel. Sent directly (not via `cancel()`,
        // which re-resolves agent/source/credential from scratch) since this
        // function already has everything in scope.
        if let Err(error) = trace
            .apply(
                state
                    .http
                    .post(&rpc_url)
                    .bearer_auth(&credential.api_key)
                    .json(&json!({
                        "jsonrpc": "2.0",
                        "id": crate::db::managed_agents::id("rpc"),
                        "method": "tasks/cancel",
                        "params": {"id": task_id}
                    })),
            )
            .send()
            .await
        {
            tracing::warn!(session_id, task_id, %error, "failed to cancel remote A2A task on continuation reject");
        }
        session_control::repository::transition(
            pool,
            turn_id,
            "rejected",
            Some(json!({"code": "approval_rejected", "message": "用户拒绝了续接该 A2A 任务"})),
        )
        .await?;
        append_event(
            state,
            pool,
            session_id,
            json!({"type": "session.status_idle", "stop_reason": {"type": "rejected"}}),
        )
        .await?;
        sessions::repository::set_status(pool, session_id, "idle").await?;
        return Ok(());
    }

    state.external_bridge_cancellations.clear(session_id);
    session_control::repository::transition(pool, turn_id, "running", None).await?;
    let reply_text = item.feedback.as_deref().unwrap_or("请继续。");
    let outcome = resume_a2a_task(
        state,
        pool,
        &row,
        &agent.name,
        &credential,
        &rpc_url,
        &task_id,
        context_id.as_deref(),
        reply_text,
        &trace,
    )
    .await;
    match outcome {
        Ok(A2aOutcome::Completed(text)) => {
            persist_message(pool, session_id, "assistant", &text, Some("stop")).await?;
            append_event(
                state,
                pool,
                session_id,
                json!({"type": "agent.message", "content": [{"type": "text", "text": text}]}),
            )
            .await?;
            append_event(
                state,
                pool,
                session_id,
                json!({"type": "session.status_idle", "stop_reason": {"type": "end_turn"}}),
            )
            .await?;
            runtime_lifecycle::mark_session_status(state, pool, session_id, "idle", None).await?;
        }
        // Paused again (multi-round clarification): another `a2a_continuation`
        // approval was already created inside `poll_a2a_task`.
        Ok(A2aOutcome::AwaitingApproval) => {}
        // The turn/session state was already finalized by whatever cancelled it.
        Ok(A2aOutcome::Cancelled) => {}
        Err(error) => {
            append_event(
                state,
                pool,
                session_id,
                json!({"type": "session.error", "error": {"message": error.to_string()}}),
            )
            .await?;
            runtime_lifecycle::mark_session_error(state, pool, session_id, error.to_string())
                .await?;
        }
    }
    Ok(())
}

async fn invoke_dify(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    prompt: &str,
    trace: &TraceHeaders,
) -> Result<String, GatewayError> {
    let mode = source
        .pointer("/raw/mode")
        .and_then(Value::as_str)
        .unwrap_or("chat");
    if mode.contains("workflow") {
        return Err(GatewayError::InvalidConfig(
            "Dify 工作流需要先配置并验证输入映射，不能作为普通聊天自动执行。".to_owned(),
        ));
    }
    let response = trace
        .apply(
            state
                .http
                .post(format!(
                    "{}/chat-messages",
                    credential.api_base.trim_end_matches('/')
                ))
                .bearer_auth(&credential.api_key)
                .json(&json!({
                    "inputs": {},
                    "query": prompt,
                    "response_mode": "blocking",
                    "conversation_id": row.provider_session_id,
                    "user": row.owner_id.as_deref().unwrap_or("lap-user")
                })),
        )
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    let payload = ensure_success(response).await?;
    let conversation_id = payload.get("conversation_id").and_then(Value::as_str);
    let task_id = payload.get("task_id").and_then(Value::as_str);
    session_control::repository::bind_active_invocation(
        pool,
        &row.id,
        conversation_id,
        None,
        task_id,
        None,
    )
    .await?;
    if let Some(conversation_id) = conversation_id {
        sessions::repository::set_provider_run(
            pool,
            &row.id,
            task_id.unwrap_or(conversation_id),
            "running",
        )
        .await?;
    }
    payload
        .get("answer")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            GatewayError::SandboxError("Dify response did not contain answer".to_owned())
        })
}

async fn invoke_openapi(
    state: &AppState,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    prompt: &str,
    trace: &TraceHeaders,
) -> Result<String, GatewayError> {
    let mapping = source.pointer("/raw/x-lap-runtime").ok_or_else(|| {
        GatewayError::InvalidConfig(
            "OpenAPI 来源必须提供经过确认的 x-lap-runtime 映射后才能执行。".to_owned(),
        )
    })?;
    let path = mapping
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| GatewayError::InvalidConfig("x-lap-runtime 缺少 path。".to_owned()))?;
    let input_field = mapping
        .get("input_field")
        .and_then(Value::as_str)
        .unwrap_or("input");
    let output_field = mapping
        .get("output_field")
        .and_then(Value::as_str)
        .unwrap_or("output");
    if !path.starts_with('/') || path.starts_with("//") {
        return Err(GatewayError::InvalidConfig(
            "x-lap-runtime path 必须是站内绝对路径。".to_owned(),
        ));
    }
    let base = validated_endpoint(openapi_runtime_base(source, &credential.api_base)).await?;
    let response = trace
        .apply(
            state
                .http
                .post(format!("{}{}", base.trim_end_matches('/'), path))
                .bearer_auth(&credential.api_key)
                .json(&json!({input_field: prompt})),
        )
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    let payload = ensure_success(response).await?;
    payload
        .get(output_field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            GatewayError::SandboxError(format!(
                "OpenAPI response did not contain mapped field {output_field}"
            ))
        })
}

/// Synchronous LangGraph run: POST {base}/runs/wait with the confirmed
/// assistant id and an operator-mapped input, returning the graph's final
/// state. The prompt is wrapped under the mapped input field, and the answer
/// is read from the mapped output pointer — the same operator-confirms-the-
/// mapping contract as the OpenAPI bridge, because a LangGraph graph's I/O
/// schema is graph-specific. `runs/wait` blocks until the run terminates, so
/// this always resolves to a single completed/failed turn.
async fn invoke_langgraph(
    state: &AppState,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    prompt: &str,
    trace: &TraceHeaders,
) -> Result<String, GatewayError> {
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
    let input_field = mapping
        .get("input_field")
        .and_then(Value::as_str)
        .unwrap_or("input");
    let output_path = mapping
        .get("output_path")
        .and_then(Value::as_str)
        .unwrap_or("/output");
    let base = validated_endpoint(langgraph_runtime_base(source, &credential.api_base)).await?;
    let response = trace
        .apply(
            state
                .http
                .post(format!("{}/runs/wait", base.trim_end_matches('/')))
                .bearer_auth(&credential.api_key)
                .header("x-api-key", &credential.api_key)
                .json(&json!({
                    "assistant_id": assistant_id,
                    "input": { input_field: prompt }
                })),
        )
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    let payload = ensure_success(response).await?;
    payload
        .pointer(output_path)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            GatewayError::SandboxError(format!(
                "LangGraph response did not contain mapped field {output_path}"
            ))
        })
}

#[derive(Default)]
struct TraceHeaders {
    traceparent: Option<String>,
    tracestate: Option<String>,
}

impl TraceHeaders {
    fn from_metadata(metadata: &Value) -> Self {
        let Some((traceparent, tracestate)) =
            crate::managed_agents::adapters::telemetry::trace_headers(metadata)
        else {
            return Self::default();
        };
        Self {
            traceparent: Some(traceparent),
            tracestate,
        }
    }

    fn apply(&self, mut request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(traceparent) = self.traceparent.as_deref() {
            request = request.header("traceparent", traceparent);
        }
        if let Some(tracestate) = self.tracestate.as_deref() {
            request = request.header("tracestate", tracestate);
        }
        request
    }
}

async fn active_trace_headers(
    pool: &PgPool,
    session_id: &str,
) -> Result<TraceHeaders, GatewayError> {
    Ok(session_control::repository::active_turn(pool, session_id)
        .await?
        .and_then(|snapshot| snapshot.invocations.into_iter().next())
        .map(|invocation| TraceHeaders::from_metadata(&invocation.metadata))
        .unwrap_or_default())
}

fn a2a_rpc_url(source: &Value, fallback: &str) -> String {
    source
        .pointer("/raw/url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

fn openapi_runtime_base<'a>(source: &'a Value, fallback: &'a str) -> &'a str {
    source
        .pointer("/raw/x-lap-runtime/base_url")
        .or_else(|| source.pointer("/raw/servers/0/url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
}

fn langgraph_runtime_base<'a>(source: &'a Value, fallback: &'a str) -> &'a str {
    source
        .pointer("/raw/x-lap-runtime/base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
}

async fn validated_endpoint(endpoint: &str) -> Result<String, GatewayError> {
    crate::http::managed_agents::source_management::validate_connector_endpoint(endpoint).await
}

async fn load_agent(
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
) -> Result<registry::schema::ManagedAgentRow, GatewayError> {
    let agent_id = row.agent_id.as_deref().ok_or_else(|| {
        GatewayError::InvalidConfig("external bridge session requires an agent".to_owned())
    })?;
    registry::repository::get(pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::UnknownAgent(agent_id.to_owned()))
}

fn agent_source(agent: &registry::schema::ManagedAgentRow) -> Result<&Value, GatewayError> {
    agent
        .config
        .get("source")
        .ok_or_else(|| GatewayError::InvalidConfig("external agent source is missing".to_owned()))
}

async fn append_event(
    state: &AppState,
    pool: &PgPool,
    session_id: &str,
    event: Value,
) -> Result<(), GatewayError> {
    runtime_events::repository::append(pool, session_id, event.clone()).await?;
    state.local_session_events.publish(session_id, event);
    Ok(())
}

async fn ensure_success(response: reqwest::Response) -> Result<Value, GatewayError> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    if !status.is_success() {
        return Err(GatewayError::SandboxError(format!(
            "external agent returned HTTP {}",
            status.as_u16()
        )));
    }
    serde_json::from_str(&body)
        .map_err(|error| GatewayError::SandboxError(format!("invalid external JSON: {error}")))
}

fn extract_text(value: &Value) -> Option<String> {
    if let Some(text) = value.get("text").and_then(Value::as_str) {
        return Some(text.to_owned());
    }
    if let Some(parts) = value.get("parts").and_then(Value::as_array) {
        let text = parts
            .iter()
            .filter_map(extract_text)
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            return Some(text);
        }
    }
    for field in ["message", "status", "artifact", "artifacts", "history"] {
        if let Some(child) = value.get(field) {
            if let Some(text) = if let Some(items) = child.as_array() {
                let text = items
                    .iter()
                    .filter_map(extract_text)
                    .collect::<Vec<_>>()
                    .join("\n");
                (!text.is_empty()).then_some(text)
            } else {
                extract_text(child)
            } {
                return Some(text);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{extract_text, TraceHeaders};

    #[test]
    fn extracts_text_from_a2a_task_artifact() {
        let payload = json!({
            "id": "task-1",
            "artifacts": [{"parts": [{"kind": "text", "text": "assessment complete"}]}]
        });
        assert_eq!(
            extract_text(&payload).as_deref(),
            Some("assessment complete")
        );
    }

    #[test]
    fn applies_persisted_w3c_headers_to_external_requests() {
        let trace = TraceHeaders::from_metadata(&json!({
            "telemetry": {
                "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                "tracestate": "vendor=value"
            }
        }));
        let request = trace
            .apply(reqwest::Client::new().post("https://agent.example/invoke"))
            .build()
            .unwrap();
        assert_eq!(
            request.headers()["traceparent"],
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        );
        assert_eq!(request.headers()["tracestate"], "vendor=value");
    }
}
