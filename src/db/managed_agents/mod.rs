pub mod agent_grants;
pub mod api_keys;
pub mod artifacts;
pub mod audit;
pub mod cloud_events;
pub mod credential_leases;
pub mod eval_runs;
pub mod exposed_apps;
pub mod files;
pub mod governance;
pub mod groups;
pub mod harnesses;
pub mod identity_mappings;
pub mod inbox;
pub mod loops;
pub mod mattermost;
pub mod mcp_invocation_grants;
pub mod memory;
pub mod messages;
pub mod metrics;
pub mod pool;
pub mod quotas;
pub mod registry;
pub mod routines;
pub mod rules;
pub mod runs;
pub mod runtime_events;
pub mod runtime_refs;
pub mod saved;
pub mod session_control;
pub mod sessions;
pub mod settings;
pub mod skills;
pub mod sources;
pub mod spend_logs;
pub mod tasks;
pub mod users;
pub mod web_sessions;

pub fn id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}
