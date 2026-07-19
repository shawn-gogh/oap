use chrono::{Datelike, TimeZone, Utc};
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::OwnedMutexGuard;

use crate::{
    db::managed_agents::{
        audit, now_ms,
        quotas::{repository, schema::AgentQuotaConfig},
        registry::schema::ManagedAgentRow,
    },
    errors::GatewayError,
};

pub(crate) fn config(agent: &ManagedAgentRow) -> Result<AgentQuotaConfig, GatewayError> {
    AgentQuotaConfig::from_config(&agent.config)
}

pub(crate) async fn lock_prompt(
    state: &crate::proxy::state::AppState,
    pool: &PgPool,
    agent_id: Option<&str>,
) -> Result<Option<OwnedMutexGuard<()>>, GatewayError> {
    let Some(agent_id) = agent_id else {
        return Ok(None);
    };
    let lock = state
        .keyed_locks
        .lock(&format!("agent_prompt_quota:{agent_id}"))
        .await;
    let agent = crate::db::managed_agents::registry::repository::get(pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::UnknownAgent(agent_id.to_owned()))?;
    super::assert_agent_interactive(pool, &agent).await?;
    enforce_prompt(pool, &agent).await?;
    Ok(Some(lock))
}

pub(crate) async fn lock_session_creation_for_id(
    state: &crate::proxy::state::AppState,
    pool: &PgPool,
    agent_id: Option<&str>,
) -> Result<Option<OwnedMutexGuard<()>>, GatewayError> {
    let Some(agent_id) = agent_id else {
        return Ok(None);
    };
    let agent = crate::db::managed_agents::registry::repository::get(pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::UnknownAgent(agent_id.to_owned()))?;
    lock_session_creation(state, pool, Some(agent_id), &agent, false).await
}

pub(crate) async fn lock_session_creation(
    state: &crate::proxy::state::AppState,
    pool: &PgPool,
    agent_id: Option<&str>,
    agent: &ManagedAgentRow,
    has_prompt: bool,
) -> Result<Option<OwnedMutexGuard<()>>, GatewayError> {
    let Some(agent_id) = agent_id else {
        return Ok(None);
    };
    super::assert_agent_interactive(pool, agent).await?;
    let lock = state
        .keyed_locks
        .lock(&format!("agent_session_quota:{agent_id}"))
        .await;
    enforce_session_creation(pool, agent).await?;
    if has_prompt {
        enforce_prompt(pool, agent).await?;
    }
    Ok(Some(lock))
}

/// Budget-only enforcement for the raw model-proxy paths (`/v1/messages`,
/// `/v1/responses`). Those requests carry `x-lap-session-id` and attribute
/// their spend to an agent, so the monthly budget cap must gate here too —
/// otherwise the cap is trivially bypassed by calling the proxy directly
/// instead of going through the session/prompt flow.
///
/// Deliberately does NOT touch `rate_per_minute` or `max_concurrent_sessions`:
/// a single user turn fans out into many model calls on this path (tool-call
/// round-trips, title/summary generation), so per-call rate consumption would
/// break multi-step agents and double-count against the turn-level limit that
/// `enforce_prompt` already applies at enqueue time. The budget check is a SUM
/// over committed SpendLogs, so evaluating it on every proxy call is idempotent
/// and does not double-count.
///
/// Attribution is best-effort: an unknown/unresolvable agent id is treated as
/// "nothing to enforce" rather than an error, so this never turns a normal
/// model request into a hard failure on its own.
pub(crate) async fn enforce_attributed_budget(
    pool: &PgPool,
    agent_id: &str,
) -> Result<(), GatewayError> {
    let Some(agent) = crate::db::managed_agents::registry::repository::get(pool, agent_id).await?
    else {
        return Ok(());
    };
    let Some(limit) = config(&agent)?.budget_usd_monthly else {
        return Ok(());
    };
    let current = repository::current_month_cost(pool, &agent.id).await?;
    if current >= limit {
        return reject(
            pool,
            &agent,
            "monthly_budget",
            current,
            limit,
            monthly_reset_at(),
        )
        .await;
    }
    Ok(())
}

pub(crate) async fn enforce_prompt(
    pool: &PgPool,
    agent: &ManagedAgentRow,
) -> Result<(), GatewayError> {
    let limits = config(agent)?;
    if let Some(limit) = limits.budget_usd_monthly {
        let current = repository::current_month_cost(pool, &agent.id).await?;
        if current >= limit {
            return reject(
                pool,
                agent,
                "monthly_budget",
                current,
                limit,
                monthly_reset_at(),
            )
            .await;
        }
    }
    if let Some(limit) = limits.rate_per_minute {
        let reset_at = repository::minute_reset_at(now_ms());
        let Some(current) = repository::consume_rate(pool, &agent.id, limit).await? else {
            return reject(
                pool,
                agent,
                "rate_per_minute",
                limit as f64,
                limit as f64,
                reset_at,
            )
            .await;
        };
        tracing::debug!(agent_id = %agent.id, current, limit, "agent rate quota consumed");
    }
    Ok(())
}

pub(crate) async fn enforce_session_creation(
    pool: &PgPool,
    agent: &ManagedAgentRow,
) -> Result<(), GatewayError> {
    let Some(limit) = config(agent)?.max_concurrent_sessions else {
        return Ok(());
    };
    let current = repository::active_sessions(pool, &agent.id).await?;
    if current >= limit {
        return reject(
            pool,
            agent,
            "max_concurrent_sessions",
            current as f64,
            limit as f64,
            0,
        )
        .await;
    }
    Ok(())
}

async fn reject(
    pool: &PgPool,
    agent: &ManagedAgentRow,
    quota: &str,
    current: f64,
    limit: f64,
    reset_at: i64,
) -> Result<(), GatewayError> {
    if let Err(error) = audit::record(
        pool,
        "quota-enforcer",
        "agent.quota.rejected",
        "agent",
        &agent.id,
        json!({
            "quota": quota,
            "current": current,
            "limit": limit,
            "reset_at": reset_at,
        }),
    )
    .await
    {
        tracing::warn!(agent_id = %agent.id, %error, "failed to audit quota rejection");
    }
    let reset = if reset_at > 0 {
        format!("，重置时间 {}", timestamp(reset_at))
    } else {
        "；结束现有运行后可重试".to_owned()
    };
    Err(GatewayError::QuotaExceeded(format!(
        "智能体「{}」触发配额 {}：当前 {:.4}，上限 {:.4}{}。",
        agent.name, quota, current, limit, reset
    )))
}

pub(crate) fn monthly_reset_at() -> i64 {
    let Some(now) = chrono::DateTime::<Utc>::from_timestamp_millis(now_ms()) else {
        return 0;
    };
    let (year, month) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0)
        .single()
        .map(|value| value.timestamp_millis())
        .unwrap_or_default()
}

fn timestamp(value: i64) -> String {
    chrono::DateTime::<Utc>::from_timestamp_millis(value)
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| value.to_string())
}
