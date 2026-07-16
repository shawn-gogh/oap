use std::{collections::BTreeSet, sync::Arc};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    db::managed_agents::{
        audit,
        governance::{self, ImportedSource},
        registry::{
            repository, revisions,
            schema::{CreateManagedAgent, ManagedAgentRow},
        },
        sources::repository as source_repository,
    },
    errors::GatewayError,
    object_storage::ObjectStorageClient,
    proxy::{
        auth::master_key::{authenticate, AuthContext},
        state::AppState,
    },
};

#[derive(Debug, Deserialize)]
pub struct ImportOpencodeFilesRequest {
    runtime: Option<String>,
    owner_id: Option<String>,
    files: Vec<ImportOpencodeFile>,
}

#[derive(Debug, Deserialize)]
pub struct ImportOpencodeFile {
    filename: String,
    content: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ImportOpencodeFilesResponse {
    agents: Vec<ManagedAgentRow>,
    results: Vec<super::import_types::ImportItemResult>,
}

pub async fn import_opencode_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<ImportOpencodeFilesRequest>,
) -> Result<(StatusCode, Json<ImportOpencodeFilesResponse>), GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    if input.files.is_empty() {
        return Err(GatewayError::InvalidJsonMessage(
            "at least one file is required".to_owned(),
        ));
    }
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let owner_id = owner_id_for_import(&input, &auth);
    let runtime = input
        .runtime
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("local-opencode")
        .to_owned();

    let mut rows = Vec::with_capacity(input.files.len());
    let mut results = Vec::with_capacity(input.files.len());
    for file in input.files {
        let parsed = parse_opencode_agent_file(&file.filename, &file.content)?;
        let row = persist_imported_agent(
            &state,
            pool,
            &auth,
            &owner_id,
            &runtime,
            "opencode-file://upload",
            "opencode_agent_file",
            None,
            &file.filename,
            &file.content,
            parsed,
        )
        .await?;
        let source = source_repository::get_source_by_agent(pool, &row.id)
            .await?
            .ok_or_else(|| GatewayError::NotFound("agent source not found".to_owned()))?;
        let snapshot = source_repository::get_snapshot(pool, source.current_snapshot_id.as_deref())
            .await?
            .ok_or_else(|| GatewayError::NotFound("source snapshot not found".to_owned()))?;
        results.push(super::import_types::ImportItemResult {
            external_id: row
                .config
                .pointer("/source/external_agent_id")
                .and_then(Value::as_str)
                .unwrap_or(&row.id)
                .to_owned(),
            agent_id: Some(row.id.clone()),
            status: "imported",
            snapshot_id: Some(snapshot.id),
            issues: snapshot.normalization_issues,
        });
        rows.push(row);
    }

    Ok((
        StatusCode::CREATED,
        Json(ImportOpencodeFilesResponse {
            agents: rows,
            results,
        }),
    ))
}

fn owner_id_for_import(input: &ImportOpencodeFilesRequest, auth: &AuthContext) -> String {
    if auth.is_admin {
        if let Some(owner_id) = input
            .owner_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return owner_id.to_owned();
        }
    }
    auth.user_id.clone()
}

fn create_input(
    parsed: ParsedOpencodeAgent,
    runtime: &str,
    owner_id: &str,
    filename: &str,
) -> CreateManagedAgent {
    CreateManagedAgent {
        name: parsed.display_name.clone(),
        owner_id: owner_id.to_owned(),
        description: parsed.description.clone(),
        runtime: Some(runtime.to_owned()),
        harness: Some("claude-code".to_owned()),
        prompt: Some(parsed.system.clone()),
        tools: Some(json!(parsed.tools)),
        schedule: None,
        vault_keys: Some(json!([])),
        setup_commands: Some(json!([])),
        max_runtime_minutes: Some(30),
        on_failure: Some("pause_and_notify".to_owned()),
        config: Some(json!({
            "runtime": runtime,
            "source": {
                "kind": "opencode_agent_file",
                "provider": "opencode",
                "provider_name": "OpenCode",
                "filename": filename,
                "external_agent_id": parsed.id,
                "mode": parsed.mode,
                "raw_frontmatter": parsed.frontmatter,
                "archived_path": source_archive_path(filename),
            }
        })),
        model: Some(parsed.model.unwrap_or_else(|| "deepseek-chat".to_owned())),
        system: Some(parsed.system),
        skill_ids: Some(json!([])),
        rule_ids: Some(json!([])),
    }
}

async fn archive_source(
    state: &AppState,
    agent_id: &str,
    filename: &str,
    content: &str,
) -> Result<(), GatewayError> {
    let Some(storage) = &state.object_storage else {
        return Ok(());
    };
    let bucket = ObjectStorageClient::agent_bucket_name(agent_id);
    storage.ensure_bucket(&bucket).await?;
    storage
        .put_bytes(
            &bucket,
            &source_archive_path(filename),
            content.as_bytes().to_vec(),
        )
        .await
}

// ── Agent bundle import ─────────────────────────────────────────────────────
// A bundle is a zip carrying one or more opencode agent .md files plus
// arbitrary knowledge/eval files. Agents become managed agent rows; every
// other file lands in the primary agent's workspace bucket, so it is seeded
// into each new session automatically.

const MAX_BUNDLE_ENTRIES: usize = 200;
const MAX_BUNDLE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct ImportBundleRequest {
    filename: String,
    content_base64: String,
    runtime: Option<String>,
    owner_id: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ImportBundleResponse {
    agents: Vec<ManagedAgentRow>,
    knowledge_files: Vec<String>,
    results: Vec<super::import_types::ImportItemResult>,
}

pub async fn import_agent_bundle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<ImportBundleRequest>,
) -> Result<(StatusCode, Json<ImportBundleResponse>), GatewayError> {
    use base64::Engine as _;

    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let owner_id = if auth.is_admin {
        input
            .owner_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&auth.user_id)
            .to_owned()
    } else {
        auth.user_id.clone()
    };
    let runtime = input
        .runtime
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("local-opencode")
        .to_owned();

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input.content_base64.trim())
        .map_err(|error| {
            GatewayError::InvalidJsonMessage(format!("content_base64 is not valid base64: {error}"))
        })?;
    let entries = unpack_bundle(&bytes)?;

    let mut agent_files = Vec::new();
    let mut knowledge = Vec::new();
    for (path, data) in entries {
        // Only frontmatter-bearing .md files are agent definitions; plain
        // markdown is knowledge (the single-file importer's lenient fallback
        // would otherwise swallow every doc in the bundle as an "agent").
        if path.ends_with(".md") {
            if let Ok(text) = String::from_utf8(data.clone()) {
                if text.trim_start().starts_with("---") {
                    if let Ok(parsed) = parse_opencode_agent_file(&path, &text) {
                        agent_files.push((path, text, parsed));
                        continue;
                    }
                }
            }
        }
        knowledge.push((path, data));
    }
    if agent_files.is_empty() {
        return Err(GatewayError::InvalidJsonMessage(
            "bundle contains no importable agent .md file (frontmatter + prompt)".to_owned(),
        ));
    }

    let mut rows = Vec::with_capacity(agent_files.len());
    let mut results = Vec::with_capacity(agent_files.len());
    for (path, text, parsed) in agent_files {
        let source_endpoint = format!("agent-bundle://upload/{}", safe_filename(&input.filename));
        let row = persist_imported_agent(
            &state,
            pool,
            &auth,
            &owner_id,
            &runtime,
            &source_endpoint,
            "agent_bundle",
            Some(&input.filename),
            &path,
            &text,
            parsed,
        )
        .await?;
        let source = source_repository::get_source_by_agent(pool, &row.id)
            .await?
            .ok_or_else(|| GatewayError::NotFound("agent source not found".to_owned()))?;
        let snapshot = source_repository::get_snapshot(pool, source.current_snapshot_id.as_deref())
            .await?
            .ok_or_else(|| GatewayError::NotFound("source snapshot not found".to_owned()))?;
        results.push(super::import_types::ImportItemResult {
            external_id: row
                .config
                .pointer("/source/external_agent_id")
                .and_then(Value::as_str)
                .unwrap_or(&row.id)
                .to_owned(),
            agent_id: Some(row.id.clone()),
            status: "imported",
            snapshot_id: Some(snapshot.id),
            issues: snapshot.normalization_issues,
        });
        rows.push(row);
    }

    // Knowledge files seed the primary (first) agent's workspace.
    let mut knowledge_files = Vec::new();
    if !knowledge.is_empty() {
        let Some(storage) = &state.object_storage else {
            return Err(GatewayError::InvalidConfig(
                "bundle carries knowledge files but object storage is not configured".to_owned(),
            ));
        };
        let bucket = ObjectStorageClient::agent_bucket_name(&rows[0].id);
        storage.ensure_bucket(&bucket).await?;
        for (path, data) in knowledge {
            storage.put_bytes(&bucket, &path, data).await?;
            knowledge_files.push(path);
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(ImportBundleResponse {
            agents: rows,
            knowledge_files,
            results,
        }),
    ))
}

#[allow(clippy::too_many_arguments)]
async fn persist_imported_agent(
    state: &AppState,
    pool: &sqlx::PgPool,
    auth: &AuthContext,
    owner_id: &str,
    runtime: &str,
    source_endpoint: &str,
    source_kind: &str,
    bundle: Option<&str>,
    filename: &str,
    content: &str,
    parsed: ParsedOpencodeAgent,
) -> Result<ManagedAgentRow, GatewayError> {
    let external_agent_id = parsed.id.clone();
    let source_hash = imported_source_hash(content, runtime, source_endpoint);
    let existing = governance::find_by_source(
        pool,
        owner_id,
        "opencode",
        source_endpoint,
        &external_agent_id,
    )
    .await?;
    let unchanged = existing
        .as_ref()
        .is_some_and(|governance| governance.source_hash == source_hash);
    let mut create = create_input(parsed, runtime, owner_id, filename);
    if let Some(source) = create
        .config
        .as_mut()
        .and_then(|config| config.get_mut("source"))
        .and_then(Value::as_object_mut)
    {
        source.insert("kind".to_owned(), json!(source_kind));
        source.insert("source_hash".to_owned(), json!(source_hash));
        if let Some(bundle) = bundle {
            source.insert("bundle".to_owned(), json!(bundle));
        }
    }
    let row = match existing.as_ref() {
        Some(existing) if unchanged => repository::get(pool, &existing.agent_id)
            .await?
            .ok_or_else(|| GatewayError::NotFound("imported agent not found".to_owned()))?,
        Some(existing) => repository::update(
            pool,
            &existing.agent_id,
            super::import::update_from_import(create),
        )
        .await?
        .ok_or_else(|| GatewayError::NotFound("imported agent not found".to_owned()))?,
        None => repository::create(pool, create).await?,
    };
    archive_source(state, &row.id, filename, content).await?;
    let revision = if unchanged {
        match revisions::latest_version(pool, &row.id).await? {
            Some(version) => version,
            None => revisions::record(pool, &row, Some(&auth.user_id)).await?,
        }
    } else {
        revisions::record(pool, &row, Some(&auth.user_id)).await?
    };
    let governance = governance::record_import(
        pool,
        ImportedSource {
            agent_id: &row.id,
            owner_id,
            provider: "opencode",
            endpoint: source_endpoint,
            external_agent_id: &external_agent_id,
            source_hash: &source_hash,
            credential_scope: "byo",
            credential_name: None,
        },
    )
    .await?;
    let source = source_repository::ensure_source(pool, &governance, "managed", None).await?;
    let snapshot = source_repository::record_import_snapshot(
        pool,
        &source,
        &row,
        json!({
            "filename": filename,
            "archive_path": source_archive_path(filename),
            "content_digest": source_hash,
        }),
        &source_hash,
        revision,
        &auth.user_id,
    )
    .await?;
    audit::record(
        pool,
        &auth.user_id,
        if unchanged {
            "agent.source.checked"
        } else {
            "agent.source.imported"
        },
        "agent",
        &row.id,
        json!({
            "provider": "opencode",
            "source_kind": source_kind,
            "source_endpoint": source_endpoint,
            "external_agent_id": external_agent_id,
            "source_version": governance.source_version,
            "revision": revision,
            "source_id": source.id,
            "snapshot_id": snapshot.id,
            "changed": !unchanged,
        }),
    )
    .await?;
    Ok(row)
}

fn imported_source_hash(content: &str, runtime: &str, source_endpoint: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(content.as_bytes());
    hash.update([0]);
    hash.update(runtime.as_bytes());
    hash.update([0]);
    hash.update(source_endpoint.as_bytes());
    format!("{:x}", hash.finalize())
}

/// Vendor/build directories that never carry agent definitions or knowledge
/// content — skipped so e.g. a `node_modules` folder tagging along in a zip
/// doesn't blow the entry-count guard or the size guard.
const IGNORED_DIR_NAMES: &[&str] = &[
    "node_modules",
    ".git",
    ".venv",
    "venv",
    "__pycache__",
    "target",
    "dist",
    "build",
    ".next",
    ".cache",
];

fn is_ignored_path(path: &str) -> bool {
    path.split('/')
        .any(|segment| IGNORED_DIR_NAMES.contains(&segment))
}

/// Extracts a zip into (normalized relative path, bytes) pairs with basic
/// zip-bomb/path-traversal guards. If every entry shares a single top-level
/// directory (the common "zip of a folder" layout), that prefix is stripped.
fn unpack_bundle(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, GatewayError> {
    use std::io::Read as _;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| GatewayError::InvalidJsonMessage(format!("invalid zip: {error}")))?;
    let mut total: u64 = 0;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| GatewayError::InvalidJsonMessage(format!("invalid zip: {error}")))?;
        if file.is_dir() {
            continue;
        }
        let Some(path) = file.enclosed_name() else {
            return Err(GatewayError::InvalidJsonMessage(format!(
                "unsafe path in bundle: {}",
                file.name()
            )));
        };
        let path = path.to_string_lossy().replace('\\', "/");
        let basename = path.rsplit('/').next().unwrap_or_default().to_owned();
        if path.starts_with("__MACOSX/")
            || basename == ".DS_Store"
            || basename.is_empty()
            || is_ignored_path(&path)
        {
            continue;
        }
        if entries.len() >= MAX_BUNDLE_ENTRIES {
            return Err(GatewayError::InvalidJsonMessage(format!(
                "bundle has too many entries (max {MAX_BUNDLE_ENTRIES})"
            )));
        }
        total = total.saturating_add(file.size());
        if total > MAX_BUNDLE_BYTES {
            return Err(GatewayError::InvalidJsonMessage(format!(
                "bundle too large (max {} MB uncompressed)",
                MAX_BUNDLE_BYTES / 1024 / 1024
            )));
        }
        let mut data = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut data)
            .map_err(|error| GatewayError::InvalidJsonMessage(format!("invalid zip: {error}")))?;
        entries.push((path, data));
    }

    // Strip a shared single top-level directory.
    let prefix = entries
        .first()
        .and_then(|(path, _)| path.split_once('/').map(|(dir, _)| format!("{dir}/")));
    if let Some(prefix) = prefix {
        if entries.iter().all(|(path, _)| path.starts_with(&prefix)) {
            for (path, _) in &mut entries {
                *path = path[prefix.len()..].to_owned();
            }
            entries.retain(|(path, _)| !path.is_empty());
        }
    }
    Ok(entries)
}

fn source_archive_path(filename: &str) -> String {
    format!("source/{}", safe_filename(filename))
}

fn safe_filename(filename: &str) -> String {
    filename
        .split(['/', '\\'])
        .next_back()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("agent.md")
        .to_owned()
}

#[derive(Debug)]
struct ParsedOpencodeAgent {
    id: String,
    display_name: String,
    description: Option<String>,
    mode: Option<String>,
    model: Option<String>,
    system: String,
    tools: Vec<Value>,
    frontmatter: Value,
}

fn parse_opencode_agent_file(
    filename: &str,
    content: &str,
) -> Result<ParsedOpencodeAgent, GatewayError> {
    let (frontmatter, system) = split_frontmatter(content)?;
    if system.trim().is_empty() {
        return Err(GatewayError::InvalidJsonMessage(format!(
            "{filename} has an empty system prompt"
        )));
    }
    let id = file_stem(&safe_filename(filename));
    let display_name = string_field(&frontmatter, "display_name")
        .or_else(|| string_field(&frontmatter, "name"))
        .unwrap_or_else(|| id.clone());
    let description = string_field(&frontmatter, "description");
    let mode = string_field(&frontmatter, "mode");
    let model = string_field(&frontmatter, "model");
    let tools = tools_from_permission(frontmatter.get("permission"));

    Ok(ParsedOpencodeAgent {
        id,
        display_name,
        description,
        mode,
        model,
        system: system.trim().to_owned(),
        tools,
        frontmatter,
    })
}

fn split_frontmatter(content: &str) -> Result<(Value, String), GatewayError> {
    let normalized = content.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return Ok((json!({}), normalized));
    };
    let Some(end) = rest.find("\n---") else {
        return Err(GatewayError::InvalidJsonMessage(
            "frontmatter block is not closed".to_owned(),
        ));
    };
    let yaml = &rest[..end];
    let body = rest[end + "\n---".len()..]
        .trim_start_matches('\n')
        .to_owned();
    let frontmatter = serde_yaml::from_str::<serde_yaml::Value>(yaml)
        .map_err(GatewayError::ConfigParse)
        .and_then(|value| {
            serde_json::to_value(value).map_err(|error| {
                GatewayError::InvalidConfig(format!("invalid frontmatter: {error}"))
            })
        })?;
    Ok((frontmatter, body))
}

fn string_field(frontmatter: &Value, key: &str) -> Option<String> {
    frontmatter
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn file_stem(filename: &str) -> String {
    filename
        .trim_end_matches(".md")
        .trim_end_matches(".markdown")
        .trim()
        .replace([' ', '_'], "-")
        .to_lowercase()
}

fn tools_from_permission(permission: Option<&Value>) -> Vec<Value> {
    let Some(permission) = permission.and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut tools = BTreeSet::new();
    for (key, value) in permission {
        if permission_denied(value) {
            continue;
        }
        for tool in tools_for_permission_key(key) {
            tools.insert(tool);
        }
    }
    tools
        .into_iter()
        .map(|tool| json!({ "type": tool }))
        .collect()
}

fn permission_denied(value: &Value) -> bool {
    value
        .as_str()
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("deny"))
}

fn tools_for_permission_key(key: &str) -> Vec<&'static str> {
    match key {
        "read" | "list" => vec!["read"],
        "glob" => vec!["glob"],
        "grep" => vec!["grep"],
        "edit" => vec!["edit"],
        "write" => vec!["write"],
        "bash" => vec!["bash"],
        "webfetch" | "web_fetch" => vec!["web_fetch"],
        "websearch" | "web_search" => vec!["web_search"],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_opencode_agent_markdown() {
        let parsed = parse_opencode_agent_file(
            "agent-builder.md",
            r#"---
description: Builds agents
display_name: Agent Builder
mode: primary
model: deepseek-chat
permission:
  read: allow
  edit: ask
  bash: deny
---
You build agents.
"#,
        )
        .unwrap();

        assert_eq!(parsed.id, "agent-builder");
        assert_eq!(parsed.display_name, "Agent Builder");
        assert_eq!(parsed.description.as_deref(), Some("Builds agents"));
        assert_eq!(parsed.mode.as_deref(), Some("primary"));
        assert_eq!(parsed.model.as_deref(), Some("deepseek-chat"));
        assert_eq!(parsed.system, "You build agents.");
        assert_eq!(
            parsed.tools,
            vec![json!({ "type": "edit" }), json!({ "type": "read" })]
        );
    }

    #[test]
    fn falls_back_to_filename_without_frontmatter() {
        let parsed = parse_opencode_agent_file("review_bot.md", "Review code.").unwrap();

        assert_eq!(parsed.id, "review-bot");
        assert_eq!(parsed.display_name, "review-bot");
        assert_eq!(parsed.system, "Review code.");
        assert!(parsed.tools.is_empty());
    }

    #[test]
    fn imported_source_hash_is_stable_and_scoped() {
        let first = imported_source_hash(
            "Review code.",
            "claude_managed_agents",
            "opencode-file://upload",
        );
        let same = imported_source_hash(
            "Review code.",
            "claude_managed_agents",
            "opencode-file://upload",
        );

        assert_eq!(first, same);
        assert_ne!(
            first,
            imported_source_hash(
                "Review code carefully.",
                "claude_managed_agents",
                "opencode-file://upload"
            )
        );
        assert_ne!(
            first,
            imported_source_hash("Review code.", "other_runtime", "opencode-file://upload")
        );
        assert_ne!(
            first,
            imported_source_hash(
                "Review code.",
                "claude_managed_agents",
                "agent-bundle://upload/agents.zip"
            )
        );
    }
}
