use std::{collections::BTreeSet, sync::Arc};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    db::managed_agents::registry::{
        repository,
        schema::{CreateManagedAgent, ManagedAgentRow},
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
    for file in input.files {
        let parsed = parse_opencode_agent_file(&file.filename, &file.content)?;
        let row = repository::create(
            pool,
            create_input(parsed, &runtime, &owner_id, &file.filename),
        )
        .await?;
        archive_source(&state, &row.id, &file.filename, &file.content).await?;
        let _ =
            crate::db::managed_agents::registry::revisions::record(pool, &row, Some(&auth.user_id))
                .await;
        rows.push(row);
    }

    Ok((
        StatusCode::CREATED,
        Json(ImportOpencodeFilesResponse { agents: rows }),
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
}
