use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::{json, Value};

use crate::{
    agents::events,
    db::managed_agents::{messages, sessions},
    errors::GatewayError,
    proxy::state::AppState,
};

mod execution;
mod generic_chat;
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
use storage::{auth_db, db, owned_session, persist_message, resolve_session_request, session};
pub use types::{CreateSessionRequest, MessageResponse, PromptRequest, SessionResponse};
pub use workspace_api::{
    batch_delete_files, batch_transfer_files, browse_files, copy_files, create_folder,
    create_upload_url, delete_file, delete_workspace_trash, download_url, empty_workspace_trash,
    list_files, list_folders, list_workspace_trash, move_files, restore_workspace_trash,
    trash_workspace_paths,
};

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<SessionResponse>>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    let owner_filter = (!auth.is_admin).then_some(auth.user_id.as_str());
    let rows = sessions::repository::list(pool, owner_filter).await?;
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
        return match create_runtime_session(state, &pool, input, Some(&auth.user_id)).await {
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
    let resolved = resolve_session_request(&state, &pool, input).await?;
    let row = sessions::repository::create(
        &pool,
        &resolved.harness,
        resolved.agent_id.as_deref(),
        &resolved.title,
        resolved.timezone.as_deref(),
        Some(&auth.user_id),
        session_task_id.as_deref(),
    )
    .await?;
    if let Some(task_id) = session_task_id.as_deref() {
        crate::db::managed_agents::tasks::repository::mark_waiting_input(&pool, task_id).await?;
    }
    state.agent_runs.track_run(&resolved.harness, &row.id);
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
        if let Some(bucket) = row.workspace_bucket.as_deref() {
            // Best-effort: a storage hiccup shouldn't block deleting the session row.
            let _ = storage.delete_bucket_recursive(bucket).await;
        }
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
    let prompt = input.prompt_text()?;
    let model = input
        .model_id()
        .ok_or(GatewayError::MissingModel)?
        .to_owned();
    let runtime_model = Some(model.clone());
    enqueue_prompt_text_with_runtime_model(state, pool, &session_id, prompt, model, runtime_model)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn enqueue_prompt_text(
    state: Arc<AppState>,
    pool: sqlx::PgPool,
    session_id: &str,
    prompt: String,
    model: String,
) -> Result<(), GatewayError> {
    enqueue_prompt_text_with_runtime_model(state, pool, session_id, prompt, model, None).await
}

async fn enqueue_prompt_text_with_runtime_model(
    state: Arc<AppState>,
    pool: sqlx::PgPool,
    session_id: &str,
    prompt: String,
    model: String,
    runtime_model: Option<String>,
) -> Result<(), GatewayError> {
    let session_id = session_id.to_owned();
    let row = session(&pool, &session_id).await?;

    persist_message(&pool, &session_id, "user", &prompt, None).await?;
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
        tokio::spawn(async move {
            if let Err(error) =
                execute_runtime_prompt(state.clone(), &pool, row, prompt, runtime_model).await
            {
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
        });
        return Ok(());
    }

    let task_pool = pool.clone();
    tokio::spawn(async move {
        if let Err(error) = execute_prompt(state.clone(), pool, row, prompt, model).await {
            let _ = crate::db::managed_agents::tasks::repository::fail_for_session(
                &task_pool,
                &session_id,
                &error.to_string(),
            )
            .await;
            record_prompt_error(&state, &session_id, error);
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
    let _ = interrupt_runtime_session(&state, pool, &row).await;
    sessions::repository::set_status(pool, &session_id, "cancelled").await?;
    state
        .agent_runs
        .set_error(&session_id, "aborted".to_owned());
    state.agent_runs.push_event(
        &session_id,
        events::SESSION_ERROR,
        json!({ "error": { "name": "MessageAbortedError", "message": "aborted" } }),
    );
    state
        .agent_runs
        .push_event(&session_id, events::SESSION_IDLE, json!({}));
    let _ =
        crate::db::managed_agents::tasks::repository::cancel_for_session(pool, &session_id).await;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn interrupt_runtime_session(
    state: &AppState,
    pool: &sqlx::PgPool,
    row: &sessions::schema::SessionRow,
) -> bool {
    let Some(runtime) = row.runtime.as_deref() else {
        return false;
    };
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
    let Some(runtime) = row.runtime.as_deref() else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let Ok(resolved) =
        crate::http::runtime_resolution::resolve_runtime_for_session(pool, &state, runtime, &row)
            .await
    else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let Ok(client) = runtime_sdk_client(&resolved) else {
        return Ok(StatusCode::NO_CONTENT);
    };
    if register_runtime_session(&client, pool, &row, &resolved)
        .await
        .is_ok()
    {
        let _ = client
            .beta()
            .sessions()
            .events()
            .interrupt(&session_id)
            .await;
    }
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
