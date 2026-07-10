use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("invalid config: {0}")]
    InvalidConfig(String),

    #[error("config read failed: {0}")]
    ConfigRead(#[from] std::io::Error),

    #[error("config parse failed: {0}")]
    ConfigParse(#[from] serde_yaml::Error),

    #[error("http client init failed: {0}")]
    HttpClient(reqwest::Error),

    #[error("invalid request json: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error("invalid request json: {0}")]
    InvalidJsonMessage(String),

    #[error("invalid request: {0}")]
    BadRequest(String),

    #[error("database is not configured")]
    MissingDatabase,

    #[error("database request failed: {0}")]
    Database(sqlx::Error),

    #[error("database migration failed: {0}")]
    Migration(sqlx::migrate::MigrateError),

    #[error("missing model")]
    MissingModel,

    #[error("unknown model: {0}")]
    UnknownModel(String),

    #[error("mcp server selection is required")]
    MissingMcpServer,

    #[error("unknown mcp server: {0}")]
    UnknownMcpServer(String),

    #[error("unknown agent: {0}")]
    UnknownAgent(String),

    #[error("unknown agent run: {0}")]
    UnknownAgentRun(String),

    #[error("{0}")]
    NotFound(String),

    #[error("unauthorized")]
    Unauthorized,

    /// Authenticated with a valid key, but that identity lacks the required
    /// role/ownership. Distinct from `Unauthorized` (401, "we don't
    /// recognize this key at all") so the frontend doesn't treat a
    /// permission gap as a dead session and force a re-login.
    #[error("forbidden")]
    Forbidden,

    #[error("upstream request failed: {0}")]
    Upstream(reqwest::Error),

    #[error("sandbox request failed: {0}")]
    Sandbox(reqwest::Error),

    #[error("sandbox error: {0}")]
    SandboxError(String),

    #[error("upstream returned HTTP {0}: {1}")]
    UpstreamHttp(u16, String),
}

impl GatewayError {
    fn status(&self) -> StatusCode {
        match self {
            Self::InvalidConfig(_)
            | Self::ConfigRead(_)
            | Self::ConfigParse(_)
            | Self::HttpClient(_)
            | Self::Database(_)
            | Self::Migration(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::MissingDatabase => StatusCode::SERVICE_UNAVAILABLE,
            Self::InvalidJson(_)
            | Self::InvalidJsonMessage(_)
            | Self::BadRequest(_)
            | Self::MissingModel
            | Self::MissingMcpServer => StatusCode::BAD_REQUEST,
            Self::UnknownModel(_)
            | Self::UnknownMcpServer(_)
            | Self::UnknownAgent(_)
            | Self::UnknownAgentRun(_)
            | Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Upstream(_)
            | Self::Sandbox(_)
            | Self::SandboxError(_)
            | Self::UpstreamHttp(_, _) => StatusCode::BAD_GATEWAY,
        }
    }
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = Json(json!({
            "error": {
                "type": "gateway_error",
                "message": crate::i18n::localized_error_message(&self)
            }
        }));
        (status, body).into_response()
    }
}
