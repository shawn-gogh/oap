use sqlx::PgPool;

use crate::{db::managed_agents::now_ms, errors::GatewayError};

pub const MCP_PROXY_BASE_URL_KEY: &str = "mcp_proxy_base_url";

pub async fn get_mcp_proxy_base_url(pool: &PgPool) -> Result<Option<String>, GatewayError> {
    get_value(pool, MCP_PROXY_BASE_URL_KEY).await
}

pub async fn set_mcp_proxy_base_url(
    pool: &PgPool,
    value: Option<&str>,
    actor: &str,
) -> Result<Option<String>, GatewayError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        delete_value(pool, MCP_PROXY_BASE_URL_KEY).await?;
        return Ok(None);
    };

    upsert_value(pool, MCP_PROXY_BASE_URL_KEY, value, actor).await?;
    Ok(Some(value.to_owned()))
}

pub const OUTBOUND_DOMAIN_WHITELIST_KEY: &str = "outbound_domain_whitelist";

pub async fn get_outbound_domain_whitelist(pool: &PgPool) -> Result<Option<String>, GatewayError> {
    get_value(pool, OUTBOUND_DOMAIN_WHITELIST_KEY).await
}

pub async fn set_outbound_domain_whitelist(
    pool: &PgPool,
    value: Option<&str>,
    actor: &str,
) -> Result<Option<String>, GatewayError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        delete_value(pool, OUTBOUND_DOMAIN_WHITELIST_KEY).await?;
        return Ok(None);
    };

    upsert_value(pool, OUTBOUND_DOMAIN_WHITELIST_KEY, value, actor).await?;
    Ok(Some(value.to_owned()))
}

pub const GUARDIAN_MODEL_KEY: &str = "guardian_model";

/// Model used for the Guardian reviewer's LLM calls. Falls back to the
/// acting session's own agent model when unset (see src/guardian/mod.rs) —
/// this override exists for deployments that want a cheaper/faster dedicated
/// judge model instead of reusing whatever model the agent itself runs.
pub async fn get_guardian_model(pool: &PgPool) -> Result<Option<String>, GatewayError> {
    get_value(pool, GUARDIAN_MODEL_KEY).await
}

pub async fn set_guardian_model(
    pool: &PgPool,
    value: Option<&str>,
    actor: &str,
) -> Result<Option<String>, GatewayError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        delete_value(pool, GUARDIAN_MODEL_KEY).await?;
        return Ok(None);
    };
    upsert_value(pool, GUARDIAN_MODEL_KEY, value, actor).await?;
    Ok(Some(value.to_owned()))
}

/// Shared by every outbound-domain enforcement point (the self-reported
/// `data_egress` approval path in tool_approvals.rs and the egress network
/// proxy) so they can never drift into inconsistent verdicts for the same
/// domain. Case-insensitive; entries are comma/space/semicolon separated;
/// `*.suffix` matches the suffix itself and any subdomain; bare `*` matches
/// everything.
pub fn match_domain_whitelist(domain: &str, whitelist: &str) -> bool {
    let domain = domain.to_lowercase();
    for entry in whitelist.split([',', ' ', ';']) {
        let entry = entry.trim().to_lowercase();
        if entry.is_empty() {
            continue;
        }
        if entry == "*" {
            return true;
        }
        if let Some(suffix) = entry.strip_prefix("*.") {
            if domain == suffix || domain.ends_with(&format!(".{suffix}")) {
                return true;
            }
        } else if domain == entry {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod whitelist_tests {
    use super::match_domain_whitelist;

    #[test]
    fn matches_exact_wildcard_and_global() {
        assert!(match_domain_whitelist("api.github.com", "api.github.com"));
        assert!(match_domain_whitelist("api.github.com", "*.github.com"));
        assert!(match_domain_whitelist("github.com", "*.github.com"));
        assert!(!match_domain_whitelist("google.com", "*.github.com"));
        assert!(match_domain_whitelist("google.com", "*"));
    }
}

async fn get_value(pool: &PgPool, key: &str) -> Result<Option<String>, GatewayError> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT value
        FROM "LiteLLM_GatewaySettingsTable"
        WHERE key = $1
        "#,
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

async fn upsert_value(
    pool: &PgPool,
    key: &str,
    value: &str,
    actor: &str,
) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        INSERT INTO "LiteLLM_GatewaySettingsTable"
          (key, value, updated_at, updated_by)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (key) DO UPDATE
        SET
          value = EXCLUDED.value,
          updated_at = EXCLUDED.updated_at,
          updated_by = EXCLUDED.updated_by
        "#,
    )
    .bind(key)
    .bind(value)
    .bind(now_ms())
    .bind(actor)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}

async fn delete_value(pool: &PgPool, key: &str) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        DELETE FROM "LiteLLM_GatewaySettingsTable"
        WHERE key = $1
        "#,
    )
    .bind(key)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}
