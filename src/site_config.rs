//! Site-specific customization policy (deployment preferences).
//!
//! Kept in its own module so customizations stay isolated from the upstream
//! sources they affect — the handlers only gain a one-line filter call, and the
//! underlying `PROVIDER_CATALOG` / runtime catalog definitions are left fully
//! intact. Hidden entries are merely omitted from the lists shown in the UI;
//! every internal lookup (`catalog_entry`, runtime credential resolution, …)
//! still resolves them, so nothing breaks.

/// Provider ids hidden from the `/api/providers` listing.
///
/// `anthropic` and `openai` are hidden here rather than removed: the built-in
/// `claude_managed_agents` runtime still maps to the `anthropic` provider for
/// credential resolution, so the definition must remain.
pub const HIDDEN_PROVIDER_IDS: &[&str] = &["anthropic", "openai"];

/// Agent runtime ids hidden from the `/api/agent-runtimes` listing.
pub const HIDDEN_RUNTIME_IDS: &[&str] = &["claude_managed_agents"];

/// Whether a provider id should be shown in UI listings.
pub fn is_visible_provider(id: &str) -> bool {
    !HIDDEN_PROVIDER_IDS.contains(&id)
}

/// Whether an agent runtime id should be shown in UI listings.
pub fn is_visible_runtime(id: &str) -> bool {
    !HIDDEN_RUNTIME_IDS.contains(&id)
}
