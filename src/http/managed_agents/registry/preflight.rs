//! Agent preflight: deterministic dependency checks run before activation.
//!
//! Each check reports one of four verdicts so the report never overstates
//! what was proven:
//! - `verified`    — the dependency was resolved/connected right now
//! - `exists_only` — the record exists but correctness was not proven
//!                   (e.g. a vault key has a value, but the value may be wrong)
//! - `unverified`  — the check is not implemented for this configuration
//! - `failed`      — the dependency is missing or unusable
//!
//! Activation (`POST /api/agents/{id}/activate`) is blocked while any check
//! reports `failed`; `exists_only`/`unverified` do not block.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Serialize;
use sqlx::PgPool;

use crate::{
    db::{
        managed_agents::registry::{repository, schema::ManagedAgentRow},
        mcp_servers, vault_keys,
    },
    errors::GatewayError,
    http::{agent_runtime_tools::runtime_tools, runtime_resolution::resolve_runtime},
    proxy::{auth::master_key::authenticate, state::AppState},
};

#[derive(Debug, Serialize)]
pub struct PreflightCheck {
    pub id: &'static str,
    pub label: String,
    pub verdict: &'static str,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct PreflightReport {
    pub agent_id: String,
    pub status: String,
    pub can_activate: bool,
    pub checks: Vec<PreflightCheck>,
}

const VERIFIED: &str = "verified";
const EXISTS_ONLY: &str = "exists_only";
const UNVERIFIED: &str = "unverified";
const FAILED: &str = "failed";

pub async fn run_preflight(
    state: &AppState,
    pool: &PgPool,
    agent: &ManagedAgentRow,
) -> Result<PreflightReport, GatewayError> {
    let mut checks = Vec::new();

    checks.push(check_runtime(state, pool, agent).await);
    checks.push(check_model(agent));
    checks.push(check_tools(agent));
    checks.extend(check_vault_keys(pool, agent).await?);
    checks.extend(check_mcp_servers(pool, agent).await?);

    let can_activate = checks.iter().all(|check| check.verdict != FAILED);
    Ok(PreflightReport {
        agent_id: agent.id.clone(),
        status: agent.status.clone(),
        can_activate,
        checks,
    })
}

fn agent_runtime_alias(agent: &ManagedAgentRow) -> Option<String> {
    agent
        .config
        .get("runtime")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|runtime| !runtime.is_empty())
        .map(str::to_owned)
}

async fn check_runtime(state: &AppState, pool: &PgPool, agent: &ManagedAgentRow) -> PreflightCheck {
    let Some(alias) = agent_runtime_alias(agent) else {
        return PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: EXISTS_ONLY,
            detail: "未配置外部 Runtime，将使用内置聊天执行。".to_owned(),
        };
    };
    match resolve_runtime(pool, state, &alias).await {
        Ok(_) => PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: VERIFIED,
            detail: format!("Runtime「{alias}」已注册且凭证已配置。"),
        },
        Err(error) => PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: FAILED,
            detail: format!("Runtime「{alias}」无法解析：{error}"),
        },
    }
}

fn check_model(agent: &ManagedAgentRow) -> PreflightCheck {
    if agent.model.trim().is_empty() {
        return PreflightCheck {
            id: "model",
            label: "模型".to_owned(),
            verdict: FAILED,
            detail: "未配置模型。".to_owned(),
        };
    }
    PreflightCheck {
        id: "model",
        label: "模型".to_owned(),
        verdict: EXISTS_ONLY,
        detail: format!(
            "模型「{}」已配置；是否在所选 Runtime 中可用未验证。",
            agent.model
        ),
    }
}

fn check_tools(agent: &ManagedAgentRow) -> PreflightCheck {
    let tool_ids: Vec<String> = agent
        .tools
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool.get("type").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect();
    if tool_ids.is_empty() {
        return PreflightCheck {
            id: "tools",
            label: "工具".to_owned(),
            verdict: VERIFIED,
            detail: "未启用原生工具。".to_owned(),
        };
    }
    // The static catalog is keyed by api_spec (e.g. claude_managed_agents);
    // custom harness aliases resolve through the same specs, so an empty
    // catalog here just means we cannot verify, not that tools are wrong.
    let catalog = agent_runtime_alias(agent)
        .map(|alias| runtime_tools(&alias))
        .unwrap_or(&[]);
    if catalog.is_empty() {
        return PreflightCheck {
            id: "tools",
            label: "工具".to_owned(),
            verdict: UNVERIFIED,
            detail: format!(
                "已启用 {} 个工具；当前 Runtime 无静态工具目录，兼容性未验证。",
                tool_ids.len()
            ),
        };
    }
    let unsupported: Vec<&String> = tool_ids
        .iter()
        .filter(|id| !catalog.iter().any(|tool| tool.id == id.as_str()))
        .collect();
    if unsupported.is_empty() {
        PreflightCheck {
            id: "tools",
            label: "工具".to_owned(),
            verdict: VERIFIED,
            detail: format!("{} 个工具均受当前 Runtime 支持。", tool_ids.len()),
        }
    } else {
        PreflightCheck {
            id: "tools",
            label: "工具".to_owned(),
            verdict: FAILED,
            detail: format!(
                "以下工具不受当前 Runtime 支持：{}",
                unsupported
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

async fn check_vault_keys(
    pool: &PgPool,
    agent: &ManagedAgentRow,
) -> Result<Vec<PreflightCheck>, GatewayError> {
    let key_names: Vec<String> = agent
        .vault_keys
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect();
    let mut checks = Vec::new();
    for name in key_names {
        let owner = agent.owner_id.as_deref().unwrap_or("");
        let present = vault_keys::resolve_vault_key(pool, &name, owner)
            .await?
            .is_some();
        checks.push(if present {
            PreflightCheck {
                id: "vault_key",
                label: format!("凭证 {name}"),
                verdict: EXISTS_ONLY,
                detail: "保险库中存在值；值是否有效未验证。".to_owned(),
            }
        } else {
            PreflightCheck {
                id: "vault_key",
                label: format!("凭证 {name}"),
                verdict: FAILED,
                detail: "已声明但保险库中没有对应的密钥值。".to_owned(),
            }
        });
    }
    Ok(checks)
}

async fn check_mcp_servers(
    pool: &PgPool,
    agent: &ManagedAgentRow,
) -> Result<Vec<PreflightCheck>, GatewayError> {
    let server_ids: Vec<String> = agent
        .config
        .get("mcp_server_ids")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect();
    let mut checks = Vec::new();
    for server_id in server_ids {
        let row = mcp_servers::repository::get(pool, &server_id).await?;
        checks.push(match row {
            Some(row) => PreflightCheck {
                id: "mcp_server",
                label: format!(
                    "MCP {}",
                    row.server_name
                        .or(row.alias)
                        .unwrap_or_else(|| row.server_id.clone())
                ),
                verdict: EXISTS_ONLY,
                detail: "已在注册表中；连通性与凭证有效性未验证。".to_owned(),
            },
            None => PreflightCheck {
                id: "mcp_server",
                label: format!("MCP {server_id}"),
                verdict: FAILED,
                detail: "配置引用了不存在的 MCP 服务器。".to_owned(),
            },
        });
    }
    Ok(checks)
}

pub async fn preflight(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<PreflightReport>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?;
    let agent = repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::super::assert_agent_use(&auth, &agent, pool).await?;
    Ok(Json(run_preflight(&state, pool, &agent).await?))
}

pub async fn activate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?;
    let agent = repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::super::assert_agent_edit(&auth, &agent, pool).await?;
    let report = run_preflight(&state, pool, &agent).await?;
    if !report.can_activate {
        return Err(GatewayError::BadRequest(format!(
            "预检未通过，无法激活：{}",
            report
                .checks
                .iter()
                .filter(|check| check.verdict == FAILED)
                .map(|check| format!("{}（{}）", check.label, check.detail))
                .collect::<Vec<_>>()
                .join("；")
        )));
    }
    repository::set_status(pool, &agent_id, "active")
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    Ok(Json(serde_json::json!({
        "id": agent_id,
        "status": "active",
        "preflight": report,
    })))
}
