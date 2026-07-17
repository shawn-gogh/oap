use std::collections::HashSet;

use serde_json::{json, Value};
use sqlx::PgPool;

use crate::{
    db::{
        managed_agents::{id, now_ms, registry, session_control::schema::TurnSnapshot, sessions},
        mcp_servers,
    },
    errors::GatewayError,
};

use super::schema::McpInvocationGrantRow;

struct GrantScope {
    server_id: String,
    allowed_tools: Vec<String>,
    allow_all: bool,
    metadata: Value,
}

pub async fn ensure_for_turn(
    pool: &PgPool,
    snapshot: &TurnSnapshot,
) -> Result<Vec<McpInvocationGrantRow>, GatewayError> {
    if !matches!(
        snapshot.turn.status.as_str(),
        "queued" | "running" | "waiting_input" | "waiting_approval" | "cancelling"
    ) {
        return Ok(Vec::new());
    }
    let Some(invocation) = snapshot.invocations.first() else {
        return Ok(Vec::new());
    };
    let session = sessions::repository::get(pool, &snapshot.turn.session_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("session not found".to_owned()))?;
    let Some(agent_id) = session
        .agent_id
        .as_deref()
        .or(invocation.agent_id.as_deref())
    else {
        return Ok(Vec::new());
    };
    let agent = registry::repository::get(pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::UnknownAgent(agent_id.to_owned()))?;
    let owner_id = session
        .owner_id
        .as_deref()
        .or(agent.owner_id.as_deref())
        .unwrap_or("default");
    let ttl_ms = i64::from(agent.max_runtime_minutes.clamp(1, 1_440)) * 60_000;
    let scopes = grant_scopes(pool, &agent).await?;
    let mut grants = Vec::with_capacity(scopes.len());
    for scope in scopes {
        grants.push(
            issue(
                pool,
                owner_id,
                &snapshot.turn.session_id,
                &snapshot.turn.id,
                &invocation.id,
                &scope,
                ttl_ms,
            )
            .await?,
        );
    }
    Ok(grants)
}

async fn issue(
    pool: &PgPool,
    owner_id: &str,
    session_id: &str,
    turn_id: &str,
    invocation_id: &str,
    scope: &GrantScope,
    ttl_ms: i64,
) -> Result<McpInvocationGrantRow, GatewayError> {
    let now = now_ms();
    let expires_at = now.saturating_add(ttl_ms.max(1));
    sqlx::query_as::<_, McpInvocationGrantRow>(
        r#"
        INSERT INTO "LiteLLM_McpInvocationGrantsTable" (
          id, owner_id, session_id, turn_id, invocation_id, server_id,
          allowed_tools, allow_all, issued_at, expires_at, metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (invocation_id, server_id) DO UPDATE SET
          allowed_tools = EXCLUDED.allowed_tools,
          allow_all = EXCLUDED.allow_all,
          expires_at = GREATEST("LiteLLM_McpInvocationGrantsTable".expires_at, EXCLUDED.expires_at),
          metadata = "LiteLLM_McpInvocationGrantsTable".metadata || EXCLUDED.metadata
        WHERE "LiteLLM_McpInvocationGrantsTable".revoked_at IS NULL
        RETURNING *
        "#,
    )
    .bind(id("mcpgrant"))
    .bind(owner_id)
    .bind(session_id)
    .bind(turn_id)
    .bind(invocation_id)
    .bind(&scope.server_id)
    .bind(json!(scope.allowed_tools))
    .bind(scope.allow_all)
    .bind(now)
    .bind(expires_at)
    .bind(&scope.metadata)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn active_for_invocation(
    pool: &PgPool,
    invocation_id: &str,
    server_id: &str,
) -> Result<Option<McpInvocationGrantRow>, GatewayError> {
    let now = now_ms();
    sqlx::query_as::<_, McpInvocationGrantRow>(
        r#"
        SELECT grant_row.*
        FROM "LiteLLM_McpInvocationGrantsTable" grant_row
        JOIN "LiteLLM_SessionTurnsTable" turn_row ON turn_row.id = grant_row.turn_id
        WHERE grant_row.invocation_id = $1
          AND grant_row.server_id = $2
          AND grant_row.revoked_at IS NULL
          AND grant_row.expires_at > $3
          AND turn_row.status IN ('queued', 'running', 'waiting_input', 'waiting_approval', 'cancelling')
        "#,
    )
    .bind(invocation_id)
    .bind(server_id)
    .bind(now)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn mark_used(pool: &PgPool, grant_id: &str) -> Result<bool, GatewayError> {
    let now = now_ms();
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_McpInvocationGrantsTable"
        SET last_used_at = $2, use_count = use_count + 1
        WHERE id = $1 AND revoked_at IS NULL AND expires_at > $2
        "#,
    )
    .bind(grant_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() == 1)
}

pub async fn revoke_for_turn(pool: &PgPool, turn_id: &str) -> Result<u64, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_McpInvocationGrantsTable"
        SET revoked_at = $2
        WHERE turn_id = $1 AND revoked_at IS NULL
        "#,
    )
    .bind(turn_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected())
}

pub async fn expire_due(pool: &PgPool, now: i64) -> Result<u64, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_McpInvocationGrantsTable"
        SET revoked_at = $1
        WHERE revoked_at IS NULL AND expires_at <= $1
        "#,
    )
    .bind(now)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected())
}

async fn grant_scopes(
    pool: &PgPool,
    agent: &registry::schema::ManagedAgentRow,
) -> Result<Vec<GrantScope>, GatewayError> {
    let toolsets = agent
        .config
        .get("tools")
        .unwrap_or(&agent.tools)
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut scopes = Vec::new();
    let server_names = agent
        .config
        .get("mcp_servers")
        .or_else(|| agent.config.get("mcpServers"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|server| server.get("name").and_then(Value::as_str));
    for server_name in server_names {
        let Some(toolset) = toolsets.iter().find(|toolset| {
            toolset.get("type").and_then(Value::as_str) == Some("mcp_toolset")
                && toolset.get("mcp_server_name").and_then(Value::as_str) == Some(server_name)
        }) else {
            continue;
        };
        let Some(server) = mcp_servers::repository::get_by_name(pool, server_name).await? else {
            continue;
        };
        let global_tools = string_set(&server.allowed_tools);
        let configured_tools = toolset
            .get("configs")
            .and_then(Value::as_array)
            .map(|configs| {
                configs
                    .iter()
                    .filter(|config| config.get("enabled").and_then(Value::as_bool) != Some(false))
                    .filter_map(|config| config.get("name").and_then(Value::as_str))
                    .map(str::to_owned)
                    .collect::<HashSet<_>>()
            });
        let default_enabled = toolset
            .pointer("/default_config/enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let (allowed_tools, allow_all) = match configured_tools {
            Some(configured) => (
                configured
                    .into_iter()
                    .filter(|name| global_tools.is_empty() || global_tools.contains(name))
                    .collect(),
                false,
            ),
            None if default_enabled && global_tools.is_empty() => (Vec::new(), true),
            None if default_enabled => (global_tools.into_iter().collect(), false),
            None => (Vec::new(), false),
        };
        scopes.push(GrantScope {
            server_id: server.server_id,
            allowed_tools,
            allow_all,
            metadata: json!({"source": "agent_mcp_toolset", "configured_name": server_name}),
        });
    }
    let platform_tools = crate::http::platform_mcps::selected_platform_mcp_ids(&agent.config);
    if !platform_tools.is_empty() {
        scopes.push(GrantScope {
            server_id: crate::http::platform_mcps::PLATFORM_MCP_SERVER_NAME.to_owned(),
            allowed_tools: platform_tools,
            allow_all: false,
            metadata: json!({"source": "platform_mcp_selection"}),
        });
    }
    Ok(scopes)
}

fn string_set(value: &Value) -> HashSet<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
