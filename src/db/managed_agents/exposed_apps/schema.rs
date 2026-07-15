use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ExposedAppRow {
    pub id: String,
    pub session_id: String,
    pub agent_id: String,
    pub owner_user_id: Option<String>,
    pub container_key: String,
    pub port: i32,
    pub name: Option<String>,
    pub share_version: i32,
    pub status: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub deleted_at: Option<i64>,
    /// Forward the full /apps/{id}/... path to the upstream instead of
    /// stripping the prefix — for apps configured with a base path (Vite
    /// `base`, webpack `publicPath`).
    pub preserve_prefix: bool,
}

#[derive(Debug)]
pub struct NewExposedApp<'a> {
    pub session_id: &'a str,
    pub agent_id: &'a str,
    pub owner_user_id: Option<&'a str>,
    pub container_key: &'a str,
    pub name: Option<&'a str>,
    pub expires_at: Option<i64>,
    pub preserve_prefix: bool,
}
