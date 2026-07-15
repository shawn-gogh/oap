use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    db::managed_agents::{
        registry,
        tasks::{
            repository,
            schema::{
                AgentTaskRow, CreateArtifactRequest, CreateTaskRequest, NewArtifact, NewTask,
                ResumeTaskRequest, RetryTaskRequest, TaskCancellation, UpdateAcceptanceRequest,
            },
        },
    },
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

pub mod timeout;

#[derive(Debug, Deserialize)]
pub struct ListTasksQuery {
    limit: Option<i64>,
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<CreateTaskRequest>,
) -> Result<Json<AgentTaskRow>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    super::assert_agent_use(&auth, &agent, pool).await?;
    let source = input.source.as_deref().unwrap_or("api");
    if !matches!(source, "manual" | "api" | "test") {
        return Err(GatewayError::BadRequest(
            "task source must be manual, api, or test".to_owned(),
        ));
    }
    let task_input = input.input.unwrap_or_else(|| json!({}));
    if !task_input.is_object() {
        return Err(GatewayError::BadRequest(
            "task input must be a JSON object".to_owned(),
        ));
    }
    validate_task_input(&agent.config, &task_input)?;
    let title = input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{} task", agent.name));
    Ok(Json(
        repository::create(
            pool,
            NewTask {
                agent_id: &agent.id,
                application_version: application_version(&agent.config),
                source,
                source_id: None,
                title: &title,
                input: task_input,
                created_by: &auth.user_id,
                completion_criteria: completion_criteria(&agent.config),
            },
        )
        .await?,
    ))
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    super::assert_agent_use(&auth, &agent, pool).await?;
    Ok(Json(json!({
        "tasks": repository::list(pool, &agent_id, query.limit.unwrap_or(20)).await?
    })))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, task_id)): Path<(String, String)>,
) -> Result<Json<AgentTaskRow>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    super::assert_agent_use(&auth, &agent, pool).await?;
    repository::get(pool, &agent_id, &task_id)
        .await?
        .map(Json)
        .ok_or_else(|| GatewayError::NotFound("task not found".to_owned()))
}

pub async fn list_attempts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, task_id)): Path<(String, String)>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    super::assert_agent_use(&auth, &agent, pool).await?;
    ensure_task(pool, &agent_id, &task_id).await?;
    let sessions =
        crate::db::managed_agents::sessions::repository::list_for_task(pool, &task_id).await?;
    let runs = crate::db::managed_agents::runs::repository::list_for_task(pool, &task_id).await?;
    Ok(Json(json!({
        "sessions": sessions,
        "runs": runs,
        "artifacts": crate::db::managed_agents::tasks::artifacts::list(pool, &task_id).await?,
        "acceptance_checks": crate::db::managed_agents::tasks::acceptance::list_all(pool, &task_id).await?,
        "max_attempts": max_attempts(&agent.config)
    })))
}

pub async fn list_artifacts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, task_id)): Path<(String, String)>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    super::assert_agent_use(&auth, &agent, pool).await?;
    let task = ensure_task(pool, &agent_id, &task_id).await?;
    Ok(Json(json!({
        "artifacts": crate::db::managed_agents::tasks::artifacts::list_for_attempt(
            pool,
            &task_id,
            task.current_attempt_number
        ).await?
    })))
}

pub async fn create_artifact(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, task_id)): Path<(String, String)>,
    Json(input): Json<CreateArtifactRequest>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    let task = ensure_task(pool, &agent_id, &task_id).await?;
    if task.created_by != auth.user_id {
        super::assert_agent_edit(&auth, &agent, pool).await?;
    }
    if !matches!(task.status.as_str(), "running" | "verifying") {
        return Err(GatewayError::BadRequest(format!(
            "task artifacts can only be added from running or verifying status, current status is {}",
            task.status
        )));
    }
    let artifact_type = required_text(&input.artifact_type, "artifact_type")?;
    let name = required_text(&input.name, "name")?;
    let location = input
        .location
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if input.content.is_none() && location.is_none() {
        return Err(GatewayError::BadRequest(
            "artifact content or location is required".to_owned(),
        ));
    }
    if let Some(session_id) = input.session_id.as_deref() {
        let session = crate::db::managed_agents::sessions::repository::get(pool, session_id)
            .await?
            .ok_or_else(|| GatewayError::NotFound("session not found".to_owned()))?;
        if session.task_id.as_deref() != Some(task_id.as_str()) {
            return Err(GatewayError::BadRequest(
                "session does not belong to this task".to_owned(),
            ));
        }
        if session.attempt_number != task.current_attempt_number {
            return Err(GatewayError::BadRequest(
                "artifacts can only be added to the current task attempt".to_owned(),
            ));
        }
    }
    let artifact = crate::db::managed_agents::tasks::artifacts::create(
        pool,
        NewArtifact {
            task_id: &task_id,
            session_id: input.session_id.as_deref(),
            run_id: None,
            artifact_type,
            name,
            content: input.content,
            location,
            dedupe_key: None,
            created_by: &auth.user_id,
        },
    )
    .await?;
    crate::db::managed_agents::tasks::acceptance::reconcile(pool, &task_id).await?;
    Ok(Json(json!({ "artifact": artifact })))
}

pub async fn list_acceptance(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, task_id)): Path<(String, String)>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    super::assert_agent_use(&auth, &agent, pool).await?;
    ensure_task(pool, &agent_id, &task_id).await?;
    Ok(Json(json!({
        "checks": crate::db::managed_agents::tasks::acceptance::list(pool, &task_id).await?
    })))
}

pub async fn update_acceptance(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, task_id)): Path<(String, String)>,
    Json(input): Json<UpdateAcceptanceRequest>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    let task = ensure_task(pool, &agent_id, &task_id).await?;
    if task.created_by != auth.user_id {
        super::assert_agent_edit(&auth, &agent, pool).await?;
    }
    if task.status != "verifying" {
        return Err(GatewayError::BadRequest(format!(
            "task acceptance can only be recorded from verifying status, current status is {}",
            task.status
        )));
    }
    if !matches!(input.verdict.as_str(), "passed" | "failed") {
        return Err(GatewayError::BadRequest(
            "acceptance verdict must be passed or failed".to_owned(),
        ));
    }
    let checks = crate::db::managed_agents::tasks::acceptance::list(pool, &task_id).await?;
    let criterion = if checks.is_empty() {
        if input.criterion_index != 0 {
            return Err(GatewayError::BadRequest(
                "legacy task manual acceptance must use criterion_index 0".to_owned(),
            ));
        }
        Some(required_text(
            input.criterion.as_deref().unwrap_or(""),
            "criterion",
        )?)
    } else {
        if !checks
            .iter()
            .any(|check| check.criterion_index == input.criterion_index)
        {
            return Err(GatewayError::NotFound(
                "acceptance criterion not found".to_owned(),
            ));
        }
        None
    };
    crate::db::managed_agents::tasks::acceptance::record(
        pool,
        &task_id,
        input.criterion_index,
        criterion,
        &input.verdict,
        input.evidence.as_deref(),
        &auth.user_id,
    )
    .await?;
    crate::db::managed_agents::tasks::acceptance::reconcile(pool, &task_id).await?;
    Ok(Json(json!({
        "task": ensure_task(pool, &agent_id, &task_id).await?,
        "checks": crate::db::managed_agents::tasks::acceptance::list(pool, &task_id).await?
    })))
}

pub async fn resume(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, task_id)): Path<(String, String)>,
    Json(input): Json<ResumeTaskRequest>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    super::assert_agent_use(&auth, &agent, pool).await?;
    let task = ensure_task(pool, &agent_id, &task_id).await?;
    if task.created_by != auth.user_id {
        super::assert_agent_edit(&auth, &agent, pool).await?;
    }
    if task.status != "waiting_input" {
        return Err(GatewayError::BadRequest(format!(
            "task can only resume from waiting_input status, current status is {}",
            task.status
        )));
    }
    let patch = input
        .input
        .as_object()
        .ok_or_else(|| GatewayError::BadRequest("task input must be a JSON object".to_owned()))?;
    let mut merged = task.input_json.as_object().cloned().unwrap_or_default();
    merged.extend(patch.clone());
    let merged = Value::Object(merged);
    validate_task_input(&agent.config, &merged)?;
    let session = crate::db::managed_agents::sessions::repository::latest_for_task(pool, &task_id)
        .await?
        .ok_or_else(|| {
            GatewayError::BadRequest(
                "task has no session to resume; start it with task_id first".to_owned(),
            )
        })?;
    repository::merge_input(pool, &task_id, Value::Object(patch.clone())).await?;
    let prompt = format!(
        "Continue the existing task using these supplied inputs:\n{}",
        serde_json::to_string_pretty(&merged)?
    );
    crate::http::sessions::enqueue_prompt_text(
        state.clone(),
        pool.clone(),
        &session.id,
        prompt,
        agent.model,
    )
    .await?;
    Ok(Json(json!({
        "task": ensure_task(pool, &agent_id, &task_id).await?,
        "session_id": session.id
    })))
}

pub async fn retry(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, task_id)): Path<(String, String)>,
    Json(input): Json<RetryTaskRequest>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    super::assert_agent_use(&auth, &agent, pool).await?;
    let task = ensure_task(pool, &agent_id, &task_id).await?;
    if task.created_by != auth.user_id {
        super::assert_agent_edit(&auth, &agent, pool).await?;
    }
    if task.status != "failed" {
        return Err(GatewayError::BadRequest(format!(
            "task can only retry from failed status, current status is {}",
            task.status
        )));
    }
    let previous = crate::db::managed_agents::sessions::repository::latest_for_task(pool, &task_id)
        .await?
        .ok_or_else(|| {
            GatewayError::BadRequest(
                "only runtime-backed tasks with an existing session can be retried".to_owned(),
            )
        })?;
    let runtime = input
        .runtime
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| previous.runtime.clone())
        .or_else(|| runtime_from_config(&agent.config))
        .unwrap_or_else(|| agent.harness.clone());
    let prompt = retry_prompt(&task);
    repository::prepare_retry(pool, &task_id, max_attempts(&agent.config)).await?;
    let session_result = crate::http::sessions::create_runtime_session_for_agent_task_with_prompt(
        state.clone(),
        pool,
        agent.id.clone(),
        runtime,
        format!("{} · retry", task.title),
        prompt,
        previous.environment_json,
        task_id.clone(),
    )
    .await;
    let session_id = match session_result {
        Ok(session_id) => session_id,
        Err(error) => {
            let _ = repository::fail(pool, &task_id, &error.to_string()).await;
            return Err(error);
        }
    };
    let session = crate::db::managed_agents::sessions::repository::get(pool, &session_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("retry session not found".to_owned()))?;
    Ok(Json(json!({
        "task": ensure_task(pool, &agent_id, &task_id).await?,
        "session": session
    })))
}

pub async fn cancel(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, task_id)): Path<(String, String)>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    super::assert_agent_use(&auth, &agent, pool).await?;
    let task = ensure_task(pool, &agent_id, &task_id).await?;
    if task.created_by != auth.user_id {
        super::assert_agent_edit(&auth, &agent, pool).await?;
    }
    let cancellation = repository::cancel(pool, &task_id).await?;
    let interruption = terminate_task_execution(&state, pool, &cancellation, "cancelled").await;
    Ok(Json(json!({
        "task": cancellation.task,
        "session_id": cancellation.session_id,
        "run_id": cancellation.run_id,
        "interruption": interruption
    })))
}

pub(crate) async fn terminate_task_execution(
    state: &AppState,
    pool: &sqlx::PgPool,
    cancellation: &TaskCancellation,
    terminal_message: &str,
) -> &'static str {
    let mut interruption = "not_running";
    if let Some(session_id) = cancellation.session_id.as_deref() {
        let _ = crate::db::managed_agents::inbox::repository::expire_pending_for_session(
            pool, session_id,
        )
        .await;
        if let Some(session) =
            crate::db::managed_agents::sessions::repository::get(pool, session_id)
                .await
                .ok()
                .flatten()
        {
            interruption =
                if crate::http::sessions::interrupt_runtime_session(state, pool, &session).await {
                    "provider_interrupted"
                } else {
                    "cooperative"
                };
        }
        state
            .agent_runs
            .set_error(session_id, terminal_message.to_owned());
    } else if let Some(run_id) = cancellation.run_id.as_deref() {
        interruption = "cooperative";
        if let Some(run) = crate::db::managed_agents::runs::repository::get(
            pool,
            &cancellation.task.agent_id,
            run_id,
        )
        .await
        .ok()
        .flatten()
        {
            if let Some(sandbox_id) = run.sandbox_id.as_deref() {
                if let Ok(runner) = crate::agents::sandboxes::SandboxRunner::from_settings(
                    state.http.clone(),
                    &state.config.general_settings,
                ) {
                    if runner.terminate_by_id(sandbox_id).await.unwrap_or(false) {
                        interruption = "sandbox_terminated";
                    }
                }
            }
        }
        state
            .agent_runs
            .set_error(run_id, terminal_message.to_owned());
    }
    interruption
}

async fn ensure_task(
    pool: &sqlx::PgPool,
    agent_id: &str,
    task_id: &str,
) -> Result<AgentTaskRow, GatewayError> {
    repository::get(pool, agent_id, task_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("task not found".to_owned()))
}

fn required_text<'a>(value: &'a str, field: &str) -> Result<&'a str, GatewayError> {
    let value = value.trim();
    if value.is_empty() {
        Err(GatewayError::BadRequest(format!("{field} is required")))
    } else {
        Ok(value)
    }
}

pub(crate) fn application_version(config: &Value) -> i32 {
    config
        .pointer("/application/version")
        .and_then(Value::as_i64)
        .and_then(|version| i32::try_from(version).ok())
        .filter(|version| *version > 0)
        .unwrap_or(1)
}

pub(crate) fn completion_criteria(config: &Value) -> Vec<String> {
    config
        .pointer("/application/completion_criteria")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|criterion| !criterion.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(crate) fn max_attempts(config: &Value) -> i32 {
    config
        .pointer("/execution/retry/max_attempts")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .map(|value| value.clamp(1, 10))
        .unwrap_or(3)
}

fn runtime_from_config(config: &Value) -> Option<String> {
    config
        .get("runtime")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn retry_prompt(task: &AgentTaskRow) -> String {
    let previous_failure = task.failure_reason.as_deref().unwrap_or("unknown failure");
    let input = serde_json::to_string_pretty(&task.input_json)
        .unwrap_or_else(|_| task.input_json.to_string());
    format!(
        "Retry the same task as a new execution attempt. Correct the previous failure and produce a complete deliverable.\n\nTask: {}\nPrevious failure: {}\nInputs:\n{}",
        task.title, previous_failure, input
    )
}

pub(crate) fn required_input_keys(config: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    for value in config
        .pointer("/application/inputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(key) = value
            .get("type")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
        else {
            continue;
        };
        if !keys.iter().any(|existing| existing == key) {
            keys.push(key.to_owned());
        }
    }
    keys
}

fn validate_task_input(config: &Value, input: &Value) -> Result<(), GatewayError> {
    let Some(input) = input.as_object() else {
        return Err(GatewayError::BadRequest(
            "task input must be a JSON object".to_owned(),
        ));
    };
    let missing = required_input_keys(config)
        .into_iter()
        .filter(|key| !input.get(key).is_some_and(input_value_present))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(GatewayError::BadRequest(format!(
            "missing required task inputs: {}",
            missing.join(", ")
        )))
    }
}

fn input_value_present(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(value) => !value.trim().is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        Value::Bool(_) | Value::Number(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        application_version, completion_criteria, max_attempts, required_input_keys,
        validate_task_input,
    };

    #[test]
    fn application_version_defaults_for_legacy_config() {
        assert_eq!(application_version(&json!({})), 1);
        assert_eq!(
            application_version(&json!({"application": {"version": 2}})),
            2
        );
    }

    #[test]
    fn completion_criteria_ignores_blank_and_non_string_values() {
        assert_eq!(
            completion_criteria(&json!({
                "application": {"completion_criteria": ["Evidence is attached", " ", 3]}
            })),
            vec!["Evidence is attached"]
        );
    }

    #[test]
    fn task_input_validation_requires_each_declared_input_type() {
        let config = json!({
            "application": {
                "inputs": [
                    {"type": "request", "source": "conversation"},
                    {"type": "repository", "source": "user"}
                ]
            }
        });
        assert_eq!(required_input_keys(&config), vec!["request", "repository"]);
        assert!(validate_task_input(
            &config,
            &json!({"request": "Review", "repository": "org/repo"})
        )
        .is_ok());
        assert!(validate_task_input(&config, &json!({"request": "Review"})).is_err());
    }

    #[test]
    fn retry_attempt_limit_defaults_and_clamps() {
        assert_eq!(max_attempts(&json!({})), 3);
        assert_eq!(
            max_attempts(&json!({"execution": {"retry": {"max_attempts": 5}}})),
            5
        );
        assert_eq!(
            max_attempts(&json!({"execution": {"retry": {"max_attempts": 99}}})),
            10
        );
    }
}
