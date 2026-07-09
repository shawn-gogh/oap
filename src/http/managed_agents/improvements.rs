//! Improvement proposals: the write half of the self-evolution loop.
//!
//! Flow: latest failed eval run → LLM drafts a single-variable improvement
//! (a revised system prompt, nothing else changes) → the proposal lands in
//! the inbox as a pending approval → a human accepts → the change is applied
//! as a new agent revision and an automatic regression eval run starts.
//! Human-in-the-loop and single-variable by construction, per the
//! methodology's red lines.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::{
    db::managed_agents::{
        eval_runs, inbox,
        registry::{self, schema::UpdateManagedAgent},
    },
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

use super::eval_runs::{complete_text, start_eval_run};

const PROPOSER_SYSTEM: &str = "You improve AI agent system prompts using single-variable \
iteration: change ONLY the system prompt, keep the agent's purpose and constraints intact. \
You are given the current system prompt, the success criteria, and the eval cases that \
failed with the agent's wrong answers. Produce a revised system prompt that fixes the \
failures without regressing the passing behaviors, and keep it as close to the original \
as possible. Return ONLY a JSON object, no markdown fence: \
{\"new_system\": \"<full revised system prompt>\", \"rationale\": \"<one short paragraph: what changed and why it fixes the failures>\"}";

pub async fn create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<(StatusCode, Json<Value>), GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::db(&state, &headers).await?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::assert_agent_access(&auth, &agent)?;
    let item = propose(&state, pool, &agent).await?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(item).unwrap_or_default())))
}

/// Drafts a single-variable improvement from the latest failed eval run and
/// files it as a pending inbox approval. Shared by the HTTP handler and the
/// evolution sweeper.
pub(crate) async fn propose(
    state: &AppState,
    pool: &PgPool,
    agent: &crate::db::managed_agents::registry::schema::ManagedAgentRow,
) -> Result<crate::db::managed_agents::inbox::schema::InboxItemRow, GatewayError> {
    let agent_id = agent.id.clone();
    // Learn from the most recent completed run that has failures.
    let runs = eval_runs::repository::list(pool, &agent_id, 10).await?;
    let source_run = runs
        .iter()
        .find(|run| run.status == "completed" && run.passed < run.total)
        .ok_or_else(|| {
            GatewayError::InvalidConfig(
                "no completed eval run with failures — run an evaluation first".to_owned(),
            )
        })?;
    let failures: Vec<&Value> = source_run
        .results
        .as_array()
        .map(|items| items.iter().filter(|r| r["pass"] != json!(true)).collect())
        .unwrap_or_default();
    let success_criteria = agent
        .config
        .pointer("/design/evaluation/success_criteria")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let failures_text = failures
        .iter()
        .map(|f| {
            format!(
                "- [{}] input: {}\n  agent answer: {}\n  judge: {}",
                f["category"].as_str().unwrap_or(""),
                f["input"].as_str().unwrap_or(""),
                f["answer"].as_str().unwrap_or(""),
                f["verdict"].as_str().unwrap_or(""),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!(
        "Current system prompt:\n{}\n\nSuccess criteria:\n{success_criteria}\n\nFailed cases ({} of {}):\n{failures_text}",
        agent.system, source_run.total - source_run.passed, source_run.total
    );

    let raw = complete_text(state, &agent.model, PROPOSER_SYSTEM, &user, 2000).await?;
    let cleaned = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let proposal: Value = serde_json::from_str(cleaned).map_err(|_| {
        GatewayError::SandboxError(format!(
            "proposer returned non-JSON output: {}",
            cleaned.chars().take(200).collect::<String>()
        ))
    })?;
    let new_system = proposal
        .get("new_system")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| GatewayError::SandboxError("proposal missing new_system".to_owned()))?;
    let rationale = proposal
        .get("rationale")
        .and_then(Value::as_str)
        .unwrap_or("");

    let base_version = registry::revisions::list(pool, &agent_id, 1)
        .await
        .ok()
        .and_then(|rows| rows.first().map(|row| row.version));

    let item = inbox::repository::create_approval(
        pool,
        format!("改进提案：{}", agent.name),
        None,
        Some(agent_id.clone()),
        Some(format!(
            "基于评估 {}（{}/{} 通过）的单变量改进（仅改 system prompt）。\n\n理由：{rationale}\n\n新 system prompt：\n{new_system}",
            source_run.id, source_run.passed, source_run.total
        )),
        Some(json!({
            "type": "agent_improvement",
            "agent_id": agent_id,
            "new_system": new_system,
            "changed_variable": "system_prompt",
            "base_version": base_version,
            "eval_run_id": source_run.id,
        })),
    )
    .await?;

    Ok(item)
}

/// Called after an approval is accepted: if it is an improvement proposal,
/// apply the change as a new revision and kick off a regression eval.
pub(crate) async fn apply_if_improvement(state: Arc<AppState>, pool: PgPool, item_id: &str) {
    let Ok(Some(item)) = inbox::repository::get(&pool, item_id).await else {
        return;
    };
    if item.status != "accepted" {
        return;
    }
    let Some(args) = item
        .args_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
    else {
        return;
    };
    if args.get("type").and_then(Value::as_str) != Some("agent_improvement") {
        return;
    }
    let (Some(agent_id), Some(new_system)) = (
        args.get("agent_id").and_then(Value::as_str),
        args.get("new_system").and_then(Value::as_str),
    ) else {
        return;
    };

    let update = UpdateManagedAgent {
        system: Some(new_system.to_owned()),
        prompt: Some(new_system.to_owned()),
        ..Default::default()
    };
    match registry::repository::update(&pool, agent_id, update).await {
        Ok(Some(row)) => {
            let _ = registry::revisions::record(&pool, &row, Some("improvement-loop")).await;
            // Regression eval against the same case set; failures show up in
            // the eval history next to the version that introduced them.
            if let Err(error) = start_eval_run(state, &pool, row, "improvement-loop").await {
                tracing::warn!(agent_id, %error, "regression eval failed to start");
            }
        }
        Ok(None) => tracing::warn!(agent_id, "improvement accepted but agent no longer exists"),
        Err(error) => tracing::warn!(agent_id, %error, "failed to apply accepted improvement"),
    }
}
