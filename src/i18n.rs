//! Chinese localization for user-facing backend messages.
//!
//! Kept in its own module so the localization layer stays isolated from the
//! upstream sources it translates: `errors.rs` only gains a single call site,
//! and this file is the one place that maps gateway errors to Chinese. The
//! `#[error("…")]` templates on `GatewayError` remain English — they are used
//! for logs and `Debug`; only the HTTP response `message` shown to end users is
//! localized here.
//!
//! The match is exhaustive on purpose: if upstream adds a new `GatewayError`
//! variant, this file fails to compile, pointing exactly at the message that
//! still needs a translation rather than silently shipping English.

use crate::errors::GatewayError;

/// User-facing Chinese message for a gateway error.
///
/// Variants that carry an opaque source error (reqwest/sqlx/io) or a message
/// built at the call site keep that inner text verbatim and only wrap it with a
/// localized prefix, since the dynamic part cannot be translated generically.
pub fn localized_error_message(err: &GatewayError) -> String {
    match err {
        GatewayError::InvalidConfig(s) => format!("配置无效：{s}"),
        GatewayError::ConfigRead(e) => format!("读取配置失败：{e}"),
        GatewayError::ConfigParse(e) => format!("解析配置失败：{e}"),
        GatewayError::HttpClient(e) => format!("HTTP 客户端初始化失败：{e}"),
        GatewayError::InvalidJson(e) => format!("请求 JSON 无效：{e}"),
        GatewayError::InvalidJsonMessage(s) => format!("请求 JSON 无效：{s}"),
        GatewayError::MissingDatabase => "数据库未配置。".to_owned(),
        GatewayError::Database(e) => format!("数据库请求失败：{e}"),
        GatewayError::Migration(e) => format!("数据库迁移失败：{e}"),
        GatewayError::MissingModel => "缺少模型（model）。".to_owned(),
        GatewayError::UnknownModel(s) => format!("未知模型：{s}"),
        GatewayError::MissingMcpServer => "需要选择 MCP 服务器。".to_owned(),
        GatewayError::UnknownMcpServer(s) => format!("未知的 MCP 服务器：{s}"),
        GatewayError::UnknownAgent(s) => format!("未知的智能体：{s}"),
        GatewayError::UnknownAgentRun(s) => format!("未知的智能体运行：{s}"),
        // Carries a message built at the call site; pass it through unchanged.
        GatewayError::NotFound(s) => s.clone(),
        GatewayError::Unauthorized => "未授权。".to_owned(),
        GatewayError::Upstream(e) => format!("上游请求失败：{e}"),
        GatewayError::Sandbox(e) => format!("沙箱请求失败：{e}"),
        GatewayError::SandboxError(s) => format!("沙箱错误：{s}"),
        GatewayError::UpstreamHttp(code, body) => format!("上游返回 HTTP {code}：{body}"),
    }
}
