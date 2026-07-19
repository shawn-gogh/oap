use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    db::managed_agents::{catalog as repository, skills},
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

#[derive(Debug, Default, Deserialize)]
pub struct CatalogQuery {
    q: Option<String>,
    tag: Option<String>,
    capability: Option<String>,
    access: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CatalogAgent {
    id: String,
    name: String,
    description: Option<String>,
    owner_id: Option<String>,
    runtime: String,
    tags: Vec<String>,
    capabilities: Vec<String>,
    can_use: bool,
    access: String,
    consumers: Vec<repository::CatalogConsumerRow>,
    session_count: i64,
    last_used_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CatalogResponse {
    agents: Vec<CatalogAgent>,
    tags: Vec<String>,
    capabilities: Vec<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<CatalogQuery>,
) -> Result<Json<CatalogResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let sources = repository::list_sources(pool).await?;
    let ids = sources
        .iter()
        .map(|source| source.id.clone())
        .collect::<Vec<_>>();
    let consumers = consumers_by_agent(repository::list_consumers(pool, &ids).await?);
    let skills = skills::repository::list(pool, None)
        .await?
        .into_iter()
        .map(|skill| (skill.id, skill.name))
        .collect::<HashMap<_, _>>();
    let granted = granted_agent_ids(pool, &auth.user_id).await?;
    let mut agents = sources
        .into_iter()
        .map(|source| {
            let can_use = auth.is_admin
                || source.owner_id.as_deref() == Some(auth.user_id.as_str())
                || granted.iter().any(|id| id == &source.id);
            let access = access_label(&auth.user_id, auth.is_admin, &source, can_use);
            // The consumer roster (who uses this agent, how often, when last)
            // is only disclosed for agents the caller may actually use.
            // Attaching it to agents the caller has no grant to would leak,
            // tenant-wide, which users consume which agents — a cross-owner
            // privacy disclosure. Unavailable agents still surface their
            // public metadata (name/description/capabilities) for discovery,
            // just without the roster and usage aggregates.
            let agent_consumers = if can_use {
                consumers.get(&source.id).cloned().unwrap_or_default()
            } else {
                Vec::new()
            };
            catalog_agent(source, &skills, agent_consumers, can_use, access)
        })
        .collect::<Vec<_>>();
    agents.retain(|agent| matches_query(agent, &query));
    agents.truncate(100);
    let tags = facets(&agents, |agent| &agent.tags);
    let capabilities = facets(&agents, |agent| &agent.capabilities);
    Ok(Json(CatalogResponse {
        agents,
        tags,
        capabilities,
    }))
}

async fn granted_agent_ids(
    pool: &sqlx::PgPool,
    user_id: &str,
) -> Result<Vec<String>, GatewayError> {
    let mut ids =
        crate::db::managed_agents::agent_grants::repository::agent_ids_for_user(pool, user_id)
            .await?;
    ids.extend(
        crate::db::managed_agents::groups::agent_grants::agent_ids_for_user(pool, user_id).await?,
    );
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn catalog_agent(
    source: repository::CatalogSourceRow,
    skills: &HashMap<String, String>,
    consumers: Vec<repository::CatalogConsumerRow>,
    can_use: bool,
    access: String,
) -> CatalogAgent {
    let tags = string_values(source.config.get("tags"));
    let capabilities = capabilities(&source, skills);
    let session_count = consumers
        .iter()
        .map(|consumer| consumer.session_count)
        .sum();
    let last_used_at = consumers.iter().map(|consumer| consumer.last_used_at).max();
    CatalogAgent {
        id: source.id,
        name: source.name,
        description: source.description,
        owner_id: source.owner_id,
        runtime: source.harness,
        tags,
        capabilities,
        can_use,
        access,
        consumers,
        session_count,
        last_used_at,
    }
}

fn capabilities(
    source: &repository::CatalogSourceRow,
    skills: &HashMap<String, String>,
) -> Vec<String> {
    let mut values = string_values(source.config.get("capabilities"));
    values.extend(tool_names(&source.tools));
    values.extend(
        string_values(Some(&source.skill_ids))
            .into_iter()
            .filter_map(|skill_id| skills.get(&skill_id).cloned()),
    );
    values.extend(string_values(source.config.get("platform_mcp_ids")));
    normalize(values)
}

fn tool_names(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| match tool {
            Value::String(name) => Some(name.clone()),
            Value::Object(object) => ["name", "id", "type"]
                .iter()
                .find_map(|key| object.get(*key).and_then(Value::as_str).map(str::to_owned)),
            _ => None,
        })
        .collect()
}

fn string_values(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn normalize(values: Vec<String>) -> Vec<String> {
    let mut values = values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort_by_key(|value| value.to_lowercase());
    values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    values
}

fn consumers_by_agent(
    consumers: Vec<repository::CatalogConsumerRow>,
) -> HashMap<String, Vec<repository::CatalogConsumerRow>> {
    let mut grouped = HashMap::new();
    for consumer in consumers {
        grouped
            .entry(consumer.agent_id.clone())
            .or_insert_with(Vec::new)
            .push(consumer);
    }
    grouped
}

fn access_label(
    user_id: &str,
    is_admin: bool,
    source: &repository::CatalogSourceRow,
    can_use: bool,
) -> String {
    if is_admin {
        "admin"
    } else if source.owner_id.as_deref() == Some(user_id) {
        "owner"
    } else if can_use {
        "granted"
    } else {
        "unavailable"
    }
    .to_owned()
}

fn matches_query(agent: &CatalogAgent, query: &CatalogQuery) -> bool {
    let search = query.q.as_deref().unwrap_or("").trim().to_lowercase();
    let search_matches = search.is_empty()
        || agent.name.to_lowercase().contains(&search)
        || agent
            .description
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains(&search)
        || agent
            .tags
            .iter()
            .chain(&agent.capabilities)
            .any(|value| value.to_lowercase().contains(&search));
    search_matches
        && exact_filter(&agent.tags, query.tag.as_deref())
        && exact_filter(&agent.capabilities, query.capability.as_deref())
        && (query.access.as_deref() != Some("mine") || agent.can_use)
}

fn exact_filter(values: &[String], filter: Option<&str>) -> bool {
    filter
        .map(str::trim)
        .filter(|filter| !filter.is_empty())
        .is_none_or(|filter| {
            values
                .iter()
                .any(|value| value.eq_ignore_ascii_case(filter))
        })
}

fn facets(agents: &[CatalogAgent], values: impl Fn(&CatalogAgent) -> &[String]) -> Vec<String> {
    normalize(agents.iter().flat_map(values).cloned().collect())
}
