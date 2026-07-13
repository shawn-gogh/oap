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
    proxy::{auth::master_key::authenticate, credential_crypto, state::AppState},
};

/// Upper bound for each outbound connectivity probe so one hung server
/// cannot stall the whole preflight report.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

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
    state: &Arc<AppState>,
    pool: &PgPool,
    agent: &ManagedAgentRow,
) -> Result<PreflightReport, GatewayError> {
    let mut checks = Vec::new();

    checks.push(check_runtime(state, pool, agent).await);
    checks.push(check_model(state, agent));
    checks.push(check_tools(agent));
    checks.extend(check_vault_keys(pool, agent).await?);
    checks.extend(check_mcp_servers(state, pool, agent).await?);

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

async fn check_runtime(
    state: &Arc<AppState>,
    pool: &PgPool,
    agent: &ManagedAgentRow,
) -> PreflightCheck {
    let Some(alias) = agent_runtime_alias(agent) else {
        return PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: EXISTS_ONLY,
            detail: "未配置外部 Runtime，将使用内置聊天执行。".to_owned(),
        };
    };
    let resolved = match resolve_runtime(pool, state, &alias).await {
        Ok(resolved) => resolved,
        Err(error) => {
            return PreflightCheck {
                id: "runtime",
                label: "Runtime".to_owned(),
                verdict: FAILED,
                detail: format!("Runtime「{alias}」无法解析：{error}"),
            }
        }
    };
    // Built-in SaaS runtimes (Anthropic etc.) are not probed; a reachability
    // check against a vendor API proves little and adds flakiness. Custom
    // harnesses run on user infrastructure, where "registered but down" is
    // the common failure worth catching before activation.
    if !resolved.is_custom_harness {
        return PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: VERIFIED,
            detail: format!("Runtime「{alias}」已注册且凭证已配置。"),
        };
    }
    let base = resolved
        .credential
        .api_base
        .trim_end_matches('/')
        .to_owned();
    match state.http.get(&base).timeout(PROBE_TIMEOUT).send().await {
        // Any HTTP response (even 404) proves the harness endpoint is up;
        // route-level correctness is the session layer's job.
        Ok(_) => PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: VERIFIED,
            detail: format!("Runtime「{alias}」已连通（{base}）。"),
        },
        Err(error) if error.is_timeout() => PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: FAILED,
            detail: format!("Runtime「{alias}」连接超时（{base}）。"),
        },
        Err(error) => PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: FAILED,
            detail: format!("Runtime「{alias}」无法连接（{base}）：{error}"),
        },
    }
}

fn check_model(state: &Arc<AppState>, agent: &ManagedAgentRow) -> PreflightCheck {
    let model = agent.model.trim();
    if model.is_empty() {
        return PreflightCheck {
            id: "model",
            label: "模型".to_owned(),
            verdict: FAILED,
            detail: "未配置模型。".to_owned(),
        };
    }
    // Custom-harness agents route inference back through this gateway, so the
    // gateway's own model_list is authoritative for them. External runtimes
    // (Anthropic managed agents, Gemini) resolve models vendor-side, which we
    // cannot verify here — say so instead of guessing.
    if state
        .config
        .model_list
        .iter()
        .any(|entry| entry.model_name == model)
    {
        return PreflightCheck {
            id: "model",
            label: "模型".to_owned(),
            verdict: VERIFIED,
            detail: format!("模型「{model}」在网关模型列表中。"),
        };
    }
    PreflightCheck {
        id: "model",
        label: "模型".to_owned(),
        verdict: UNVERIFIED,
        detail: format!(
            "模型「{model}」不在网关模型列表中；若由外部 Runtime 直接解析则可能可用，此处无法验证。"
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
    state: &Arc<AppState>,
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
        let Some(row) = mcp_servers::repository::get(pool, &server_id).await? else {
            checks.push(PreflightCheck {
                id: "mcp_server",
                label: format!("MCP {server_id}"),
                verdict: FAILED,
                detail: "配置引用了不存在的 MCP 服务器。".to_owned(),
            });
            continue;
        };
        let label = format!(
            "MCP {}",
            row.server_name
                .clone()
                .or(row.alias.clone())
                .unwrap_or_else(|| row.server_id.clone())
        );
        checks.push(smoke_test_mcp(state, pool, agent, &row, &server_id, label).await);
    }
    Ok(checks)
}

/// Live tools/list smoke test against one attached MCP server, on behalf of
/// the agent owner (matching the identity used at run time). A successful
/// tools/list upgrades the verdict to `verified`; failures are classified so
/// the user knows whether to fix the network, the credential, or the server.
async fn smoke_test_mcp(
    state: &Arc<AppState>,
    pool: &PgPool,
    agent: &ManagedAgentRow,
    row: &crate::db::mcp_servers::schema::McpServerRow,
    server_id: &str,
    label: String,
) -> PreflightCheck {
    let owner = agent.owner_id.clone().unwrap_or_default();
    let enc_key =
        credential_crypto::encryption_key(state.config.general_settings.master_key.as_deref()).ok();
    let smoke = tokio::time::timeout(
        PROBE_TIMEOUT,
        crate::http::mcp_registry::tools::tools_for_server(
            state,
            pool,
            row,
            server_id,
            &owner,
            enc_key.as_deref(),
        ),
    )
    .await;
    match smoke {
        Ok(Ok(tools)) => PreflightCheck {
            id: "mcp_server",
            label,
            verdict: VERIFIED,
            detail: format!("tools/list 连通成功，{} 个工具可用。", tools.len()),
        },
        Ok(Err(GatewayError::UpstreamHttp(status @ (401 | 403), _))) => PreflightCheck {
            id: "mcp_server",
            label,
            verdict: FAILED,
            detail: format!("认证失败（HTTP {status}）：请检查该服务器的凭证配置。"),
        },
        Ok(Err(GatewayError::UpstreamHttp(status, _))) => PreflightCheck {
            id: "mcp_server",
            label,
            verdict: FAILED,
            detail: format!("服务器返回 HTTP {status}：协议或服务端错误。"),
        },
        Ok(Err(GatewayError::Upstream(error))) => PreflightCheck {
            id: "mcp_server",
            label,
            verdict: FAILED,
            detail: format!("网络不可达：{error}"),
        },
        Ok(Err(error)) => PreflightCheck {
            id: "mcp_server",
            label,
            verdict: FAILED,
            detail: format!("冒烟测试失败：{error}"),
        },
        Err(_) => PreflightCheck {
            id: "mcp_server",
            label,
            verdict: FAILED,
            detail: format!("连接超时（>{}s）。", PROBE_TIMEOUT.as_secs()),
        },
    }
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
