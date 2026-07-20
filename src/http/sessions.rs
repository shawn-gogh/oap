use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    agents::events,
    db::managed_agents::{messages, session_control, sessions},
    errors::GatewayError,
    proxy::state::AppState,
};

mod artifacts_api;
mod cloudevents_api;
mod control_events_api;
mod execution;
pub(crate) mod external_bridge;
mod generic_chat;
mod quotas;
pub mod recovery;
mod run_projection;
mod run_types;
mod runtime;
mod runtime_events_api;
mod runtime_events_reconcile;
mod runtime_inputs;
mod runtime_lifecycle;
mod runtime_mcp_validation;
mod runtime_provision;
mod runtime_sdk;
mod runtime_vault;
mod storage;
mod types;
mod workspace_api;

pub use artifacts_api::{create_artifact, get_artifact, list_artifacts};
pub use cloudevents_api::{egress as cloud_events, ingress as ingest_cloud_event};
pub use control_events_api::control_event_stream;
use execution::execute_prompt;
use runtime::{create_runtime_session, execute_runtime_prompt};
pub(crate) use runtime::{
    create_runtime_session_for_agent, create_runtime_session_for_agent_task,
    create_runtime_session_for_agent_task_with_prompt,
    create_runtime_session_for_agent_without_prompt,
};
pub(crate) use runtime_events_api::runtime_event_stream_for_session;
pub use runtime_events_api::{runtime_event_list, runtime_events};
pub(crate) use runtime_sdk::lap_from_credential;
use runtime_sdk::{register_runtime_session, runtime_sdk_client};
use storage::{auth_db, db, owned_session, persist_message, session};
pub use types::{CreateSessionRequest, MessageResponse, PromptRequest, SessionResponse};
pub use workspace_api::{
    batch_delete_files, batch_transfer_files, browse_files, copy_files, create_folder,
    create_upload_url, delete_file, delete_workspace_trash, download_url, empty_workspace_trash,
    list_files, list_folders, list_workspace_trash, move_files, restore_workspace_trash,
    trash_workspace_paths,
};

#[derive(Debug, Default, Deserialize)]
pub struct ListSessionsQuery {
    agent_id: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<Vec<SessionResponse>>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    let owner_filter = (!auth.is_admin).then_some(auth.user_id.as_str());
    let agent_filter = query
        .agent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let rows = sessions::repository::list_filtered(pool, owner_filter, agent_filter).await?;
    Ok(Json(rows.into_iter().map(SessionResponse::from).collect()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(mut input): Json<CreateSessionRequest>,
) -> Result<Json<SessionResponse>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    let pool = pool.clone();
    // Using an agent requires use-level access (owner, admin, or a grant).
    let requested_agent = input
        .agent_id
        .as_deref()
        .or(input.agent.as_deref())
        .filter(|id| id.starts_with("agent_"));
    let mut task_id = None;
    if let Some(agent_id) = requested_agent {
        let agent = crate::db::managed_agents::registry::repository::get(&pool, agent_id)
            .await?
            .ok_or_else(|| GatewayError::UnknownAgent(agent_id.to_owned()))?;
        crate::http::managed_agents::assert_agent_use(&auth, &agent, &pool).await?;
        task_id = Some(resolve_session_task(&pool, &agent, &input, &auth.user_id).await?);
        input.task_id.clone_from(&task_id);
    }
    if input.has_runtime() {
        let (traceparent, tracestate) = trace_headers(&headers);
        return match create_runtime_session(
            state,
            &pool,
            input,
            Some(&auth.user_id),
            traceparent,
            tracestate,
        )
        .await
        {
            Ok(response) => Ok(Json(response)),
            Err(error) => {
                if let Some(task_id) = task_id {
                    let _ = crate::db::managed_agents::tasks::repository::fail(
                        &pool,
                        &task_id,
                        &error.to_string(),
                    )
                    .await;
                }
                Err(error)
            }
        };
    }
    let session_task_id = input.task_id.clone();
    let (resolved, quota) = quotas::resolve_non_runtime_session(&state, &pool, input).await?;
    let row = {
        let _quota = quota;
        sessions::repository::create(
            &pool,
            &resolved.harness,
            resolved.agent_id.as_deref(),
            &resolved.title,
            resolved.timezone.as_deref(),
            Some(&auth.user_id),
            session_task_id.as_deref(),
        )
        .await?
    };
    quotas::finish_non_runtime(&state, &pool, &resolved, &row, session_task_id.as_deref()).await?;
    Ok(Json(SessionResponse::from(row)))
}

async fn resolve_session_task(
    pool: &sqlx::PgPool,
    agent: &crate::db::managed_agents::registry::schema::ManagedAgentRow,
    input: &CreateSessionRequest,
    created_by: &str,
) -> Result<String, GatewayError> {
    use crate::db::managed_agents::tasks::{repository, schema::NewTask};

    if let Some(task_id) = input.task_id.as_deref() {
        let task = repository::get(pool, &agent.id, task_id)
            .await?
            .ok_or_else(|| GatewayError::NotFound("task not found".to_owned()))?;
        if !matches!(task.status.as_str(), "queued" | "waiting_input") {
            return Err(GatewayError::BadRequest(format!(
                "task {} cannot start from status {}",
                task.id, task.status
            )));
        }
        return Ok(task.id);
    }

    let title = input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{} task", agent.name));
    let mut task_input = serde_json::Map::new();
    if let Some(prompt) = input
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
    {
        task_input.insert("prompt".to_owned(), Value::String(prompt.to_owned()));
    }
    let task = repository::create(
        pool,
        NewTask {
            agent_id: &agent.id,
            application_version: crate::http::managed_agents::tasks::application_version(
                &agent.config,
            ),
            source: if agent.status == "draft" {
                "test"
            } else {
                "manual"
            },
            source_id: None,
            title: &title,
            input: Value::Object(task_input),
            created_by,
            completion_criteria: crate::http::managed_agents::tasks::completion_criteria(
                &agent.config,
            ),
        },
    )
    .await?;
    Ok(task.id)
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<SessionResponse>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    let row = owned_session(pool, &auth, &session_id).await?;
    Ok(Json(SessionResponse::from(row)))
}

/// Model the session's agent is configured for, if resolvable. Callers that
/// enqueue prompts on behalf of a session (approval resume, platform MCP
/// send) should prefer this over any hardcoded default.
pub(crate) async fn agent_model_for_session(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> Option<String> {
    let session = sessions::repository::get(pool, session_id).await.ok()??;
    if session.agent_id.is_none() {
        return session
            .environment_json
            .get("temporary_model")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_owned);
    }
    let agent_id = session.agent_id.as_deref().or(
        // Legacy rows store the agent reference in `harness`.
        session
            .harness
            .starts_with("agent_")
            .then_some(session.harness.as_str()),
    )?;
    let agent = crate::db::managed_agents::registry::repository::get(pool, agent_id)
        .await
        .ok()??;
    (!agent.model.trim().is_empty()).then_some(agent.model)
}

#[derive(serde::Deserialize)]
pub struct RenameSessionRequest {
    pub title: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct SetApprovalModeRequest {
    pub mode: String,
}

/// PUT /session/{id}/approval-mode — per-session tool-approval policy for
/// the composer selector: "ask" | "auto" | "full".
pub async fn set_approval_mode(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<SetApprovalModeRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    let mode = input.mode.trim();
    if !matches!(mode, "ask" | "auto" | "full") {
        return Err(GatewayError::BadRequest(
            "mode must be one of: ask, auto, full".to_owned(),
        ));
    }
    let updated = sessions::repository::set_approval_mode(pool, &session_id, mode).await?;
    if !updated {
        return Err(GatewayError::NotFound("session not found".to_owned()));
    }
    Ok(Json(serde_json::json!({ "approval_mode": mode })))
}

pub async fn rename(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<RenameSessionRequest>,
) -> Result<Json<SessionResponse>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    let title = input.title.trim();
    if title.is_empty() {
        return Err(GatewayError::InvalidConfig(
            "title must not be empty".to_owned(),
        ));
    }
    sessions::repository::set_title(pool, &session_id, title).await?;
    let row = sessions::repository::get(pool, &session_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("session not found".to_owned()))?;
    Ok(Json(SessionResponse::from(row)))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<bool>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    let row = owned_session(pool, &auth, &session_id).await?;
    if let Some(storage) = &state.object_storage {
        let bucket = row.workspace_bucket.unwrap_or_else(|| {
            crate::object_storage::ObjectStorageClient::bucket_name(&session_id)
        });
        // Canonical artifacts also use the deterministic Session bucket, even
        // for lightweight Sessions that never provisioned a workspace.
        // Best-effort: a storage hiccup shouldn't block deleting the row.
        let _ = storage.delete_bucket_recursive(&bucket).await;
    }
    // Pending approvals for a deleted session can never be decided into a
    // live turn; expire them so the inbox doesn't accumulate zombies.
    let _ =
        crate::db::managed_agents::inbox::repository::expire_pending_for_session(pool, &session_id)
            .await;
    let _ =
        crate::db::managed_agents::tasks::repository::cancel_for_session(pool, &session_id).await;
    Ok(Json(sessions::repository::delete(pool, &session_id).await?))
}

pub async fn messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<MessageResponse>>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    let rows = messages::repository::list(pool, &session_id).await?;
    rows.into_iter()
        .map(MessageResponse::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

pub async fn prompt_async(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<PromptRequest>,
) -> Result<StatusCode, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    let pool = pool.clone();
    owned_session(&pool, &auth, &session_id).await?;
    let run_input = input.run_input()?;
    let prompt = input.execution_prompt()?;
    let structured = input.has_structured_input();
    let model = input
        .model_id()
        .ok_or(GatewayError::MissingModel)?
        .to_owned();
    let runtime_model = Some(model.clone());
    let request_id = request_id(&headers, input.request_id());
    let (traceparent, tracestate) = trace_headers(&headers);
    enqueue_prompt_text_with_runtime_model(
        state,
        pool,
        &session_id,
        prompt,
        run_input,
        structured,
        if structured { "manual" } else { "conversation" },
        None,
        1,
        model,
        runtime_model,
        request_id,
        traceparent,
        tracestate,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_turn(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<PromptRequest>,
) -> Result<Json<run_types::RunSnapshotV1>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    let pool = pool.clone();
    owned_session(&pool, &auth, &session_id).await?;
    let run_input = input.run_input()?;
    let prompt = input.execution_prompt()?;
    let structured = input.has_structured_input();
    let model = input
        .model_id()
        .ok_or(GatewayError::MissingModel)?
        .to_owned();
    let request_id = request_id(&headers, input.request_id());
    let (traceparent, tracestate) = trace_headers(&headers);
    enqueue_prompt_text_with_runtime_model(
        state,
        pool.clone(),
        &session_id,
        prompt,
        run_input,
        structured,
        if structured { "manual" } else { "conversation" },
        None,
        1,
        model.clone(),
        Some(model),
        request_id.clone(),
        traceparent,
        tracestate,
    )
    .await?;
    let turn_id = session_control::repository::get_by_request(&pool, &session_id, &request_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("turn was not created".to_owned()))?
        .turn
        .id;
    Ok(Json(
        run_projection::load(&pool, &session_id, &turn_id).await?,
    ))
}

pub async fn list_turns(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<session_control::schema::SessionTurnRow>>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    Ok(Json(
        session_control::repository::list_turns(pool, &session_id).await?,
    ))
}

pub async fn get_turn(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> Result<Json<run_types::RunSnapshotV1>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    Ok(Json(
        run_projection::load(pool, &session_id, &turn_id).await?,
    ))
}

pub async fn active_turn(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<Option<session_control::schema::TurnSnapshot>>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    Ok(Json(
        session_control::repository::active_turn(pool, &session_id).await?,
    ))
}

pub async fn cancel_turn(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> Result<Json<run_types::RunSnapshotV1>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    let row = owned_session(pool, &auth, &session_id).await?;
    let snapshot = session_control::repository::get_turn(pool, &session_id, &turn_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("turn {turn_id} not found")))?;
    if matches!(
        snapshot.turn.status.as_str(),
        "completed" | "failed" | "rejected" | "cancelled" | "timed_out"
    ) {
        return Ok(Json(
            run_projection::load(pool, &session_id, &turn_id).await?,
        ));
    }
    abort_session_internal(&state, pool, &row, "cancelled by user").await?;
    Ok(Json(
        run_projection::load(pool, &session_id, &turn_id).await?,
    ))
}

pub async fn resume_turn(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
    Json(input): Json<run_types::ResumeRunRequestV1>,
) -> Result<Json<run_types::RunSnapshotV1>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    let pool = pool.clone();
    let row = owned_session(&pool, &auth, &session_id).await?;
    let snapshot = session_control::repository::get_turn(&pool, &session_id, &turn_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("turn {turn_id} not found")))?;
    if snapshot.turn.status != "waiting_input" {
        return Err(GatewayError::BadRequest(format!(
            "turn {turn_id} can only resume from waiting_input, current status is {}",
            snapshot.turn.status
        )));
    }
    let request_id = input.request_id.trim();
    if request_id.is_empty() {
        return Err(GatewayError::BadRequest(
            "resume request_id is required".to_owned(),
        ));
    }
    let patch = input
        .input
        .as_object()
        .filter(|input| !input.is_empty())
        .ok_or_else(|| {
            GatewayError::BadRequest("resume input must be a non-empty object".to_owned())
        })?;
    let turn =
        session_control::repository::merge_turn_input(&pool, &session_id, &turn_id, patch).await?;
    session_control::repository::append_event(
        &pool,
        session_control::repository::NewControlEvent {
            session_id: &session_id,
            turn_id: Some(&turn_id),
            invocation_id: snapshot
                .invocations
                .first()
                .map(|invocation| invocation.id.as_str()),
            request_id: Some(request_id),
            event_key: &format!("turn:{turn_id}:input:{request_id}:resolved"),
            event_type: "input.resolved",
            event: json!({
                "schema_version": 1,
                "request_id": request_id,
                "input": Value::Object(patch.clone())
            }),
        },
    )
    .await?;
    let prompt = types::execution_prompt_for_input(&turn.input_json)?;
    persist_message(&pool, &session_id, "user", &prompt, None).await?;
    session_control::repository::transition(&pool, &turn_id, "running", None).await?;
    sessions::repository::set_status(&pool, &session_id, "running").await?;
    spawn_existing_turn_execution(state, pool.clone(), row, turn, prompt);
    Ok(Json(
        run_projection::load(&pool, &session_id, &turn_id).await?,
    ))
}

pub async fn retry_turn(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
    Json(input): Json<run_types::RetryRunRequestV1>,
) -> Result<Json<run_types::RunSnapshotV1>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    let pool = pool.clone();
    owned_session(&pool, &auth, &session_id).await?;
    let previous = session_control::repository::get_turn(&pool, &session_id, &turn_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("turn {turn_id} not found")))?;
    if !matches!(
        previous.turn.status.as_str(),
        "completed" | "failed" | "rejected" | "cancelled" | "timed_out"
    ) {
        return Err(GatewayError::BadRequest(format!(
            "turn {turn_id} must be terminal before retry"
        )));
    }
    let retry_request_id = input.request_id().map(str::to_owned);
    let run_input = input
        .input
        .unwrap_or_else(|| previous.turn.input_json.clone());
    if !run_input.is_object() {
        return Err(GatewayError::BadRequest(
            "retry input must be a JSON object".to_owned(),
        ));
    }
    let prompt = types::execution_prompt_for_input(&run_input)?;
    let model = previous
        .turn
        .model
        .clone()
        .ok_or(GatewayError::MissingModel)?;
    let request_id = request_id(&headers, retry_request_id.as_deref());
    enqueue_prompt_text_with_runtime_model(
        state,
        pool.clone(),
        &session_id,
        prompt,
        run_input,
        true,
        "retry",
        Some(turn_id),
        previous.turn.attempt_number.saturating_add(1),
        model.clone(),
        Some(model),
        request_id.clone(),
        None,
        None,
    )
    .await?;
    let next_turn_id = session_control::repository::get_by_request(&pool, &session_id, &request_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("retry turn was not created".to_owned()))?
        .turn
        .id;
    Ok(Json(
        run_projection::load(&pool, &session_id, &next_turn_id).await?,
    ))
}

fn spawn_existing_turn_execution(
    state: Arc<AppState>,
    pool: sqlx::PgPool,
    row: sessions::schema::SessionRow,
    turn: session_control::schema::SessionTurnRow,
    prompt: String,
) {
    tokio::spawn(async move {
        let result = if row.runtime.is_some() {
            execute_runtime_prompt(
                state.clone(),
                &pool,
                row.clone(),
                prompt,
                turn.model.clone(),
            )
            .await
        } else {
            let model = turn.model.clone().ok_or(GatewayError::MissingModel);
            match model {
                Ok(model) => {
                    execute_prompt(state.clone(), pool.clone(), row.clone(), prompt, model).await
                }
                Err(error) => Err(error),
            }
        };
        match result {
            Ok(()) => {
                let _ = session_control::repository::transition(&pool, &turn.id, "completed", None)
                    .await;
            }
            Err(error) => {
                let _ = session_control::repository::transition(
                    &pool,
                    &turn.id,
                    "failed",
                    Some(json!({"code": "resume_error", "message": error.to_string()})),
                )
                .await;
                let _ = runtime_lifecycle::mark_session_error(
                    &state,
                    &pool,
                    &row.id,
                    error.to_string(),
                )
                .await;
            }
        }
    });
}

#[derive(Debug, Default, Deserialize)]
pub struct ControlEventsQuery {
    after_sequence: Option<i32>,
}

pub async fn control_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<ControlEventsQuery>,
) -> Result<Json<Vec<run_types::ControlEventV1>>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    Ok(Json(
        session_control::repository::list_events(
            pool,
            &session_id,
            query.after_sequence.unwrap_or_default().max(0),
        )
        .await?
        .into_iter()
        .map(run_types::ControlEventV1::from)
        .collect(),
    ))
}

pub(crate) async fn enqueue_prompt_text(
    state: Arc<AppState>,
    pool: sqlx::PgPool,
    session_id: &str,
    prompt: String,
    model: String,
) -> Result<(), GatewayError> {
    enqueue_prompt_text_with_runtime_model(
        state,
        pool,
        session_id,
        prompt.clone(),
        json!({"message": prompt}),
        false,
        "conversation",
        None,
        1,
        model,
        None,
        crate::db::managed_agents::id("req"),
        None,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn enqueue_prompt_text_with_runtime_model(
    state: Arc<AppState>,
    pool: sqlx::PgPool,
    session_id: &str,
    prompt: String,
    input_json: Value,
    structured_input: bool,
    trigger_type: &'static str,
    retry_of_turn_id: Option<String>,
    attempt_number: i32,
    model: String,
    runtime_model: Option<String>,
    request_id: String,
    traceparent: Option<String>,
    tracestate: Option<String>,
) -> Result<(), GatewayError> {
    let session_id = session_id.to_owned();
    let row = session(&pool, &session_id).await?;
    let descriptor = crate::http::runtime_resolution::describe_session_runtime(&pool, &row).await?;
    let input_schema = json!({"type": "object"});
    let output_schema = json!({});
    let interaction_profile = serde_json::to_value(
        crate::managed_agents::adapters::types::InteractionProfileV1 {
            primary_surface: if structured_input {
                crate::managed_agents::adapters::types::PrimarySurface::Run
            } else {
                crate::managed_agents::adapters::types::PrimarySurface::Conversation
            },
            ..Default::default()
        },
    )?;
    let created_turn = {
        let _quota = quotas::prompt(&state, &pool, &row).await?;
        session_control::repository::create_or_get(
            &pool,
            session_control::repository::NewTurn {
                session_id: &row.id,
                request_id: &request_id,
                model: Some(&model),
                input: &input_json,
                input_schema: &input_schema,
                output_schema: &output_schema,
                interaction_profile: &interaction_profile,
                trigger_type,
                retry_of_turn_id: retry_of_turn_id.as_deref(),
                attempt_number,
                agent_id: row.agent_id.as_deref(),
                runtime: row.runtime.as_deref(),
                protocol: &descriptor.protocol,
                protocol_version: &descriptor.protocol_version,
                adapter_id: &descriptor.adapter_id,
                traceparent: traceparent.as_deref(),
                tracestate: tracestate.as_deref(),
            },
        )
        .await?
    };
    if !created_turn.created {
        return Ok(());
    }
    let turn_id = created_turn.snapshot.turn.id;

    if let Err(error) = persist_message(&pool, &session_id, "user", &prompt, None).await {
        let _ = session_control::repository::transition(
            &pool,
            &turn_id,
            "failed",
            Some(json!({"code": "message_persist_failed", "message": error.to_string()})),
        )
        .await;
        return Err(error);
    }
    session_control::repository::transition(&pool, &turn_id, "running", None).await?;
    if row.task_id.is_some() {
        crate::db::managed_agents::tasks::repository::mark_running_for_session(&pool, &row.id)
            .await?;
    }
    state
        .agent_runs
        .track_run(row.agent_id.as_deref().unwrap_or(&row.harness), &session_id);

    if row.runtime.is_some() {
        sessions::repository::set_status(&pool, &row.id, "running").await?;
        runtime_lifecycle::emit_runtime_stage(&state, &pool, &row.id, "accepted").await?;
        let task_pool = pool.clone();
        let task_turn_id = turn_id.clone();
        tokio::spawn(async move {
            match execute_runtime_prompt(state.clone(), &pool, row, prompt, runtime_model).await {
                Ok(()) => {
                    let _ = session_control::repository::transition(
                        &task_pool,
                        &task_turn_id,
                        "completed",
                        None,
                    )
                    .await;
                }
                Err(error) => {
                    let _ = session_control::repository::transition(
                        &task_pool,
                        &task_turn_id,
                        "failed",
                        Some(json!({"code": "runtime_error", "message": error.to_string()})),
                    )
                    .await;
                    let _ = crate::db::managed_agents::tasks::repository::fail_for_session(
                        &task_pool,
                        &session_id,
                        &error.to_string(),
                    )
                    .await;
                    let _ = runtime_lifecycle::mark_session_error(
                        &state,
                        &task_pool,
                        &session_id,
                        error.to_string(),
                    )
                    .await;
                    record_prompt_error(&state, &session_id, error);
                }
            }
        });
        return Ok(());
    }

    let task_pool = pool.clone();
    let task_turn_id = turn_id;
    tokio::spawn(async move {
        match execute_prompt(state.clone(), pool, row, prompt, model).await {
            Ok(()) => {
                let _ = session_control::repository::transition(
                    &task_pool,
                    &task_turn_id,
                    "completed",
                    None,
                )
                .await;
            }
            Err(error) => {
                let _ = session_control::repository::transition(
                    &task_pool,
                    &task_turn_id,
                    "failed",
                    Some(json!({"code": "execution_error", "message": error.to_string()})),
                )
                .await;
                let _ = crate::db::managed_agents::tasks::repository::fail_for_session(
                    &task_pool,
                    &session_id,
                    &error.to_string(),
                )
                .await;
                record_prompt_error(&state, &session_id, error);
            }
        }
    });

    Ok(())
}

pub async fn send_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<PromptRequest>,
) -> Result<Json<Vec<MessageResponse>>, GatewayError> {
    prompt_async(
        State(state.clone()),
        headers.clone(),
        Path(session_id.clone()),
        Json(input),
    )
    .await?;
    let pool = db(&state, &headers).await?;
    let rows = messages::repository::list(pool, &session_id).await?;
    rows.into_iter()
        .map(MessageResponse::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

pub async fn abort(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<StatusCode, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    let row = owned_session(pool, &auth, &session_id).await?;
    abort_session_internal(&state, pool, &row, "aborted").await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Shared by the user-facing abort endpoint and internal callers that don't
/// have (or need) an HTTP-authenticated principal — e.g. the Guardian
/// reviewer's circuit breaker interrupting a turn after repeated denials.
pub(crate) async fn abort_session_internal(
    state: &AppState,
    pool: &sqlx::PgPool,
    row: &sessions::schema::SessionRow,
    reason: &str,
) -> Result<(), GatewayError> {
    let session_id = &row.id;
    let active_turn = session_control::repository::active_turn(pool, session_id).await?;
    if let Some(snapshot) = active_turn.as_ref() {
        session_control::repository::transition(pool, &snapshot.turn.id, "cancelling", None)
            .await?;
    }
    let _ = interrupt_runtime_session(state, pool, row).await;
    if let Some(snapshot) = active_turn {
        session_control::repository::transition(
            pool,
            &snapshot.turn.id,
            "cancelled",
            Some(json!({"code": "user_cancelled", "message": reason})),
        )
        .await?;
    }
    sessions::repository::set_status(pool, session_id, "idle").await?;
    state.agent_runs.set_error(session_id, reason.to_owned());
    state.agent_runs.push_event(
        session_id,
        events::SESSION_ERROR,
        json!({ "error": { "name": "MessageAbortedError", "message": reason } }),
    );
    state
        .agent_runs
        .push_event(session_id, events::SESSION_IDLE, json!({}));
    let _ =
        crate::db::managed_agents::tasks::repository::cancel_for_session(pool, session_id).await;
    Ok(())
}

fn request_id(headers: &HeaderMap, body_request_id: Option<&str>) -> String {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(body_request_id)
        .map(str::to_owned)
        .unwrap_or_else(|| crate::db::managed_agents::id("req"))
}

fn trace_headers(headers: &HeaderMap) -> (Option<String>, Option<String>) {
    let value = |name: &str, max_len: usize| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= max_len)
            .map(str::to_owned)
    };
    (value("traceparent", 55), value("tracestate", 512))
}

pub(crate) async fn interrupt_runtime_session(
    state: &AppState,
    pool: &sqlx::PgPool,
    row: &sessions::schema::SessionRow,
) -> bool {
    let Some(runtime) = row.runtime.as_deref() else {
        return false;
    };
    if external_bridge::supports(runtime) {
        return external_bridge::cancel(state, pool, row).await.is_ok();
    }
    let Ok(resolved) =
        crate::http::runtime_resolution::resolve_runtime_for_session(pool, state, runtime, row)
            .await
    else {
        return false;
    };
    let Ok(client) = runtime_sdk_client(&resolved) else {
        return false;
    };
    if register_runtime_session(&client, pool, row, &resolved)
        .await
        .is_err()
    {
        return false;
    }
    client
        .beta()
        .sessions()
        .events()
        .interrupt(&row.id)
        .await
        .is_ok()
}

pub async fn interrupt(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<StatusCode, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    let Ok(row) = owned_session(pool, &auth, &session_id).await else {
        return Ok(StatusCode::NO_CONTENT);
    };
    abort_session_internal(&state, pool, &row, "interrupted by user").await?;
    Ok(StatusCode::NO_CONTENT)
}

fn record_prompt_error(state: &AppState, session_id: &str, error: GatewayError) {
    let message = error.to_string();
    state.agent_runs.set_error(session_id, message.clone());
    state.agent_runs.push_event(
        session_id,
        events::SESSION_ERROR,
        json!({ "error": { "message": message } }),
    );
    state
        .agent_runs
        .push_event(session_id, events::SESSION_IDLE, json!({}));
}
