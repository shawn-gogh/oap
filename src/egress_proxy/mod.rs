//! Outbound network egress proxy: an HTTP CONNECT forward proxy embedded in
//! `lap` that agent runtime containers are made to route all outbound HTTP(S)
//! traffic through (via `HTTP_PROXY`/`HTTPS_PROXY` env vars set on each
//! session's opencode process — see templates/opencode/src/session-pool.mjs).
//!
//! Why this exists: the self-reported `data_egress` approval path in
//! tool_approvals.rs depends on the runtime voluntarily telling us "this is a
//! network request" — a bash-invoked `curl` never does. This proxy enforces
//! the same outbound domain whitelist at the network layer instead, so
//! coverage doesn't depend on the tool cooperating. Domain filtering happens
//! on the CONNECT line, before the TLS handshake, so it needs no MITM/CA
//! trust — the tunnel is opened blind once allowed and the actual traffic
//! stays end-to-end encrypted.
//!
//! Identity: each session's proxy credentials are `session_id:gateway_key`
//! (HTTP Basic on `Proxy-Authorization`), so a decision can be scoped to that
//! session's `approval_mode` without any new secret management — the key is
//! the same one already used for the wrapper's other gateway calls.

use std::sync::Arc;

use base64::Engine;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use crate::{
    db::managed_agents::{inbox, registry, runtime_events, sessions, settings},
    guardian::{self, GuardianContext},
    proxy::{auth::master_key::authenticate_explicit_key, state::AppState},
};

const DEFAULT_PORT: u16 = 3128;
/// CONNECT line + headers must fit in this; anything larger is not a proxy
/// request we support and gets rejected rather than parsed further.
const MAX_HEADER_BYTES: usize = 8 * 1024;

pub fn spawn(state: Arc<AppState>) {
    if state.db.is_none() {
        return;
    }
    let port: u16 = std::env::var("EGRESS_PROXY_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    tokio::spawn(async move {
        let addr = format!("0.0.0.0:{port}");
        let listener = match TcpListener::bind(&addr).await {
            Ok(listener) => listener,
            Err(error) => {
                tracing::error!("egress proxy failed to bind {addr}: {error}");
                return;
            }
        };
        tracing::info!("egress proxy listening on {addr}");
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(pair) => pair,
                Err(error) => {
                    tracing::warn!("egress proxy accept failed: {error}");
                    continue;
                }
            };
            let state = state.clone();
            tokio::spawn(async move {
                if let Err(error) = handle_connection(state, stream).await {
                    tracing::debug!("egress proxy connection error: {error}");
                }
            });
        }
    });
}

async fn handle_connection(state: Arc<AppState>, mut stream: TcpStream) -> std::io::Result<()> {
    let Some(request) = read_connect_request(&mut stream).await? else {
        respond(&mut stream, 501, "Not Implemented").await?;
        return Ok(());
    };

    let pool = state.db.as_ref().expect("checked in spawn");

    let Some(session_id) = request.proxy_username.as_deref() else {
        respond(&mut stream, 407, "Proxy Authentication Required").await?;
        return Ok(());
    };
    let Some(key) = request.proxy_password.as_deref() else {
        respond(&mut stream, 407, "Proxy Authentication Required").await?;
        return Ok(());
    };
    if authenticate_explicit_key(key, &state).await.is_err() {
        respond(&mut stream, 407, "Proxy Authentication Required").await?;
        return Ok(());
    }

    let Ok(Some(session)) = sessions::repository::get(pool, session_id).await else {
        respond(&mut stream, 407, "Proxy Authentication Required").await?;
        return Ok(());
    };

    let decision = decide(&state, pool, &session, &request.host, request.port).await;
    audit(pool, &session, &request.host, &decision).await;

    match decision {
        Decision::Allow(_) => {
            respond(&mut stream, 200, "Connection Established").await?;
            tunnel(&mut stream, &request.host, request.port).await?;
        }
        Decision::Deny(_) => {
            respond(&mut stream, 403, "Forbidden").await?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
enum Decision {
    Allow(String),
    Deny(String),
}

/// `ask`/`auto` non-whitelisted-domain denials are always final here — unlike
/// the tool-permission choke point in tool_approvals.rs, there's no "route to
/// a human" fallback for a live TCP CONNECT: a raw socket can't sit open
/// waiting on an async approval decision the way a paused opencode tool call
/// can. So a Guardian denial (or a failed Guardian call — fail-closed) simply
/// closes the connection; the agent sees a normal connection failure and can
/// retry through other means (e.g. asking a human via `request_human_approval`)
/// if it still needs that host.
async fn decide(
    state: &Arc<AppState>,
    pool: &sqlx::PgPool,
    session: &sessions::schema::SessionRow,
    host: &str,
    port: u16,
) -> Decision {
    let mode = approval_mode(&session.environment_json);
    if mode == "full" {
        return Decision::Allow("policy:session-full-access".to_owned());
    }
    let whitelist = settings::repository::get_outbound_domain_whitelist(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    if settings::repository::match_domain_whitelist(host, &whitelist) {
        return Decision::Allow("policy:egress-proxy-whitelist".to_owned());
    }
    if mode != "auto" {
        return Decision::Deny("policy:egress-proxy-denied".to_owned());
    }

    let agent = match session.agent_id.as_deref() {
        Some(agent_id) => registry::repository::get(pool, agent_id).await.ok().flatten(),
        None => None,
    };
    let context = GuardianContext {
        action_description: format!("Outbound network connection to {host}:{port}"),
        target: Some(format!("{host}:{port}")),
        agent_name: agent.as_ref().map(|agent| agent.name.clone()),
        recent_user_message: recent_user_message(pool, &session.id).await,
        fallback_model: agent.map(|agent| agent.model).unwrap_or_default(),
    };
    let verdict = guardian::review(state, pool, &context).await;

    if verdict.allow {
        state.guardian_circuit_breaker.record_non_denial(&session.id);
        return Decision::Allow(format!("guardian:allow — {}", verdict.reason));
    }

    let breaker_action = state.guardian_circuit_breaker.record_denial(&session.id);
    if let guardian::CircuitBreakerAction::InterruptTurn {
        consecutive_denials,
        recent_denials,
    } = breaker_action
    {
        let _ = crate::http::sessions::abort_session_internal(
            state,
            pool,
            session,
            "guardian: too many denied actions this turn",
        )
        .await;
        return Decision::Deny(format!(
            "guardian:circuit-breaker — {consecutive_denials} consecutive / {recent_denials} recent denials, turn aborted"
        ));
    }

    Decision::Deny(format!("guardian:deny — {}", verdict.reason))
}

/// Same evidence-gathering helper as tool_approvals.rs — kept local since
/// wiring a shared helper across the two modules isn't worth the coupling
/// for a single small function.
async fn recent_user_message(pool: &sqlx::PgPool, session_id: &str) -> Option<String> {
    let events = runtime_events::repository::list(pool, session_id).await.ok()?;
    events.iter().rev().find_map(|event| {
        if event.get("type").and_then(serde_json::Value::as_str) != Some("user.message") {
            return None;
        }
        let content = event.get("content")?.as_array()?;
        let text: String = content
            .iter()
            .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        (!text.trim().is_empty()).then_some(text)
    })
}

fn approval_mode(environment: &serde_json::Value) -> &str {
    match environment.get("approval_mode").and_then(|v| v.as_str()) {
        Some(mode @ ("auto" | "full")) => mode,
        _ => "ask",
    }
}

async fn audit(
    pool: &sqlx::PgPool,
    session: &sessions::schema::SessionRow,
    host: &str,
    decision: &Decision,
) {
    let (title, reason) = match decision {
        Decision::Allow(reason) => (format!("自动授权数据外发：{host}"), reason.clone()),
        Decision::Deny(reason) => (format!("已拒绝的数据外发：{host}"), reason.clone()),
    };
    let args = serde_json::json!({ "host": host, "via": "egress_proxy" });
    let Ok(item) = inbox::repository::create_approval(
        pool,
        "data_egress",
        title,
        Some(session.id.clone()),
        None,
        Some(format!("出站代理判定：{reason}")),
        Some(args),
    )
    .await
    else {
        return;
    };
    let decision_str = if matches!(decision, Decision::Allow(_)) {
        "accept"
    } else {
        "reject"
    };
    let _ = inbox::repository::decide_approval(
        pool,
        &item.id,
        decision_str,
        None,
        None,
        &reason,
        "once",
    )
    .await;
}

async fn tunnel(client: &mut TcpStream, host: &str, port: u16) -> std::io::Result<()> {
    let mut upstream = TcpStream::connect((host, port)).await?;
    tokio::io::copy_bidirectional(client, &mut upstream).await?;
    Ok(())
}

struct ConnectRequest {
    host: String,
    port: u16,
    proxy_username: Option<String>,
    proxy_password: Option<String>,
}

/// Reads and parses a minimal HTTP/1.1 `CONNECT host:port HTTP/1.1` request
/// plus headers up to the blank line. Returns `Ok(None)` for anything that
/// isn't a well-formed CONNECT (the only method a forward proxy needs to
/// support here — legitimate outbound traffic from these sessions is HTTPS).
async fn read_connect_request(
    stream: &mut TcpStream,
) -> std::io::Result<Option<ConnectRequest>> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 512];
    loop {
        if buf.len() > MAX_HEADER_BYTES {
            return Ok(None);
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_header_end(&buf) {
            let head = String::from_utf8_lossy(&buf[..pos]).into_owned();
            return Ok(parse_connect(&head));
        }
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn parse_connect(head: &str) -> Option<ConnectRequest> {
    let mut lines = head.split("\r\n");
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    if !parts.next()?.eq_ignore_ascii_case("connect") {
        return None;
    }
    let authority = parts.next()?;
    let (host, port) = authority.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    if host.is_empty() {
        return None;
    }

    let mut proxy_username = None;
    let mut proxy_password = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("proxy-authorization") {
            if let Some((user, pass)) = decode_basic_auth(value.trim()) {
                proxy_username = Some(user);
                proxy_password = Some(pass);
            }
        }
    }

    Some(ConnectRequest {
        host: host.to_owned(),
        port,
        proxy_username,
        proxy_password,
    })
}

fn decode_basic_auth(header_value: &str) -> Option<(String, String)> {
    let encoded = header_value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (user, pass) = text.split_once(':')?;
    Some((user.to_owned(), pass.to_owned()))
}

async fn respond(stream: &mut TcpStream, status: u16, reason: &str) -> std::io::Result<()> {
    let response = format!("HTTP/1.1 {status} {reason}\r\nConnection: close\r\n\r\n");
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_connect_with_proxy_auth() {
        let head = "CONNECT httpbin.org:443 HTTP/1.1\r\nHost: httpbin.org:443\r\nProxy-Authorization: Basic c2VzX2FiYzpzay1sb2NhbA==\r\n\r\n";
        let request = parse_connect(head).expect("should parse");
        assert_eq!(request.host, "httpbin.org");
        assert_eq!(request.port, 443);
        assert_eq!(request.proxy_username.as_deref(), Some("ses_abc"));
        assert_eq!(request.proxy_password.as_deref(), Some("sk-local"));
    }

    #[test]
    fn rejects_non_connect_methods() {
        let head = "GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n";
        assert!(parse_connect(head).is_none());
    }

    #[test]
    fn rejects_connect_without_auth() {
        let head = "CONNECT httpbin.org:443 HTTP/1.1\r\nHost: httpbin.org:443\r\n\r\n";
        let request = parse_connect(head).expect("should parse");
        assert!(request.proxy_username.is_none());
    }

    #[test]
    fn approval_mode_defaults_to_ask() {
        assert_eq!(approval_mode(&serde_json::json!({})), "ask");
        assert_eq!(
            approval_mode(&serde_json::json!({"approval_mode": "full"})),
            "full"
        );
        assert_eq!(
            approval_mode(&serde_json::json!({"approval_mode": "auto"})),
            "auto"
        );
    }
}
