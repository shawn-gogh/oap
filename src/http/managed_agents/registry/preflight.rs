//! Agent preflight: deterministic dependency checks run before activation.
//!
//! Each check reports one of four verdicts so the report never overstates
//! what was proven:
//! - `verified`    — the dependency was resolved/connected right now
//! - `exists_only` — the record exists but correctness was not proven
//!   (e.g. a vault key has a value, but the value may be wrong)
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
    http::{agent_runtime_tools::runtime_tools, runtime_resolution::resolve_runtime_for_agent},
    proxy::{auth::master_key::authenticate, credential_crypto, state::AppState},
    sdk::agents::{
        canonical::{normalize_agent, NormalizationSeverity},
        conformance::{inspect_runtime_contract, ConformanceReport},
    },
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
    run_preflight_with_smoke(state, pool, agent, None).await
}

/// Like `run_preflight`, but when `smoke_user` is set, additionally exercises
/// the real execution path (an A2A `message/send` round trip) for federated
/// agents. Only the explicit governance "运行检查" passes a user: scheduled
/// health checks must never send messages to remote agents on their own.
pub async fn run_preflight_with_smoke(
    state: &Arc<AppState>,
    pool: &PgPool,
    agent: &ManagedAgentRow,
    smoke_user: Option<&str>,
) -> Result<PreflightReport, GatewayError> {
    let mut checks = Vec::new();

    checks.push(check_runtime(state, pool, agent).await);
    checks.push(check_model(state, agent));
    checks.push(check_tools(agent));
    checks.extend(check_vault_keys(pool, agent).await?);
    checks.extend(check_mcp_servers(state, pool, agent).await?);
    checks.extend(check_source_contract(pool, agent).await?);
    checks.extend(check_source_credential(pool, agent).await?);
    if let Some(user_id) = smoke_user {
        if let Some(check) = check_execution_smoke(state, pool, agent, user_id).await {
            checks.push(check);
        }
    }

    let can_activate = checks.iter().all(|check| check.verdict != FAILED);
    Ok(PreflightReport {
        agent_id: agent.id.clone(),
        status: agent.status.clone(),
        can_activate,
        checks,
    })
}

async fn check_source_contract(
    pool: &PgPool,
    agent: &ManagedAgentRow,
) -> Result<Vec<PreflightCheck>, GatewayError> {
    if !crate::db::managed_agents::governance::requires_governance(agent) {
        return Ok(Vec::new());
    }
    let mut checks = Vec::new();
    let normalization = normalize_agent(agent);
    let blocking = normalization
        .issues
        .iter()
        .filter(|issue| issue.severity == NormalizationSeverity::Blocking)
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>();
    checks.push(if blocking.is_empty() {
        PreflightCheck {
            id: "canonical_spec",
            label: "统一规范".to_owned(),
            verdict: if normalization.requires_approval {
                EXISTS_ONLY
            } else {
                VERIFIED
            },
            detail: if normalization.requires_approval {
                "规范化完成，但包含发布时必须人工确认的高风险来源字段。".to_owned()
            } else {
                "来源已成功归一化为平台统一规范。".to_owned()
            },
        }
    } else {
        PreflightCheck {
            id: "canonical_spec",
            label: "统一规范".to_owned(),
            verdict: FAILED,
            detail: blocking.join("；"),
        }
    });
    let conformance = inspect_runtime_contract(agent);
    let federated = crate::db::managed_agents::governance::external_source_kind(agent)
        == Some("external_agent");
    checks.push(PreflightCheck {
        id: "runtime_contract",
        label: "运行时契约".to_owned(),
        verdict: if conformance.status == "conformant" {
            VERIFIED
        } else {
            FAILED
        },
        detail: runtime_contract_detail(&conformance, federated),
    });
    checks.push(
        match crate::db::managed_agents::sources::repository::get_source_by_agent(pool, &agent.id)
            .await?
        {
            Some(source)
                if source.sync_state == "in_sync" && source.candidate_snapshot_id.is_none() =>
            {
                PreflightCheck {
                    id: "source_sync",
                    label: "来源同步".to_owned(),
                    verdict: VERIFIED,
                    detail: "来源处于同步状态且没有待处理漂移。".to_owned(),
                }
            }
            Some(source) => {
                let reason = if source.sync_state == "sync_error" {
                    crate::db::managed_agents::sources::repository::latest_sync_run(
                        pool, &source.id,
                    )
                    .await?
                    .and_then(|run| run.error_detail)
                } else {
                    None
                };
                PreflightCheck {
                    id: "source_sync",
                    label: "来源同步".to_owned(),
                    verdict: FAILED,
                    detail: match reason {
                        Some(reason) => format!(
                            "来源状态为 {}：{}。请先处理该错误，或到「来源、漂移与运行保障」区点击「立即同步来源」重试。",
                            source.sync_state, reason
                        ),
                        None => format!(
                            "来源状态为 {}，必须先处理同步错误、缺失或候选漂移。",
                            source.sync_state
                        ),
                    },
                }
            }
            None => PreflightCheck {
                id: "source_sync",
                label: "来源同步".to_owned(),
                verdict: FAILED,
                detail: "缺少统一来源记录。".to_owned(),
            },
        },
    );
    Ok(checks)
}

/// A federated source is only non-conformant when its protocol has no execution
/// bridge in `sessions::external_bridge` (bridged protocols are conformant). Say
/// that plainly instead of surfacing a cryptic contract status, so operators
/// know the agent is catalog-only rather than misconfigured.
fn runtime_contract_detail(conformance: &ConformanceReport, federated: bool) -> String {
    if conformance.status != "conformant" && federated {
        return format!(
            "该来源协议暂不支持平台托管执行，仅可编目发现，不能通过治理测试或发布运行（契约状态：{}）。",
            conformance.status
        );
    }
    if conformance.status == "conformant" {
        return format!(
            "契约 {} 检查结果：{}。",
            conformance.contract_version, conformance.status
        );
    }
    // "partial"/"non_conformant" alone tells the operator nothing about what
    // to fix — name the specific required sub-checks that failed, matching
    // the ids/labels shown in the governance panel's own breakdown table.
    let failing: Vec<&str> = conformance
        .checks
        .iter()
        .filter(|check| check.required && !check.passed)
        .map(|check| check.detail.as_str())
        .collect();
    if failing.is_empty() {
        format!(
            "契约 {} 检查结果：{}。",
            conformance.contract_version, conformance.status
        )
    } else {
        format!(
            "契约 {} 检查结果：{}。未满足的必需项：{}",
            conformance.contract_version,
            conformance.status,
            failing.join("；")
        )
    }
}

/// Verifies the imported agent's execution credential is resolvable *as
/// sessions will resolve it* (per credential_mode), instead of assuming the
/// discovery probe's credential story carries over to execution.
async fn check_source_credential(
    pool: &PgPool,
    agent: &ManagedAgentRow,
) -> Result<Vec<PreflightCheck>, GatewayError> {
    let Some(source) = agent.config.get("source") else {
        return Ok(Vec::new());
    };
    if source.get("kind").and_then(serde_json::Value::as_str) != Some("external_agent") {
        return Ok(Vec::new());
    }
    let credential_mode = source
        .get("credential_mode")
        .and_then(serde_json::Value::as_str);
    let check = match credential_mode {
        Some("shared") => {
            let credential_name = source
                .get("credential_name")
                .and_then(serde_json::Value::as_str);
            let owner_id = agent.owner_id.as_deref().unwrap_or_default();
            let resolved = match credential_name {
                Some(name) if !owner_id.is_empty() => {
                    crate::db::credentials::get_personal_by_name(pool, name, owner_id)
                        .await?
                        .is_some()
                }
                _ => false,
            };
            PreflightCheck {
                id: "source_credential",
                label: "执行凭据".to_owned(),
                verdict: if resolved { VERIFIED } else { FAILED },
                detail: if resolved {
                    "共享凭据存在且属主匹配，会话可以解析。".to_owned()
                } else {
                    "共享凭据缺失或属主不匹配，会话执行将失败。".to_owned()
                },
            }
        }
        Some("byo") => {
            let configured_users = sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*) FROM "LiteLLM_CredentialsTable"
                WHERE credential_name = $1 AND scope = 'personal'
                "#,
            )
            .bind(crate::http::runtime_resolution::byo_credential_name(
                &agent.id,
            ))
            .fetch_one(pool)
            .await
            .map_err(GatewayError::Database)?;
            PreflightCheck {
                id: "source_credential",
                label: "执行凭据".to_owned(),
                verdict: EXISTS_ONLY,
                detail: format!(
                    "BYO 模式：每个使用者需自行配置密钥，当前已有 {configured_users} 个用户配置。未配置的用户会话将失败。"
                ),
            }
        }
        other => PreflightCheck {
            id: "source_credential",
            label: "执行凭据".to_owned(),
            verdict: FAILED,
            detail: format!("凭据模式无效：{}。", other.unwrap_or("<missing>")),
        },
    };
    Ok(vec![check])
}

/// Real execution smoke: one A2A `message/send` round trip using the same
/// URL/credential resolution as `sessions::external_bridge`. Discovery-only
/// probes green-light agents whose sessions then fail 100% of the time (e.g.
/// unresolvable execution credential) — this closes that gap for the
/// explicit governance test. Returns None for specs without a safe smoke.
async fn check_execution_smoke(
    state: &Arc<AppState>,
    pool: &PgPool,
    agent: &ManagedAgentRow,
    user_id: &str,
) -> Option<PreflightCheck> {
    let source = agent.config.get("source")?;
    if source.get("kind").and_then(serde_json::Value::as_str) != Some("external_agent") {
        return None;
    }
    if source.get("api_spec").and_then(serde_json::Value::as_str) != Some("a2a_v1") {
        return None;
    }
    let endpoint = source
        .get("endpoint")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let rpc_url = source
        .pointer("/raw/url")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| endpoint.to_owned());
    let credential_mode = source
        .get("credential_mode")
        .and_then(serde_json::Value::as_str);
    let (credential_name, credential_owner) = match credential_mode {
        Some("shared") => (
            source
                .get("credential_name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            agent.owner_id.clone().unwrap_or_default(),
        ),
        Some("byo") => (
            Some(crate::http::runtime_resolution::byo_credential_name(
                &agent.id,
            )),
            user_id.to_owned(),
        ),
        _ => return None, // credential check already reports the failure
    };
    let api_key = match crate::http::managed_agents::source_management::credential_api_key(
        state,
        pool,
        credential_name.as_deref(),
        &credential_owner,
    )
    .await
    {
        Ok(key) => key,
        Err(_) if credential_mode == Some("byo") => {
            return Some(PreflightCheck {
                id: "execution_smoke",
                label: "执行冒烟".to_owned(),
                verdict: EXISTS_ONLY,
                detail: "当前用户未配置该智能体的 BYO 密钥，跳过执行冒烟。".to_owned(),
            });
        }
        Err(error) => {
            return Some(PreflightCheck {
                id: "execution_smoke",
                label: "执行冒烟".to_owned(),
                verdict: FAILED,
                detail: format!("执行凭据解析失败：{error}"),
            });
        }
    };
    let rpc_url =
        match crate::http::managed_agents::source_management::validate_connector_endpoint(&rpc_url)
            .await
        {
            Ok(url) => url,
            Err(error) => {
                return Some(PreflightCheck {
                    id: "execution_smoke",
                    label: "执行冒烟".to_owned(),
                    verdict: FAILED,
                    detail: format!("执行端点校验失败：{error}"),
                });
            }
        };
    let started = std::time::Instant::now();
    let response = tokio::time::timeout(
        PROBE_TIMEOUT,
        state
            .http
            .post(&rpc_url)
            .bearer_auth(&api_key)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": crate::db::managed_agents::id("rpc"),
                "method": "message/send",
                "params": {
                    "message": {
                        "kind": "message",
                        "role": "user",
                        "messageId": crate::db::managed_agents::id("msg"),
                        "parts": [{"kind": "text", "text": "ping：这是平台运行检查的执行冒烟，请直接简短回复。"}]
                    }
                }
            }))
            .send(),
    )
    .await;
    let latency = started.elapsed().as_millis();
    let payload = match response {
        Ok(Ok(response)) if response.status().is_success() => {
            match response.json::<serde_json::Value>().await {
                Ok(payload) => payload,
                Err(error) => {
                    return Some(PreflightCheck {
                        id: "execution_smoke",
                        label: "执行冒烟".to_owned(),
                        verdict: FAILED,
                        detail: format!("message/send 返回了无效 JSON：{error}"),
                    });
                }
            }
        }
        Ok(Ok(response)) => {
            return Some(PreflightCheck {
                id: "execution_smoke",
                label: "执行冒烟".to_owned(),
                verdict: FAILED,
                detail: format!("message/send 返回 HTTP {}。", response.status().as_u16()),
            });
        }
        Ok(Err(error)) => {
            return Some(PreflightCheck {
                id: "execution_smoke",
                label: "执行冒烟".to_owned(),
                verdict: FAILED,
                detail: format!("message/send 请求失败：{error}"),
            });
        }
        Err(_) => {
            return Some(PreflightCheck {
                id: "execution_smoke",
                label: "执行冒烟".to_owned(),
                verdict: FAILED,
                detail: "message/send 冒烟超时。".to_owned(),
            });
        }
    };
    if payload.get("error").is_some() {
        return Some(PreflightCheck {
            id: "execution_smoke",
            label: "执行冒烟".to_owned(),
            verdict: FAILED,
            detail: format!(
                "message/send 返回 JSON-RPC 错误：{}",
                payload
                    .pointer("/error/message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown")
            ),
        });
    }
    // Either an immediate text reply or an accepted async task proves the
    // execution path (URL + auth + protocol). If a task was created, cancel
    // it best-effort so the smoke doesn't leave remote work running.
    let result = payload.get("result").cloned().unwrap_or_default();
    let task_id = result
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    if let Some(task_id) = task_id.as_deref() {
        let _ = state
            .http
            .post(&rpc_url)
            .bearer_auth(&api_key)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": crate::db::managed_agents::id("rpc"),
                "method": "tasks/cancel",
                "params": {"id": task_id}
            }))
            .send()
            .await;
    }
    Some(PreflightCheck {
        id: "execution_smoke",
        label: "执行冒烟".to_owned(),
        verdict: VERIFIED,
        detail: format!(
            "message/send 执行链路验证通过（{}，{latency}ms）。",
            if task_id.is_some() {
                "远端以异步任务受理，已即时取消"
            } else {
                "远端同步回复"
            }
        ),
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
    if let Some(check) = check_federated_source(state, pool, agent).await {
        return check;
    }
    let Some(alias) = agent_runtime_alias(agent) else {
        return PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: EXISTS_ONLY,
            detail: "未配置外部 Runtime，将使用内置聊天执行。".to_owned(),
        };
    };
    let resolved = match resolve_runtime_for_agent(pool, state, &alias, agent).await {
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

/// Federated sources (A2A/ACP/Dify/OpenAPI today — any provider whose
/// `expose_runtime_harness()` is false) execute through
/// `sessions::external_bridge`'s direct `source.raw.url` bridge, never
/// through a registered runtime harness. `resolve_runtime_for_agent` doesn't
/// know their api_spec (e.g. `a2a_v1`) and always fails to resolve it, so the
/// harness-oriented check below would never apply to them. Probe reachability
/// instead, the same way a source connector's "test connection" does:
/// `provider.discover()` against the source endpoint. This proves the
/// endpoint is up and speaks the expected discovery protocol — it does not
/// prove `message/send` (the actual execution call) works.
async fn check_federated_source(
    state: &Arc<AppState>,
    pool: &PgPool,
    agent: &ManagedAgentRow,
) -> Option<PreflightCheck> {
    if crate::db::managed_agents::governance::external_source_kind(agent) != Some("external_agent")
    {
        return None;
    }
    let source = agent.config.get("source")?;
    let provider_id = source.get("provider").and_then(serde_json::Value::as_str)?;
    let provider = crate::http::managed_agents::import::provider_for_id(provider_id).ok()?;
    if provider.expose_runtime_harness() {
        return None;
    }
    let endpoint = source
        .get("endpoint")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if endpoint.is_empty() {
        return Some(PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: FAILED,
            detail: "来源缺少可执行的端点。".to_owned(),
        });
    }
    if let Err(error) =
        crate::http::managed_agents::source_management::validate_connector_endpoint(endpoint).await
    {
        return Some(PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: FAILED,
            detail: format!("来源端点校验失败：{error}"),
        });
    }
    // Resolve the discovery credential the same way sync does (connector
    // credential first, then the source's own reference) — otherwise a
    // BYO-mode agent behind an authenticated source probes unauthenticated
    // here, fails, and can never activate while sync reports it reachable.
    let api_key = crate::http::managed_agents::source_management::discovery_api_key_for_agent(
        state, pool, agent,
    )
    .await
    .unwrap_or_default();
    let started = std::time::Instant::now();
    let probe = tokio::time::timeout(
        PROBE_TIMEOUT,
        provider.discover(&state.http, endpoint, &api_key),
    )
    .await;
    Some(match probe {
        Ok(Ok(agents)) => PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: VERIFIED,
            detail: format!(
                "联邦来源「{}」已连通（{endpoint}），发现 {} 个智能体，耗时 {}ms。",
                provider.name(),
                agents.len(),
                started.elapsed().as_millis()
            ),
        },
        Ok(Err(error)) => PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: FAILED,
            detail: format!(
                "联邦来源「{}」不可达（{endpoint}）：{}",
                provider.name(),
                crate::http::managed_agents::import_types::provider_error(error)
            ),
        },
        Err(_) => PreflightCheck {
            id: "runtime",
            label: "Runtime".to_owned(),
            verdict: FAILED,
            detail: format!("联邦来源「{}」连接超时（{endpoint}）。", provider.name()),
        },
    })
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
    let governance = crate::db::managed_agents::governance::get(pool, &agent_id).await?;
    if crate::db::managed_agents::governance::requires_governance(&agent) && governance.is_none() {
        return Err(GatewayError::BadRequest(
            "外部智能体缺少纳管记录，必须重新导入或完成治理迁移后才能激活。".to_owned(),
        ));
    }
    if let Some(governance) = governance {
        if !matches!(
            governance.lifecycle_status.as_str(),
            "published" | "rolled_back"
        ) {
            return Err(GatewayError::BadRequest(
                "外部智能体必须通过纳管测试和发布审批后才能激活。".to_owned(),
            ));
        }
    }
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
