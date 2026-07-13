//! Site-specific customization policy (deployment preferences).
//!
//! Kept in its own module so customizations stay isolated from the upstream
//! sources they affect — the handlers only gain a one-line filter call, and the
//! underlying `PROVIDER_CATALOG` / runtime catalog definitions are left fully
//! intact. Hidden entries are merely omitted from the lists shown in the UI;
//! every internal lookup (`catalog_entry`, runtime credential resolution, …)
//! still resolves them, so nothing breaks.
//!
//! OAP policy: only open, self-hostable agent runtimes are offered as
//! first-class UI options. Closed-source vendor APIs (Anthropic, Cursor,
//! Gemini) are hidden from the connect-a-runtime and connect-a-provider
//! lists — but the `claude_managed_agents` runtime id and `anthropic`
//! provider id must stay in the underlying catalogs, because opencode/
//! hermes/openclaw (open-source harnesses this deployment DOES support)
//! register under that same api_spec/credential path. See
//! `src/http/sessions/runtime_resolution.rs` and `compose.yaml`'s
//! `RUNTIME_API_SPEC: claude_managed_agents` for the custom-harness
//! bridging that depends on this staying intact.

/// Provider ids hidden from the `/api/providers` listing.
///
/// `anthropic` and `openai` are hidden here rather than removed: the built-in
/// `claude_managed_agents` runtime still maps to the `anthropic` provider for
/// credential resolution, so the definition must remain.
pub const HIDDEN_PROVIDER_IDS: &[&str] = &["anthropic", "openai", "cursor", "gemini"];

/// Agent runtime ids hidden from the `/api/runtimes` listing.
pub const HIDDEN_RUNTIME_IDS: &[&str] = &["claude_managed_agents", "cursor", "gemini_antigravity"];

/// Whether a provider id should be shown in UI listings.
pub fn is_visible_provider(id: &str) -> bool {
    !HIDDEN_PROVIDER_IDS.contains(&id)
}

/// Whether an agent runtime id should be shown in UI listings.
pub fn is_visible_runtime(id: &str) -> bool {
    !HIDDEN_RUNTIME_IDS.contains(&id)
}
