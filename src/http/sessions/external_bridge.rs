use serde_json::{json, Value};
use sqlx::PgPool;
use std::{future::Future, pin::Pin, sync::Arc};

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
const CREWAI_SPEC: &str = "crewai_crew";
const ACP_SPEC: &str = "acp_legacy";

type BridgeFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, GatewayError>> + Send + 'a>>;

#[derive(Clone, Copy)]
struct ExternalInvocation<'a> {
    state: &'a AppState,
    pool: &'a PgPool,
    row: &'a sessions::schema::SessionRow,
    source: &'a Value,
    credential: &'a crate::http::agent_runtimes::RuntimeCredential,
    input: &'a Value,
    prompt: &'a str,
    agent_name: &'a str,
    trace: &'a TraceHeaders,
}

#[derive(Clone, Copy)]
struct ExternalCancellation<'a> {
    state: &'a AppState,
    row: &'a sessions::schema::SessionRow,
    source: &'a Value,
    credential: &'a crate::http::agent_runtimes::RuntimeCredential,
    binding: &'a session_control::schema::SessionInvocationRow,
    trace: &'a TraceHeaders,
}

trait ExternalRuntimeAdapter: Send + Sync {
    fn spec(&self) -> &'static str;

    fn invoke<'a>(&'a self, context: ExternalInvocation<'a>) -> BridgeFuture<'a, Option<Value>>;

    fn cancel<'a>(&'a self, _context: ExternalCancellation<'a>) -> BridgeFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn abort<'a>(&'a self, context: ExternalCancellation<'a>) -> BridgeFuture<'a, ()> {
        self.cancel(context)
    }
}

struct A2aRuntimeAdapter;
struct DifyRuntimeAdapter;
struct OpenApiRuntimeAdapter;
struct LangGraphRuntimeAdapter;
struct CrewAiRuntimeAdapter;
struct AcpRuntimeAdapter;

static A2A_ADAPTER: A2aRuntimeAdapter = A2aRuntimeAdapter;
static DIFY_ADAPTER: DifyRuntimeAdapter = DifyRuntimeAdapter;
static OPENAPI_ADAPTER: OpenApiRuntimeAdapter = OpenApiRuntimeAdapter;
static LANGGRAPH_ADAPTER: LangGraphRuntimeAdapter = LangGraphRuntimeAdapter;
static CREWAI_ADAPTER: CrewAiRuntimeAdapter = CrewAiRuntimeAdapter;
static ACP_ADAPTER: AcpRuntimeAdapter = AcpRuntimeAdapter;

fn adapter_for(spec: &str) -> Option<&'static dyn ExternalRuntimeAdapter> {
    let adapters: [&dyn ExternalRuntimeAdapter; 6] = [
        &A2A_ADAPTER,
        &DIFY_ADAPTER,
        &OPENAPI_ADAPTER,
        &LANGGRAPH_ADAPTER,
        &CREWAI_ADAPTER,
        &ACP_ADAPTER,
    ];
    adapters.into_iter().find(|adapter| adapter.spec() == spec)
}

pub(crate) fn supports(runtime: &str) -> bool {
    adapter_for(runtime).is_some()
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
    let run_input = session_control::repository::active_turn(pool, &row.id)
        .await?
        .map(|snapshot| snapshot.turn.input_json)
        .unwrap_or_else(|| json!({"message": prompt}));
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

    let adapter = adapter_for(spec).ok_or_else(|| {
        GatewayError::InvalidConfig(format!("unsupported external bridge: {spec}"))
    })?;
    let response = adapter
        .invoke(ExternalInvocation {
            state: &state,
            pool,
            row,
            source,
            credential: &credential,
            input: &run_input,
            prompt,
            agent_name: &agent.name,
            trace: &trace,
        })
        .await;

    match response {
        // A2A task paused on `input-required`/`auth-required`: an
        // `a2a_continuation` inbox item was already created and the turn
        // already moved to `waiting_approval` inside `poll_a2a_task`. Nothing
        // left to do here — `resolve_continuation` picks it back up once the
        // approval is decided.
        Ok(None) => Ok(()),
        Ok(Some(result)) => finish_external_result(&state, pool, row, result).await,
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

async fn finish_external_result(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    result: Value,
) -> Result<(), GatewayError> {
    let reply = result_display_text(&result);
    persist_message(pool, &row.id, "assistant", &reply, Some("stop")).await?;
    runtime_lifecycle::persist_text_message(pool, &row.id, &reply).await?;
    runtime_lifecycle::persist_turn_result(pool, &row.id, result).await?;
    append_event(
        state,
        pool,
        &row.id,
        json!({
            "type": "agent.message",
            "content": [{"type": "text", "text": reply}]
        }),
    )
    .await?;
    append_event(
        state,
        pool,
        &row.id,
        json!({"type": "session.status_idle", "stop_reason": {"type": "end_turn"}}),
    )
    .await?;
    runtime_lifecycle::mark_session_status(state, pool, &row.id, "idle", None).await
}

pub(super) async fn cancel(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
) -> Result<(), GatewayError> {
    control(state, pool, row, false).await
}

pub(super) async fn abort(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
) -> Result<(), GatewayError> {
    control(state, pool, row, true).await
}

pub(super) fn is_dify_continuation(
    row: &sessions::schema::SessionRow,
    snapshot: &session_control::schema::TurnSnapshot,
) -> bool {
    row.runtime.as_deref() == Some(DIFY_SPEC)
        && snapshot
            .invocations
            .first()
            .is_some_and(|invocation| invocation.resume_cursor.is_some())
}

pub(super) async fn resume_dify_turn(
    state: Arc<AppState>,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    input: &Value,
) -> Result<(), GatewayError> {
    let agent = load_agent(pool, row).await?;
    let credential =
        crate::http::runtime_resolution::imported_agent_credential(pool, &state, &agent, row)
            .await?;
    let snapshot = session_control::repository::active_turn(pool, &row.id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("active Dify turn not found".to_owned()))?;
    let binding = snapshot
        .invocations
        .first()
        .ok_or_else(|| GatewayError::NotFound("active Dify invocation not found".to_owned()))?;
    let trace = TraceHeaders::from_metadata(&binding.metadata);
    match super::dify_bridge::resume(&state, pool, row, &credential, binding, input, &trace).await {
        Ok(Some(result)) => finish_external_result(&state, pool, row, result).await,
        Ok(None) => Ok(()),
        Err(error) => {
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

pub(super) fn is_langgraph_continuation(
    row: &sessions::schema::SessionRow,
    snapshot: &session_control::schema::TurnSnapshot,
) -> bool {
    row.runtime.as_deref() == Some(LANGGRAPH_SPEC)
        && snapshot.turn.status == "waiting_input"
        && snapshot.invocations.first().is_some_and(|invocation| {
            invocation.remote_session_id.is_some() && invocation.remote_task_id.is_some()
        })
}

pub(super) async fn resume_langgraph_turn(
    state: Arc<AppState>,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    input: &Value,
) -> Result<(), GatewayError> {
    let agent = load_agent(pool, row).await?;
    let source = agent_source(&agent)?;
    let credential =
        crate::http::runtime_resolution::imported_agent_credential(pool, &state, &agent, row)
            .await?;
    let snapshot = session_control::repository::active_turn(pool, &row.id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("active LangGraph turn not found".to_owned()))?;
    let binding = snapshot.invocations.first().ok_or_else(|| {
        GatewayError::NotFound("active LangGraph invocation not found".to_owned())
    })?;
    let trace = TraceHeaders::from_metadata(&binding.metadata);
    match super::langgraph_bridge::resume(
        &state,
        pool,
        row,
        source,
        &credential,
        binding,
        input,
        &trace,
    )
    .await
    {
        Ok(Some(result)) => finish_external_result(&state, pool, row, result).await,
        Ok(None) => Ok(()),
        Err(error) => {
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

async fn control(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    abort: bool,
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
    let adapter = adapter_for(spec).ok_or_else(|| {
        GatewayError::InvalidConfig(format!("unsupported external bridge: {spec}"))
    })?;
    let context = ExternalCancellation {
        state,
        row,
        source,
        credential: &credential,
        binding: &binding,
        trace: &trace,
    };
    if abort {
        adapter.abort(context).await
    } else {
        adapter.cancel(context).await
    }
}

impl ExternalRuntimeAdapter for A2aRuntimeAdapter {
    fn spec(&self) -> &'static str {
        A2A_SPEC
    }

    fn invoke<'a>(&'a self, context: ExternalInvocation<'a>) -> BridgeFuture<'a, Option<Value>> {
        Box::pin(async move {
            invoke_a2a(
                context.state,
                context.pool,
                context.row,
                context.source,
                context.credential,
                context.prompt,
                context.agent_name,
                context.trace,
            )
            .await
            .map(|result| result.map(Value::String))
        })
    }

    fn cancel<'a>(&'a self, context: ExternalCancellation<'a>) -> BridgeFuture<'a, ()> {
        Box::pin(cancel_a2a(context))
    }
}

impl ExternalRuntimeAdapter for DifyRuntimeAdapter {
    fn spec(&self) -> &'static str {
        DIFY_SPEC
    }

    fn invoke<'a>(&'a self, context: ExternalInvocation<'a>) -> BridgeFuture<'a, Option<Value>> {
        Box::pin(super::dify_bridge::invoke(
            context.state,
            context.pool,
            context.row,
            context.source,
            context.credential,
            context.input,
            context.prompt,
            context.trace,
        ))
    }

    fn cancel<'a>(&'a self, context: ExternalCancellation<'a>) -> BridgeFuture<'a, ()> {
        Box::pin(super::dify_bridge::cancel(
            context.state,
            context.row,
            context.source,
            context.credential,
            context.binding,
            context.trace,
        ))
    }
}

impl ExternalRuntimeAdapter for OpenApiRuntimeAdapter {
    fn spec(&self) -> &'static str {
        OPENAPI_SPEC
    }

    fn invoke<'a>(&'a self, context: ExternalInvocation<'a>) -> BridgeFuture<'a, Option<Value>> {
        Box::pin(async move {
            invoke_openapi(
                context.state,
                context.source,
                context.credential,
                context.input,
                context.prompt,
                context.trace,
            )
            .await
            .map(Some)
        })
    }
}

impl ExternalRuntimeAdapter for LangGraphRuntimeAdapter {
    fn spec(&self) -> &'static str {
        LANGGRAPH_SPEC
    }

    fn invoke<'a>(&'a self, context: ExternalInvocation<'a>) -> BridgeFuture<'a, Option<Value>> {
        Box::pin(super::langgraph_bridge::invoke(
            context.state,
            context.pool,
            context.row,
            context.source,
            context.credential,
            context.input,
            context.prompt,
            context.trace,
        ))
    }

    fn cancel<'a>(&'a self, context: ExternalCancellation<'a>) -> BridgeFuture<'a, ()> {
        Box::pin(super::langgraph_bridge::cancel(
            context.state,
            context.row,
            context.source,
            context.credential,
            context.binding,
            context.trace,
        ))
    }
}

impl ExternalRuntimeAdapter for CrewAiRuntimeAdapter {
    fn spec(&self) -> &'static str {
        CREWAI_SPEC
    }

    fn invoke<'a>(&'a self, context: ExternalInvocation<'a>) -> BridgeFuture<'a, Option<Value>> {
        Box::pin(async move {
            invoke_crewai(
                context.state,
                context.pool,
                context.row,
                context.source,
                context.credential,
                context.input,
                context.prompt,
                context.trace,
            )
            .await
            .map(Some)
        })
    }
}

impl ExternalRuntimeAdapter for AcpRuntimeAdapter {
    fn spec(&self) -> &'static str {
        ACP_SPEC
    }

    fn invoke<'a>(&'a self, _context: ExternalInvocation<'a>) -> BridgeFuture<'a, Option<Value>> {
        Box::pin(async {
            Err(GatewayError::InvalidConfig(
                "ACP 接入必须选择并验证具体兼容配置后才能执行。".to_owned(),
            ))
        })
    }
}

async fn cancel_a2a(context: ExternalCancellation<'_>) -> Result<(), GatewayError> {
    let Some(task_id) = context.binding.remote_task_id.as_deref() else {
        return Ok(());
    };
    let rpc_url =
        validated_endpoint(&a2a_rpc_url(context.source, &context.credential.api_base)).await?;
    let response = context
        .trace
        .apply(
            context
                .state
                .http
                .post(rpc_url)
                .bearer_auth(&context.credential.api_key)
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
            runtime_lifecycle::persist_text_result(pool, session_id, &text).await?;
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

async fn invoke_openapi(
    state: &AppState,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    input: &Value,
    prompt: &str,
    trace: &TraceHeaders,
) -> Result<Value, GatewayError> {
    let mapping = source.pointer("/raw/x-lap-runtime").ok_or_else(|| {
        GatewayError::InvalidConfig(
            "OpenAPI 来源必须提供经过确认的 x-lap-runtime 映射后才能执行。".to_owned(),
        )
    })?;
    let output_field = mapping
        .get("output_field")
        .and_then(Value::as_str)
        .unwrap_or("output");
    let payload = openapi_request(state, source, credential, input, prompt, trace, mapping).await?;
    payload.get(output_field).cloned().ok_or_else(|| {
        GatewayError::SandboxError(format!(
            "OpenAPI response did not contain mapped field {output_field}"
        ))
    })
}

/// Issues the mapped OpenAPI call and returns the whole response body, leaving
/// field extraction to the caller. Split out so `probe_openapi` observes the
/// exact request execution sends rather than a parallel implementation of it.
///
/// `base_url` still comes from `source` (not `mapping`), so a probe driven by a
/// candidate mapping keeps hitting whatever host the confirmed mapping pinned.
async fn openapi_request(
    state: &AppState,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    input: &Value,
    prompt: &str,
    trace: &TraceHeaders,
    mapping: &Value,
) -> Result<Value, GatewayError> {
    let path = mapping
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| GatewayError::InvalidConfig("x-lap-runtime 缺少 path。".to_owned()))?;
    let input_field = mapping
        .get("input_field")
        .and_then(Value::as_str)
        .unwrap_or("input");
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
                .json(&json!({input_field: mapped_input(input, input_field, prompt)})),
        )
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    ensure_success(response).await
}

/// OpenAPI counterpart to `probe_langgraph`: posts a sentinel to a candidate
/// `path`/`input_field` and returns the whole response body.
///
/// Note the mapping languages differ — `invoke_openapi` reads its answer with
/// `payload.get(output_field)`, a *top-level field name*, not the JSON Pointer
/// LangGraph's `output_path` uses. Only depth-1 keys of this response are
/// therefore addressable for OpenAPI sources.
///
/// Executes the remote service for real; operator-triggered only.
pub(crate) async fn probe_openapi(
    state: &AppState,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    path: &str,
    input_field: &str,
    sentinel: &str,
) -> Result<Value, GatewayError> {
    openapi_request(
        state,
        source,
        credential,
        &json!({}),
        sentinel,
        &TraceHeaders::default(),
        &json!({ "path": path, "input_field": input_field }),
    )
    .await
}

/// The explicit mapping probe intentionally uses the blocking endpoint so it
/// can return the complete native state to the operator for output selection.
async fn invoke_langgraph_wait(
    state: &AppState,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    input: &Value,
    prompt: &str,
    trace: &TraceHeaders,
) -> Result<Value, GatewayError> {
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
                    "input": { input_field: mapped_input(input, input_field, prompt) }
                })),
        )
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    let payload = ensure_success(response).await?;
    payload.pointer(output_path).cloned().ok_or_else(|| {
        GatewayError::SandboxError(format!(
            "LangGraph response did not contain mapped field {output_path}"
        ))
    })
}

/// Runs one real LangGraph request with a candidate `input_field` and returns
/// the *whole* response, so an operator confirms `output_path` against an
/// observed payload instead of guessing a JSON Pointer and discovering it was
/// wrong only when a session fails.
///
/// The synthetic mapping sets `output_path` to the empty RFC 6901 pointer so
/// the complete state is observable without loosening the mapping gate.
///
/// This *executes the remote agent for real* (side effects, model spend), so
/// it is only ever reachable from an explicit operator action, never from
/// import or the background sync scheduler.
pub(crate) async fn probe_langgraph(
    state: &AppState,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    input_field: &str,
    sentinel: &str,
) -> Result<Value, GatewayError> {
    let mut probe_source = source.clone();
    let mut mapping = source
        .pointer("/raw/x-lap-runtime")
        .filter(|mapping| mapping.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    // Keep any confirmed `base_url` from the stored mapping: the probe must hit
    // the same host execution would.
    mapping["input_field"] = json!(input_field);
    mapping["output_path"] = json!("");
    // Built through the map API rather than `Value` indexing, which panics on a
    // non-object it cannot auto-vivify.
    let root = probe_source
        .as_object_mut()
        .ok_or_else(|| GatewayError::InvalidConfig("外部智能体来源必须是对象。".to_owned()))?;
    let raw = root.entry("raw").or_insert_with(|| json!({}));
    if !raw.is_object() {
        *raw = json!({});
    }
    if let Some(raw) = raw.as_object_mut() {
        raw.insert("x-lap-runtime".to_owned(), mapping);
    }
    invoke_langgraph_wait(
        state,
        &probe_source,
        credential,
        // An empty input makes `mapped_input` fall back to the sentinel, which
        // is exactly what a chat-triggered turn sends.
        &json!({}),
        sentinel,
        &TraceHeaders::default(),
    )
    .await
}

/// CrewAI is asynchronous: POST {base}/kickoff starts the crew and returns a
/// kickoff id, then GET {base}/status/{id} is polled until the run reaches a
/// terminal state. The prompt is wrapped under the mapped kickoff input field
/// and the answer read from the mapped output pointer (operator-confirmed
/// mapping, like the OpenAPI/LangGraph bridges). Polling is bounded and honours
/// session cancellation, so the turn always resolves to a completed/failed
/// state within the deadline.
async fn invoke_crewai(
    state: &AppState,
    pool: &PgPool,
    row: &sessions::schema::SessionRow,
    source: &Value,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    input: &Value,
    prompt: &str,
    trace: &TraceHeaders,
) -> Result<Value, GatewayError> {
    let mapping = source.pointer("/raw/x-lap-runtime").ok_or_else(|| {
        GatewayError::InvalidConfig(
            "CrewAI 来源必须提供经过确认的 kickoff 输入映射后才能执行。".to_owned(),
        )
    })?;
    let input_field = mapping
        .get("input_field")
        .and_then(Value::as_str)
        .unwrap_or("topic");
    let output_path = mapping
        .get("output_path")
        .and_then(Value::as_str)
        .unwrap_or("/result");
    let base = validated_endpoint(crewai_runtime_base(source, &credential.api_base)).await?;
    let base = base.trim_end_matches('/');
    let kickoff = trace
        .apply(
            state
                .http
                .post(format!("{base}/kickoff"))
                .bearer_auth(&credential.api_key)
                .json(&json!({
                    "inputs": { input_field: mapped_input(input, input_field, prompt) }
                })),
        )
        .send()
        .await
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    let kicked = ensure_success(kickoff).await?;
    let kickoff_id = kicked
        .get("kickoff_id")
        .or_else(|| kicked.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            GatewayError::SandboxError("CrewAI kickoff did not return a kickoff id".to_owned())
        })?;
    session_control::repository::bind_active_invocation(
        pool,
        &row.id,
        None,
        None,
        Some(kickoff_id),
        None,
    )
    .await?;
    let status_url = format!("{base}/status/{kickoff_id}");
    poll_crewai_status(state, row, credential, &status_url, output_path, trace).await
}

/// Polls a CrewAI kickoff to a terminal state, honouring session cancellation
/// and a bounded deadline (120 × 500ms = 60s). Returns the mapped output field
/// on success.
async fn poll_crewai_status(
    state: &AppState,
    row: &sessions::schema::SessionRow,
    credential: &crate::http::agent_runtimes::RuntimeCredential,
    status_url: &str,
    output_path: &str,
    trace: &TraceHeaders,
) -> Result<Value, GatewayError> {
    for _ in 0..120 {
        if state.external_bridge_cancellations.is_cancelled(&row.id) {
            return Err(GatewayError::SandboxError(
                "CrewAI run cancelled".to_owned(),
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let response = trace
            .apply(state.http.get(status_url).bearer_auth(&credential.api_key))
            .send()
            .await
            .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
        let payload = ensure_success(response).await?;
        match payload
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("RUNNING")
            .to_ascii_uppercase()
            .as_str()
        {
            "SUCCESS" | "COMPLETED" => {
                return payload.pointer(output_path).cloned().ok_or_else(|| {
                    GatewayError::SandboxError(format!(
                        "CrewAI response did not contain mapped field {output_path}"
                    ))
                });
            }
            "FAILED" | "ERROR" => {
                return Err(GatewayError::SandboxError(
                    "CrewAI run ended in a failed state".to_owned(),
                ));
            }
            _ => {}
        }
    }
    Err(GatewayError::SandboxError(
        "CrewAI run did not finish before the bridge deadline".to_owned(),
    ))
}

#[derive(Default)]
pub(super) struct TraceHeaders {
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

    pub(super) fn apply(&self, mut request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
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

fn crewai_runtime_base<'a>(source: &'a Value, fallback: &'a str) -> &'a str {
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

    use super::{adapter_for, extract_text, supports, TraceHeaders};

    #[test]
    fn resolves_supported_sources_through_runtime_adapters() {
        for spec in [
            "a2a_v1",
            "dify_app",
            "openapi_rest",
            "langgraph_assistant",
            "crewai_crew",
            "acp_legacy",
        ] {
            assert!(supports(spec));
            assert_eq!(adapter_for(spec).map(|adapter| adapter.spec()), Some(spec));
        }
        assert!(!supports("unknown"));
    }

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

fn result_display_text(result: &Value) -> String {
    match result {
        Value::String(s) => s.clone(),
        Value::Object(map) => {
            if let Some(Value::String(text)) = map
                .get("text")
                .or_else(|| map.get("answer"))
                .or_else(|| map.get("output"))
            {
                text.clone()
            } else {
                serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string())
            }
        }
        _ => result.to_string(),
    }
}

fn mapped_input(input: &Value, input_field: &str, fallback_prompt: &str) -> Value {
    if let Value::Object(map) = input {
        if let Some(val) = map.get(input_field) {
            return val.clone();
        }
    }
    Value::String(fallback_prompt.to_owned())
}
