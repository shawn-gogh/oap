use std::sync::Arc;

use serde_json::{json, Value};
use sqlx::PgPool;

use crate::{
    db::managed_agents::{
        registry::{self, schema::ManagedAgentRow},
        sessions::{self, schema::SessionRow},
    },
    errors::GatewayError,
    proxy::state::AppState,
};

use super::{
    runtime_lifecycle::{
        mark_session_error, mark_session_status, persist_send_response_events, provider_run_status,
        update_agent_run_status,
    },
    runtime_provision::provision_runtime_session,
    runtime_sdk::{agent_sdk_error, register_runtime_session, send_events_params},
    runtime_vault::resolve_agent_vault_keys,
    storage::persist_message,
    types::{CreateSessionRequest, SessionResponse},
};

pub(super) struct CreatedRuntimeSession {
    pub(super) runtime: String,
    pub(super) resolved: crate::http::runtime_resolution::ResolvedRuntime,
    pub(super) agent: ManagedAgentRow,
    pub(super) environment: Value,
    pub(super) initial_user_prompt: Option<String>,
    pub(super) prompt: String,
    pub(super) row: SessionRow,
}

pub(super) async fn create_runtime_session(
    state: Arc<AppState>,
    pool: &PgPool,
    input: CreateSessionRequest,
    owner: Option<&str>,
) -> Result<SessionResponse, GatewayError> {
    // generic_chat harnesses have no managed-agents provider to provision
    // against; the gateway itself is the runtime.
    if let Some(alias) = input.runtime.as_deref() {
        if super::generic_chat::is_generic_chat(pool, alias).await? {
            return create_generic_chat_session(state, pool, input, owner).await;
        }
    }
    let mut created = create_runtime_session_row(&state, pool, input, owner).await?;
    // Workspaces are opt-in per runtime — only local-opencode's per-session
    // process model (see runtime_provision.rs) can actually mount one today.
    if created.resolved.alias == "local-opencode" {
        if let Some(storage) = &state.object_storage {
            let bucket = crate::object_storage::ObjectStorageClient::bucket_name(&created.row.id);
            storage.ensure_bucket(&bucket).await?;
            // Seed the session workspace with the agent's knowledge/template
            // files before provisioning, so they're present when opencode
            // mounts the bucket. Session edits never write back.
            let agent_bucket =
                crate::object_storage::ObjectStorageClient::agent_bucket_name(&created.agent.id);
            storage.copy_all(&agent_bucket, &bucket).await?;
            sessions::repository::set_workspace_bucket(pool, &created.row.id, &bucket).await?;
            created.row.workspace_bucket = Some(bucket);
        }
    }
    if let Some(prompt) = created.initial_user_prompt.as_deref() {
        persist_message(pool, &created.row.id, "user", prompt, None).await?;
    }
    let mut row = match provision_runtime_session(&state, pool, &created).await {
        Ok(row) => row,
        Err(error) => {
            if created.row.task_id.is_some() {
                let _ = sessions::repository::set_status(pool, &created.row.id, "error").await;
                let _ = crate::db::managed_agents::tasks::repository::fail_for_session(
                    pool,
                    &created.row.id,
                    &error.to_string(),
                )
                .await;
            } else {
                let _ = sessions::repository::delete(pool, &created.row.id).await;
            }
            return Err(error);
        }
    };
    state.agent_runs.track_run(&created.agent.id, &row.id);
    if let Some(task_id) = row.task_id.as_deref() {
        match row.status.as_str() {
            "idle" => {
                crate::db::managed_agents::tasks::repository::mark_verifying_for_session(
                    pool, &row.id,
                )
                .await?;
            }
            "error" => {
                crate::db::managed_agents::tasks::repository::fail_for_session(
                    pool,
                    &row.id,
                    "runtime session provisioning failed",
                )
                .await?;
            }
            _ if created.initial_user_prompt.is_some() => {
                crate::db::managed_agents::tasks::repository::mark_running_for_session(
                    pool, &row.id,
                )
                .await?;
            }
            _ => {
                crate::db::managed_agents::tasks::repository::mark_waiting_input(pool, task_id)
                    .await?;
            }
        }
    }
    if row.provider_run_id.is_none() {
        if let Some(prompt) = created.initial_user_prompt.as_deref() {
            execute_runtime_prompt(state.clone(), pool, row.clone(), prompt.to_owned(), None)
                .await?;
        } else {
            sessions::repository::set_status(pool, &row.id, "idle").await?;
            state
                .agent_runs
                .update_status(&row.id, crate::agents::runs::AgentRunStatus::Completed);
            row.status = "idle".to_owned();
        }
    }
    Ok(SessionResponse::from(row))
}

async fn create_generic_chat_session(
    state: Arc<AppState>,
    pool: &PgPool,
    input: CreateSessionRequest,
    owner: Option<&str>,
) -> Result<SessionResponse, GatewayError> {
    let alias = input.runtime.clone().unwrap_or_default();
    let agent = load_agent(pool, &input).await?;
    let title = input.title.clone().unwrap_or_else(|| agent.name.clone());
    let prompt = input
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .map(str::to_owned);
    let agent_id = input.agent_id.as_deref().or(input.agent.as_deref());
    let mut environment = input.environment.clone().unwrap_or_else(|| json!({}));
    if agent_id.is_none() {
        environment["temporary_model"] = json!(agent.model);
        environment["temporary_system"] = json!(agent.system);
    }
    let row = sessions::repository::create_runtime(
        pool,
        sessions::repository::CreateRuntimeSession {
            runtime: &alias,
            agent_id,
            title: &title,
            timezone: input.timezone.as_deref().or(input.tz.as_deref()),
            runtime_agent_ref_id: None,
            environment,
            provider_session_id: None,
            provider_run_id: None,
            owner_id: Some(owner.or(agent.owner_id.as_deref()).unwrap_or("system")),
            task_id: input.task_id.as_deref(),
        },
    )
    .await?;
    state.agent_runs.track_run(&agent.id, &row.id);
    if let Some(prompt) = prompt {
        if row.task_id.is_some() {
            crate::db::managed_agents::tasks::repository::mark_running_for_session(pool, &row.id)
                .await?;
        }
        persist_message(pool, &row.id, "user", &prompt, None).await?;
        let state = state.clone();
        let pool_bg = pool.clone();
        let row_bg = row.clone();
        tokio::spawn(async move {
            if let Err(error) =
                super::generic_chat::execute_prompt(state, &pool_bg, &row_bg, &prompt).await
            {
                tracing::warn!(session_id = %row_bg.id, %error, "generic chat prompt failed");
            }
        });
        let mut row = row;
        row.status = "running".to_owned();
        return Ok(SessionResponse::from(row));
    }
    sessions::repository::set_status(pool, &row.id, "idle").await?;
    if let Some(task_id) = row.task_id.as_deref() {
        crate::db::managed_agents::tasks::repository::mark_waiting_input(pool, task_id).await?;
    }
    let mut row = row;
    row.status = "idle".to_owned();
    Ok(SessionResponse::from(row))
}

pub(crate) async fn create_runtime_session_for_agent(
    state: Arc<AppState>,
    pool: &PgPool,
    agent_id: String,
    runtime: String,
    title: String,
    prompt: String,
    environment: Value,
) -> Result<String, GatewayError> {
    create_runtime_session_for_agent_input(
        state,
        pool,
        agent_id,
        runtime,
        title,
        Some(prompt),
        environment,
        None,
    )
    .await
}

pub(crate) async fn create_runtime_session_for_agent_without_prompt(
    state: Arc<AppState>,
    pool: &PgPool,
    agent_id: String,
    runtime: String,
    title: String,
    environment: Value,
) -> Result<String, GatewayError> {
    create_runtime_session_for_agent_input(
        state,
        pool,
        agent_id,
        runtime,
        title,
        None,
        environment,
        None,
    )
    .await
}

pub(crate) async fn create_runtime_session_for_agent_task(
    state: Arc<AppState>,
    pool: &PgPool,
    agent_id: String,
    runtime: String,
    title: String,
    environment: Value,
    task_id: String,
) -> Result<String, GatewayError> {
    create_runtime_session_for_agent_input(
        state,
        pool,
        agent_id,
        runtime,
        title,
        None,
        environment,
        Some(task_id),
    )
    .await
}

pub(crate) async fn create_runtime_session_for_agent_task_with_prompt(
    state: Arc<AppState>,
    pool: &PgPool,
    agent_id: String,
    runtime: String,
    title: String,
    prompt: String,
    environment: Value,
    task_id: String,
) -> Result<String, GatewayError> {
    let response = create_runtime_session(
        state,
        pool,
        CreateSessionRequest {
            title: Some(title),
            harness: None,
            agent: Some(agent_id.clone()),
            agent_id: Some(agent_id),
            runtime: Some(runtime),
            model: None,
            prompt: Some(prompt),
            environment: Some(environment),
            timezone: None,
            tz: None,
            task_id: Some(task_id),
        },
        None,
    )
    .await?;
    Ok(response.id().to_owned())
}

async fn create_runtime_session_for_agent_input(
    state: Arc<AppState>,
    pool: &PgPool,
    agent_id: String,
    runtime: String,
    title: String,
    prompt: Option<String>,
    environment: Value,
    task_id: Option<String>,
) -> Result<String, GatewayError> {
    let runtime = registry::repository::get(pool, &agent_id)
        .await?
        .and_then(|agent| runtime_from_agent_config(&agent))
        .unwrap_or(runtime);
    let response = create_runtime_session(
        state,
        pool,
        CreateSessionRequest {
            title: Some(title),
            harness: None,
            agent: Some(agent_id.clone()),
            agent_id: Some(agent_id),
            runtime: Some(runtime),
            model: None,
            prompt,
            environment: Some(environment),
            timezone: None,
            tz: None,
            task_id,
        },
        None,
    )
    .await?;
    Ok(response.id().to_owned())
}

fn runtime_from_agent_config(agent: &ManagedAgentRow) -> Option<String> {
    agent
        .config
        .get("runtime")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

async fn create_runtime_session_row(
    state: &AppState,
    pool: &PgPool,
    input: CreateSessionRequest,
    owner: Option<&str>,
) -> Result<CreatedRuntimeSession, GatewayError> {
    let mut agent = load_agent(pool, &input).await?;
    let alias = input.runtime.as_deref().unwrap_or_default();
    let resolved =
        crate::http::runtime_resolution::resolve_runtime_for_agent(pool, state, alias, &agent)
            .await?;
    let runtime = resolved.alias.clone();
    agent.system =
        crate::db::managed_agents::skills::compose::compose_agent_system_prompt(pool, &agent)
            .await?;
    let mut stored_environment = input.environment.clone().unwrap_or_else(|| json!({}));
    if input.agent_id.is_none() && input.agent.is_none() {
        stored_environment["temporary_model"] = json!(agent.model);
        stored_environment["temporary_system"] = json!(agent.system);
    }
    let title = input.title.clone().unwrap_or_else(|| agent.name.clone());
    let initial_user_prompt = input
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .map(str::to_owned);
    let agent_id = input.agent_id.as_deref().or(input.agent.as_deref());
    let row = sessions::repository::create_runtime(
        pool,
        sessions::repository::CreateRuntimeSession {
            runtime: &runtime,
            agent_id,
            title: &title,
            timezone: input.timezone.as_deref().or(input.tz.as_deref()),
            runtime_agent_ref_id: None,
            environment: stored_environment.clone(),
            provider_session_id: None,
            provider_run_id: None,
            // Channel/routine-originated sessions carry no caller identity;
            // they inherit the agent's owner (or "system" for legacy agents).
            owner_id: Some(owner.or(agent.owner_id.as_deref()).unwrap_or("system")),
            task_id: input.task_id.as_deref(),
        },
    )
    .await?;
    let mut provision_environment = stored_environment;
    resolve_agent_vault_keys(state, pool, &agent, &mut provision_environment).await?;
    let prompt = runtime_prompt(input.prompt, &agent);
    Ok(CreatedRuntimeSession {
        runtime,
        resolved,
        agent,
        environment: provision_environment,
        initial_user_prompt,
        prompt,
        row,
    })
}

pub(super) async fn execute_runtime_prompt(
    state: Arc<AppState>,
    pool: &PgPool,
    row: SessionRow,
    prompt: String,
    model: Option<String>,
) -> Result<(), GatewayError> {
    let runtime = row.runtime.as_deref().ok_or_else(|| {
        GatewayError::InvalidConfig("runtime session is missing runtime".to_owned())
    })?;
    if super::generic_chat::is_generic_chat(pool, runtime).await? {
        return super::generic_chat::execute_prompt(state, pool, &row, &prompt).await;
    }
    let resolved =
        crate::http::runtime_resolution::resolve_runtime_for_session(pool, &state, runtime, &row)
            .await?;
    let client = super::runtime_sdk::lap_from_credential(&resolved)?;
    if let Err(error) = register_runtime_session(&client, pool, &row, &resolved).await {
        mark_session_error(&state, pool, &row.id, error.to_string()).await?;
        return Err(error);
    }
    state
        .agent_runs
        .update_status(&row.id, crate::agents::runs::AgentRunStatus::Running);
    super::runtime_lifecycle::emit_runtime_stage(&state, pool, &row.id, "submitting").await?;
    let sent = match client
        .beta()
        .sessions()
        .events()
        .send_with_model(&row.id, model, send_events_params(prompt))
        .await
    {
        Ok(sent) => sent,
        Err(error) => {
            let error = agent_sdk_error(error);
            mark_session_error(&state, pool, &row.id, error.to_string()).await?;
            return Err(error);
        }
    };
    let status = provider_run_status(&sent.raw);
    if let Some(run_id) = resolved.adapter.provider_run_id_from_agent_raw(&sent.raw) {
        sessions::repository::set_provider_run(pool, &row.id, &run_id, status).await?;
        update_agent_run_status(&state, &row.id, status, &sent.raw);
    }
    persist_send_response_events(pool, &resolved, &row.id, &sent.raw).await?;
    if status == "running" {
        super::runtime_lifecycle::emit_runtime_stage(&state, pool, &row.id, "running").await?;
        let stream = match client.beta().sessions().events().stream(&row.id).await {
            Ok(stream) => stream,
            Err(error) => {
                let error = agent_sdk_error(error);
                mark_session_error(&state, pool, &row.id, error.to_string()).await?;
                return Err(error);
            }
        };
        super::runtime_events_api::replace_provider_consumer(&state, pool, &row.id, stream);
    } else {
        mark_session_status(&state, pool, &row.id, status, None).await?;
    }
    Ok(())
}

async fn load_agent(
    pool: &PgPool,
    input: &CreateSessionRequest,
) -> Result<ManagedAgentRow, GatewayError> {
    let Some(agent_id) = input.agent_id.clone().or(input.agent.clone()) else {
        let model = input
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .ok_or(GatewayError::MissingModel)?;
        return Ok(ManagedAgentRow {
            id: "temporary-session".to_owned(),
            name: input
                .title
                .clone()
                .unwrap_or_else(|| "Temporary session".to_owned()),
            model: model.to_owned(),
            system: "You are a helpful assistant. Use available tools when they help complete the user's request.".to_owned(),
            tools: json!([{ "type": "agent_toolset_20260401" }]),
            cadence: None,
            interval_seconds: None,
            session_id: None,
            loop_id: None,
            created_at: crate::db::managed_agents::now_ms(),
            prompt: None,
            cron: None,
            timezone: "UTC".to_owned(),
            vault_keys: json!([]),
            setup_commands: json!([]),
            max_runtime_minutes: 30,
            on_failure: "pause_and_notify".to_owned(),
            config: json!({ "runtime": input.runtime, "temporary": true }),
            owner_id: None,
            status: "active".to_owned(),
            description: Some("Temporary session".to_owned()),
            harness: input.runtime.clone().unwrap_or_default(),
            skill_ids: json!([]),
            rule_ids: json!([]),
        });
    };
    registry::repository::get(pool, &agent_id)
        .await?
        .ok_or(GatewayError::UnknownAgent(agent_id))
}

fn runtime_prompt(prompt: Option<String>, agent: &ManagedAgentRow) -> String {
    prompt
        .filter(|prompt| !prompt.trim().is_empty())
        .unwrap_or_else(|| format!("Start a session for {}.", agent.name))
}
