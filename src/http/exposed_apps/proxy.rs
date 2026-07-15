use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::{
        ws::{self, rejection::WebSocketUpgradeRejection, WebSocketUpgrade},
        Path, RawQuery, State,
    },
    http::{
        header::{AUTHORIZATION, CONTENT_LENGTH, COOKIE, HOST, LOCATION, SET_COOKIE},
        HeaderMap, HeaderName, HeaderValue, Method, StatusCode,
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt, TryStreamExt};
use reqwest::Url;
use sqlx::PgPool;
use tokio_tungstenite::tungstenite;

use crate::{
    db::managed_agents::{
        exposed_apps::{repository, schema::ExposedAppRow},
        now_ms, sessions,
    },
    errors::GatewayError,
    proxy::{
        auth::master_key::{authenticate, WEB_SESSION_COOKIE},
        state::AppState,
    },
};

use super::{resolve, share};

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// `/apps/{app_id}` — normalize to the trailing-slash form so the app's
/// relative asset URLs resolve under its prefix.
pub async fn root(Path(app_id): Path<String>, RawQuery(query): RawQuery) -> Response {
    let location = match query {
        Some(query) if !query.is_empty() => format!("/apps/{app_id}/?{query}"),
        _ => format!("/apps/{app_id}/"),
    };
    redirect(&location)
}

/// `/apps/{app_id}/` — the wildcard route does not match an empty suffix.
#[allow(clippy::too_many_arguments)]
pub async fn root_slash(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<String>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    method: Method,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Result<Response, GatewayError> {
    proxy_inner(
        state,
        app_id,
        String::new(),
        ws.ok(),
        method,
        headers,
        query,
        body,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn proxy(
    State(state): State<Arc<AppState>>,
    Path((app_id, path)): Path<(String, String)>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    method: Method,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Result<Response, GatewayError> {
    proxy_inner(state, app_id, path, ws.ok(), method, headers, query, body).await
}

#[allow(clippy::too_many_arguments)]
async fn proxy_inner(
    state: Arc<AppState>,
    app_id: String,
    path: String,
    ws: Option<WebSocketUpgrade>,
    method: Method,
    headers: HeaderMap,
    query: Option<String>,
    body: Bytes,
) -> Result<Response, GatewayError> {
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let app = repository::get_routable(pool, &app_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("exposed app not found: {app_id}")))?;

    let (query_token, forwarded_query) = split_token(query.as_deref());
    let reload_location = match forwarded_query.as_deref() {
        Some(query) => format!("/apps/{app_id}/{path}?{query}"),
        None => format!("/apps/{app_id}/{path}"),
    };
    match authorize(&state, &app, &headers, query_token.as_deref(), &reload_location).await? {
        Access::Redirect(response) => return Ok(response),
        Access::Allowed => {}
    }

    let session = sessions::repository::get(pool, &app.session_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("session not found: {}", app.session_id)))?;
    let mut target = resolve::container_base(pool, &state, &session).await?;
    target
        .set_port(Some(app.port as u16))
        .map_err(|_| GatewayError::InvalidConfig("cannot set upstream port".to_owned()))?;
    // Apps declaring preserve_prefix (Vite base / webpack publicPath) expect
    // the full prefixed path; plain root-served apps get the stripped one.
    if app.preserve_prefix {
        target.set_path(&format!("/apps/{app_id}/{path}"));
    } else {
        target.set_path(&format!("/{path}"));
    }
    target.set_query(forwarded_query.as_deref());
    target.set_fragment(None);

    if let Some(ws) = ws {
        return proxy_websocket(ws, target, &headers).await;
    }
    proxy_http(&state, pool, &app, target, method, &headers, body).await
}

enum Access {
    Allowed,
    Redirect(Response),
}

/// Order: share token in the query (sets the scoped cookie and redirects the
/// token out of the URL), then the app-scoped share cookie, then the owner's
/// gateway identity (web-session cookie or API key).
async fn authorize(
    state: &AppState,
    app: &ExposedAppRow,
    headers: &HeaderMap,
    query_token: Option<&str>,
    reload_location: &str,
) -> Result<Access, GatewayError> {
    let master_key = state.config.general_settings.master_key.as_deref();

    if let (Some(token), Some(key)) = (query_token, master_key) {
        if share::verify_token(key, token, &app.id, app.share_version, now_ms()) {
            return Ok(Access::Redirect(share_cookie_redirect(
                app,
                token,
                reload_location,
            )));
        }
    }
    if let (Some(token), Some(key)) = (cookie_value(headers, &share::share_cookie_name(&app.id)), master_key)
    {
        if share::verify_token(key, &token, &app.id, app.share_version, now_ms()) {
            return Ok(Access::Allowed);
        }
    }

    let auth = authenticate(headers, state).await?;
    if auth.is_admin || app.owner_user_id.as_deref() == Some(auth.user_id.as_str()) {
        return Ok(Access::Allowed);
    }
    Err(GatewayError::Forbidden)
}

/// Persist the verified token in an app-scoped HttpOnly cookie and reload the
/// URL without it, so the token doesn't linger in the address bar or get
/// leaked to the upstream app via Referer.
fn share_cookie_redirect(app: &ExposedAppRow, token: &str, location: &str) -> Response {
    let cookie = format!(
        "{}={token}; Path=/apps/{}; HttpOnly; SameSite=Lax",
        share::share_cookie_name(&app.id),
        app.id
    );
    let mut response = redirect(location);
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(SET_COOKIE, value);
    }
    response
}

async fn proxy_http(
    state: &AppState,
    _pool: &PgPool,
    app: &ExposedAppRow,
    target: Url,
    method: Method,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, GatewayError> {
    let method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|_| GatewayError::BadRequest("invalid method".to_owned()))?;
    let mut request = state.http.request(method, target);
    for (name, value) in forward_request_headers(headers) {
        request = request.header(name, value);
    }
    if !body.is_empty() {
        request = request.body(body);
    }

    let upstream = request.send().await.map_err(GatewayError::Upstream)?;
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let response_headers =
        forward_response_headers(upstream.headers(), &app.id, app.preserve_prefix);
    let stream = upstream.bytes_stream().map_err(std::io::Error::other);
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = status;
    *response.headers_mut() = response_headers;
    Ok(response)
}

fn forward_request_headers(headers: &HeaderMap) -> Vec<(HeaderName, HeaderValue)> {
    let mut forwarded = Vec::new();
    for (name, value) in headers {
        let lower = name.as_str().to_ascii_lowercase();
        if HOP_BY_HOP.contains(&lower.as_str())
            || name == HOST
            || name == CONTENT_LENGTH
            || name == AUTHORIZATION
            || name == COOKIE
            || lower.starts_with("sec-websocket-")
        {
            continue;
        }
        forwarded.push((name.clone(), value.clone()));
    }
    // Preserve the browser's original Host (standard reverse-proxy behavior):
    // dev servers with host checks (e.g. Vite's server.allowedHosts) reject
    // the container-internal hostname but allow localhost.
    if let Some(host) = headers.get(HOST) {
        forwarded.push((HOST, host.clone()));
        forwarded.push((HeaderName::from_static("x-forwarded-host"), host.clone()));
    }
    // Forward the app's own cookies but never the gateway's auth cookies.
    if let Some(filtered) = filtered_cookie_header(headers) {
        forwarded.push((COOKIE, filtered));
    }
    if let Ok(value) = HeaderValue::from_str("http") {
        forwarded.push((HeaderName::from_static("x-forwarded-proto"), value));
    }
    forwarded
}

fn filtered_cookie_header(headers: &HeaderMap) -> Option<HeaderValue> {
    let raw = headers.get(COOKIE)?.to_str().ok()?;
    let kept: Vec<&str> = raw
        .split(';')
        .map(str::trim)
        .filter(|cookie| {
            let name = cookie.split('=').next().unwrap_or("");
            name != WEB_SESSION_COOKIE && !name.starts_with("lap_app_")
        })
        .filter(|cookie| !cookie.is_empty())
        .collect();
    if kept.is_empty() {
        return None;
    }
    HeaderValue::from_str(&kept.join("; ")).ok()
}

fn forward_response_headers(
    headers: &reqwest::header::HeaderMap,
    app_id: &str,
    preserve_prefix: bool,
) -> HeaderMap {
    let mut copied = HeaderMap::new();
    for (name, value) in headers {
        let lower = name.as_str().to_ascii_lowercase();
        if HOP_BY_HOP.contains(&lower.as_str()) {
            continue;
        }
        let Ok(name) = HeaderName::from_bytes(name.as_str().as_bytes()) else {
            continue;
        };
        // Rewrite absolute-path redirects back under the app prefix (a
        // preserve_prefix app already emits prefixed paths).
        if name == LOCATION && !preserve_prefix {
            if let Ok(location) = value.to_str() {
                if let Some(rewritten) = rewrite_location(location, app_id) {
                    if let Ok(rewritten) = HeaderValue::from_str(&rewritten) {
                        copied.insert(name, rewritten);
                        continue;
                    }
                }
            }
        }
        let Ok(value) = HeaderValue::from_bytes(value.as_bytes()) else {
            continue;
        };
        copied.append(name, value);
    }
    copied
}

fn rewrite_location(location: &str, app_id: &str) -> Option<String> {
    if location.starts_with('/') && !location.starts_with("//") {
        Some(format!("/apps/{app_id}{location}"))
    } else {
        None
    }
}

async fn proxy_websocket(
    ws: WebSocketUpgrade,
    mut target: Url,
    headers: &HeaderMap,
) -> Result<Response, GatewayError> {
    let scheme = if target.scheme() == "https" { "wss" } else { "ws" };
    target
        .set_scheme(scheme)
        .map_err(|_| GatewayError::InvalidConfig("cannot set ws scheme".to_owned()))?;

    let mut request = tungstenite::client::IntoClientRequest::into_client_request(target.as_str())
        .map_err(|error| GatewayError::UpstreamHttp(502, format!("ws request: {error}")))?;
    if let Some(protocols) = headers.get("sec-websocket-protocol") {
        if let Ok(value) = tungstenite::http::HeaderValue::from_bytes(protocols.as_bytes()) {
            request
                .headers_mut()
                .insert("sec-websocket-protocol", value);
        }
    }
    // Same original-Host preservation as the HTTP path (Vite HMR checks it).
    if let Some(host) = headers.get(HOST) {
        if let Ok(value) = tungstenite::http::HeaderValue::from_bytes(host.as_bytes()) {
            request.headers_mut().insert("host", value);
        }
    }

    let (upstream, response) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|error| GatewayError::UpstreamHttp(502, format!("ws connect: {error}")))?;

    let negotiated = response
        .headers()
        .get("sec-websocket-protocol")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let ws = match negotiated {
        Some(protocol) => ws.protocols([protocol]),
        None => ws,
    };

    Ok(ws.on_upgrade(move |client| pump(client, upstream)))
}

/// Bidirectional relay; either side closing (or erroring) tears down both.
async fn pump(
    client: ws::WebSocket,
    upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    let (mut client_tx, mut client_rx) = client.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();

    let client_to_upstream = async {
        while let Some(Ok(message)) = client_rx.next().await {
            let outbound = client_to_tungstenite(message);
            let is_close = matches!(outbound, tungstenite::Message::Close(_));
            if upstream_tx.send(outbound).await.is_err() || is_close {
                break;
            }
        }
        let _ = upstream_tx.close().await;
    };

    let upstream_to_client = async {
        while let Some(Ok(message)) = upstream_rx.next().await {
            let Some(outbound) = tungstenite_to_client(message) else {
                continue;
            };
            let is_close = matches!(outbound, ws::Message::Close(_));
            if client_tx.send(outbound).await.is_err() || is_close {
                break;
            }
        }
        let _ = client_tx.close().await;
    };

    tokio::join!(client_to_upstream, upstream_to_client);
}

fn client_to_tungstenite(message: ws::Message) -> tungstenite::Message {
    match message {
        ws::Message::Text(text) => tungstenite::Message::text(text.as_str().to_owned()),
        ws::Message::Binary(data) => tungstenite::Message::Binary(data),
        ws::Message::Ping(data) => tungstenite::Message::Ping(data),
        ws::Message::Pong(data) => tungstenite::Message::Pong(data),
        ws::Message::Close(frame) => tungstenite::Message::Close(frame.map(|frame| {
            tungstenite::protocol::CloseFrame {
                code: frame.code.into(),
                reason: frame.reason.as_str().to_owned().into(),
            }
        })),
    }
}

fn tungstenite_to_client(message: tungstenite::Message) -> Option<ws::Message> {
    Some(match message {
        tungstenite::Message::Text(text) => ws::Message::Text(text.as_str().into()),
        tungstenite::Message::Binary(data) => ws::Message::Binary(data),
        tungstenite::Message::Ping(data) => ws::Message::Ping(data),
        tungstenite::Message::Pong(data) => ws::Message::Pong(data),
        tungstenite::Message::Close(frame) => ws::Message::Close(frame.map(|frame| {
            ws::CloseFrame {
                code: frame.code.into(),
                reason: frame.reason.as_str().into(),
            }
        })),
        tungstenite::Message::Frame(_) => return None,
    })
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (cookie_name, value) = cookie.trim().split_once('=')?;
                (cookie_name == name).then(|| value.to_owned())
            })
        })
}

/// Splits the gateway's `token` param out of the query string; everything
/// else is forwarded to the upstream app untouched.
fn split_token(query: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(query) = query else {
        return (None, None);
    };
    let mut token = None;
    let mut kept = Vec::new();
    for pair in query.split('&') {
        match pair.strip_prefix("token=") {
            Some(value) if token.is_none() => token = Some(value.to_owned()),
            _ => kept.push(pair),
        }
    }
    let forwarded = if kept.is_empty() {
        None
    } else {
        Some(kept.join("&"))
    };
    (token, forwarded)
}

fn redirect(location: &str) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::TEMPORARY_REDIRECT;
    if !location.is_empty() {
        if let Ok(value) = HeaderValue::from_str(location) {
            response.headers_mut().insert(LOCATION, value);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_token_from_query() {
        let (token, rest) = split_token(Some("a=1&token=abc&b=2"));
        assert_eq!(token.as_deref(), Some("abc"));
        assert_eq!(rest.as_deref(), Some("a=1&b=2"));
    }

    #[test]
    fn no_token_leaves_query_untouched() {
        let (token, rest) = split_token(Some("a=1"));
        assert!(token.is_none());
        assert_eq!(rest.as_deref(), Some("a=1"));
    }

    #[test]
    fn rewrites_absolute_location_only() {
        assert_eq!(
            rewrite_location("/login", "app_1").as_deref(),
            Some("/apps/app_1/login")
        );
        assert!(rewrite_location("https://other/", "app_1").is_none());
        assert!(rewrite_location("//other/x", "app_1").is_none());
    }
}
