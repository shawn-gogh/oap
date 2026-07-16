//! Bridges opencode's native permission gate (`Permission.Service.ask`) into
//! LAP's inbox, so a human decision genuinely blocks tool execution instead
//! of merely being suggested to the model.
//!
//! Flow: the opencode wrapper (templates/opencode/src/app.mjs) sees a
//! `permission.asked` event on its /event pump and POSTs here to file a
//! pending inbox item (kind = "tool_permission"). A human accepts/rejects it
//! through the normal `/api/approvals/{id}/accept|reject` endpoints
//! (inbox/approvals.rs), which — for this kind — call `reply` below to POST
//! opencode's own `/permission/{requestID}/reply`, unblocking (or permanently
//! denying) the paused tool call. If nobody ever answers, the tool call stays
//! blocked: fail-closed by construction, not by anything we implement.

use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{inbox, registry, sessions},
    errors::GatewayError,
    http::{managed_agents::inbox::types::ApprovalScope, runtime_resolution::resolve_runtime},
    proxy::{auth::master_key::authenticate, state::AppState},
};

#[derive(Debug, Deserialize)]
pub struct PermissionAskedRequest {
    /// The *provider-side* (opencode) session id — the wrapper only knows
    /// its own id, not LAP's. Resolved to LAP's session below.
    pub session_id: String,
    pub request_id: String,
    pub permission: String,
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub metadata: Value,
}

/// POST /api/tool-approvals — called by the opencode wrapper when opencode
/// pauses a tool call awaiting permission. Authenticated the same way as
/// other internal agent endpoints (gateway/master key), since the wrapper
/// calls back through `LITELLM_API_KEY`, not a user session.
pub async fn asked(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<PermissionAskedRequest>,
) -> Result<Json<Value>, GatewayError> {
    authenticate(&headers, &state).await?;
    let pool = super::db(&state, &headers).await?;

    let session = sessions::repository::get_by_provider_session_id(pool, &input.session_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("session {}", input.session_id)))?;
    let agent_name = match session.agent_id.as_deref() {
        Some(agent_id) => registry::repository::get(pool, agent_id)
            .await?
            .map(|agent| agent.name),
        None => None,
    };

    let (Some(runtime), Some(provider_session_id)) = (
        session.runtime.as_deref(),
        session.provider_session_id.as_deref(),
    ) else {
        return Err(GatewayError::NotFound(format!(
            "session missing runtime/provider_session_id"
        )));
    };

    let title = format!("工具权限请求：{}", input.permission);
    let body = if input.patterns.is_empty() {
        None
    } else {
        Some(input.patterns.join(", "))
    };
    let args = serde_json::json!({
        "request_id": input.request_id,
        "permission": input.permission,
        "patterns": input.patterns,
        "metadata": input.metadata,
    });

    // Session-level approval mode (composer selector):
    //   "ask"  — every non-whitelisted op needs a human (default)
    //   "auto" — auto-approve everything except non-whitelisted egress
    //   "full" — auto-approve everything, egress included
    let approval_mode = approval_mode(&session.environment_json);

    // Check if this is an outbound HTTP/network request (Data Egress)
    let outbound_domain = is_outbound_request(&input.permission, &input.patterns);

    if approval_mode == "full" {
        let (kind, title, body) = match outbound_domain.as_deref() {
            Some(domain) => (
                "data_egress",
                format!("自动授权数据外发：{}", domain),
                Some("会话处于完全访问模式".to_owned()),
            ),
            None => ("runtime_permission", title, Some("会话处于完全访问模式".to_owned())),
        };
        let item = auto_approve_and_reply(
            &state,
            pool,
            AutoApprove {
                kind,
                title,
                session_id: &session.id,
                agent_name,
                body,
                args,
                reason: "policy:session-full-access",
                provider_session_id,
                runtime,
            },
        )
        .await?;
        return Ok(Json(serde_json::json!({ "id": item })));
    }

    if approval_mode == "auto" && outbound_domain.is_none() {
        let item = auto_approve_and_reply(
            &state,
            pool,
            AutoApprove {
                kind: "runtime_permission",
                title,
                session_id: &session.id,
                agent_name,
                body: Some("会话处于替我审批模式，非风险操作自动放行".to_owned()),
                args,
                reason: "policy:session-auto-approve",
                provider_session_id,
                runtime,
            },
        )
        .await?;
        return Ok(Json(serde_json::json!({ "id": item })));
    }

    if let Some(domain) = outbound_domain {
        let whitelist =
            crate::db::managed_agents::settings::repository::get_outbound_domain_whitelist(pool)
                .await?
                .unwrap_or_default();

        if crate::db::managed_agents::settings::repository::match_domain_whitelist(
            &domain, &whitelist,
        ) {
            let item = auto_approve_and_reply(
                &state,
                pool,
                AutoApprove {
                    kind: "data_egress",
                    title: format!("自动授权数据外发：{}", domain),
                    session_id: &session.id,
                    agent_name,
                    body: Some(format!("匹配出站白名单：{}", domain)),
                    args,
                    reason: "policy:egress-whitelist",
                    provider_session_id,
                    runtime,
                },
            )
            .await?;
            return Ok(Json(serde_json::json!({ "id": item })));
        } else {
            let title = format!("数据外发审批请求：{}", domain);
            let body = Some(format!(
                "检测到非白名单出站请求：{}。需要安全管理审批。",
                domain
            ));
            let item = inbox::repository::create_approval(
                pool,
                "data_egress",
                title,
                Some(session.id.clone()),
                agent_name,
                body,
                Some(args),
            )
            .await?;

            state.local_session_events.publish(
                &session.id,
                serde_json::json!({
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
            );

            return Ok(Json(serde_json::json!({ "id": item.id })));
        }
    }

    let item = inbox::repository::create_approval(
        pool,
        "runtime_permission",
        title,
        Some(session.id.clone()),
        agent_name,
        body,
        Some(args),
    )
    .await?;

    state.local_session_events.publish(
        &session.id,
        serde_json::json!({
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
    );

    Ok(Json(serde_json::json!({ "id": item.id })))
}

fn approval_mode(environment: &Value) -> &str {
    match environment.get("approval_mode").and_then(Value::as_str) {
        Some(mode @ ("auto" | "full")) => mode,
        _ => "ask",
    }
}

struct AutoApprove<'a> {
    kind: &'a str,
    title: String,
    session_id: &'a str,
    agent_name: Option<String>,
    body: Option<String>,
    args: Value,
    reason: &'a str,
    provider_session_id: &'a str,
    runtime: &'a str,
}

/// Policy-driven fast path: file the inbox item for the audit trail, decide
/// it immediately, and unblock the paused tool call in the same request.
async fn auto_approve_and_reply(
    state: &Arc<AppState>,
    pool: &PgPool,
    input: AutoApprove<'_>,
) -> Result<String, GatewayError> {
    let item = inbox::repository::create_approval(
        pool,
        input.kind,
        input.title,
        Some(input.session_id.to_owned()),
        input.agent_name,
        input.body,
        Some(input.args),
    )
    .await?;
    inbox::repository::decide_approval(pool, &item.id, "accept", None, None, input.reason, "once")
        .await?;
    let decided = inbox::repository::get(pool, &item.id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("approval not found".to_owned()))?;
    match reply_direct(
        state,
        pool,
        &decided,
        ApprovalScope::Once,
        input.provider_session_id,
        input.runtime,
    )
    .await
    {
        Ok(()) => inbox::repository::mark_delivery_applied(pool, &item.id).await?,
        Err(error) => {
            inbox::repository::mark_delivery_failed(pool, &item.id, &error.to_string()).await?
        }
    }
    Ok(item.id)
}

/// Replies to opencode's own permission request so the paused tool call
/// resumes (or is permanently denied). Fire-and-forget: a failure here just
/// leaves the tool call blocked in opencode until it times out on its own —
/// safer than silently letting it through.
pub async fn reply(
    state: &Arc<AppState>,
    pool: &PgPool,
    item: &inbox::schema::InboxItemRow,
    scope: ApprovalScope,
) -> Result<(), GatewayError> {
    let Some(session_id) = item.session_id.as_deref() else {
        return Err(GatewayError::BadRequest(
            "approval missing session".to_owned(),
        ));
    };
    let session = sessions::repository::get(pool, session_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("approval session not found".to_owned()))?;
    let (Some(runtime), Some(provider_session_id)) = (
        session.runtime.as_deref(),
        session.provider_session_id.as_deref(),
    ) else {
        return Err(GatewayError::BadRequest(
            "approval session missing runtime/provider_session_id".to_owned(),
        ));
    };
    reply_direct(state, pool, item, scope, provider_session_id, runtime).await
}

pub async fn reply_direct(
    state: &Arc<AppState>,
    pool: &PgPool,
    item: &inbox::schema::InboxItemRow,
    scope: ApprovalScope,
    provider_session_id: &str,
    runtime: &str,
) -> Result<(), GatewayError> {
    let resolved = resolve_runtime(pool, state, runtime).await?;
    let Some(request_id) = item
        .args_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|value| {
            value
                .get("request_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
    else {
        return Err(GatewayError::BadRequest(
            "runtime approval missing request_id".to_owned(),
        ));
    };
    let reply_value = permission_reply_value(&item.status, scope);
    let base = resolved.credential.api_base.trim_end_matches('/');
    let url = format!("{base}/v1/sessions/{provider_session_id}/permissions/{request_id}/reply");
    let body = serde_json::json!({ "reply": reply_value, "message": item.feedback });
    let response = state
        .http
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(GatewayError::Upstream)?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(GatewayError::UpstreamHttp(status, body));
    }
    Ok(())
}

fn permission_reply_value(status: &str, scope: ApprovalScope) -> &'static str {
    if status != "accepted" {
        return "reject";
    }
    match scope {
        ApprovalScope::Once => "once",
        ApprovalScope::Session => "always",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_accepted_scope_to_opencode_reply() {
        assert_eq!(
            permission_reply_value("accepted", ApprovalScope::Once),
            "once"
        );
        assert_eq!(
            permission_reply_value("accepted", ApprovalScope::Session),
            "always"
        );
    }

    #[test]
    fn rejection_never_grants_permission() {
        assert_eq!(
            permission_reply_value("rejected", ApprovalScope::Session),
            "reject"
        );
    }

    #[test]
    fn approval_mode_defaults_to_ask() {
        assert_eq!(approval_mode(&serde_json::json!({})), "ask");
        assert_eq!(approval_mode(&serde_json::json!({"approval_mode": "auto"})), "auto");
        assert_eq!(approval_mode(&serde_json::json!({"approval_mode": "full"})), "full");
        assert_eq!(approval_mode(&serde_json::json!({"approval_mode": "bogus"})), "ask");
        assert_eq!(approval_mode(&serde_json::Value::Null), "ask");
    }

    #[test]
    fn test_is_outbound_request() {
        assert_eq!(
            is_outbound_request("web_request", &["api.github.com".to_owned()]),
            Some("api.github.com".to_owned())
        );
        assert_eq!(
            is_outbound_request("web_request", &["https://google.com/search".to_owned()]),
            Some("google.com".to_owned())
        );
        assert_eq!(
            is_outbound_request("read_file", &["/tmp/a.txt".to_owned()]),
            None
        );
        assert_eq!(
            is_outbound_request("Permission.Service.ask", &["*.txt".to_owned()]),
            None
        );
    }

}

fn is_outbound_request(permission: &str, patterns: &[String]) -> Option<String> {
    let is_net = permission.eq_ignore_ascii_case("outbound_request")
        || permission.eq_ignore_ascii_case("web_request")
        || permission.to_lowercase().contains("network")
        || permission.to_lowercase().contains("egress");

    for pattern in patterns {
        // Bare hostnames are meaningful only for an explicitly network-related
        // permission. For other permissions require a URL so file globs such
        // as `*.txt` are not mistaken for data-egress destinations.
        if is_net || pattern.contains("://") {
            if let Some(host) = extract_host(pattern) {
                return Some(host);
            }
        }
    }
    None
}

fn extract_host(pattern: &str) -> Option<String> {
    if pattern.starts_with('/') || pattern.starts_with("./") || pattern.starts_with("../") {
        return None;
    }
    let url_str = if pattern.contains("://") {
        pattern.to_owned()
    } else {
        format!("https://{}", pattern)
    };
    if let Ok(url) = reqwest::Url::parse(&url_str) {
        if let Some(host) = url.host_str() {
            if host == "localhost" || host.contains('.') {
                return Some(host.to_owned());
            }
        }
    }
    None
}
