use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::managed_agents::{metrics, registry},
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

const DEFAULT_DAYS: i32 = 30;
const MAX_DAYS: i32 = 90;

#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    days: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct AgentMetricsResponse {
    agent_id: String,
    days: i32,
    timezone: &'static str,
    totals: UsageMetrics,
    coverage: MeteringCoverage,
    quota: AgentQuotaStatus,
    daily: Vec<DailyUsageMetrics>,
}

#[derive(Debug, Serialize)]
pub struct AgentQuotaStatus {
    config: crate::db::managed_agents::quotas::schema::AgentQuotaConfig,
    month_cost_usd: f64,
    month_remaining_usd: Option<f64>,
    month_reset_at: i64,
    active_sessions: i64,
    requests_this_minute: i64,
    minute_reset_at: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct UsageMetrics {
    model_calls: i64,
    invocations: i64,
    total_tokens: i64,
    estimated_cost_usd: f64,
    average_latency_ms: Option<f64>,
    success_rate: Option<f64>,
}

#[derive(Debug, Default, Serialize)]
pub struct MeteringCoverage {
    gateway_metered: i64,
    provider_reported: i64,
    unmetered: i64,
}

#[derive(Debug, Serialize)]
pub struct DailyUsageMetrics {
    date: String,
    model_calls: i64,
    invocations: i64,
    total_tokens: i64,
    estimated_cost_usd: f64,
    average_latency_ms: Option<f64>,
    success_rate: Option<f64>,
    coverage: MeteringCoverage,
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Query(query): Query<MetricsQuery>,
) -> Result<Json<AgentMetricsResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("agent {agent_id}")))?;
    super::assert_agent_use(&auth, &agent, pool).await?;
    let days = query.days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS);
    let rows = metrics::repository::daily(pool, &agent_id, days).await?;
    let quota = quota_status(pool, &agent).await?;
    Ok(Json(build_response(agent_id, days, rows, quota)))
}

fn build_response(
    agent_id: String,
    days: i32,
    rows: Vec<metrics::schema::AgentUsageDayRow>,
    quota: AgentQuotaStatus,
) -> AgentMetricsResponse {
    let mut totals = UsageAccumulator::default();
    let daily = rows
        .into_iter()
        .map(|row| {
            totals.add(&row);
            daily_metrics(row)
        })
        .collect();
    AgentMetricsResponse {
        agent_id,
        days,
        timezone: "UTC",
        totals: totals.metrics(),
        coverage: totals.coverage,
        quota,
        daily,
    }
}

async fn quota_status(
    pool: &sqlx::PgPool,
    agent: &crate::db::managed_agents::registry::schema::ManagedAgentRow,
) -> Result<AgentQuotaStatus, GatewayError> {
    let config = super::quota_enforcement::config(agent)?;
    let month_cost_usd =
        crate::db::managed_agents::quotas::repository::current_month_cost(pool, &agent.id).await?;
    Ok(AgentQuotaStatus {
        month_remaining_usd: config
            .budget_usd_monthly
            .map(|limit| (limit - month_cost_usd).max(0.0)),
        config,
        month_cost_usd,
        month_reset_at: super::quota_enforcement::monthly_reset_at(),
        active_sessions: crate::db::managed_agents::quotas::repository::active_sessions(
            pool, &agent.id,
        )
        .await?,
        requests_this_minute: crate::db::managed_agents::quotas::repository::rate_count(
            pool, &agent.id,
        )
        .await?,
        minute_reset_at: crate::db::managed_agents::quotas::repository::minute_reset_at(
            crate::db::managed_agents::now_ms(),
        ),
    })
}

fn daily_metrics(row: metrics::schema::AgentUsageDayRow) -> DailyUsageMetrics {
    DailyUsageMetrics {
        date: row.date,
        model_calls: row.model_calls,
        invocations: row.invocations,
        total_tokens: row.total_tokens,
        estimated_cost_usd: row.estimated_cost_usd,
        average_latency_ms: average(row.duration_ms_sum, row.duration_samples),
        success_rate: success_rate(row.model_calls, row.error_calls),
        coverage: MeteringCoverage {
            gateway_metered: row.gateway_metered_invocations,
            provider_reported: 0,
            unmetered: row.unmetered_invocations,
        },
    }
}

#[derive(Default)]
struct UsageAccumulator {
    model_calls: i64,
    error_calls: i64,
    invocations: i64,
    total_tokens: i64,
    estimated_cost_usd: f64,
    duration_ms_sum: i64,
    duration_samples: i64,
    coverage: MeteringCoverage,
}

impl UsageAccumulator {
    fn add(&mut self, row: &metrics::schema::AgentUsageDayRow) {
        self.model_calls += row.model_calls;
        self.error_calls += row.error_calls;
        self.invocations += row.invocations;
        self.total_tokens += row.total_tokens;
        self.estimated_cost_usd += row.estimated_cost_usd;
        self.duration_ms_sum += row.duration_ms_sum;
        self.duration_samples += row.duration_samples;
        self.coverage.gateway_metered += row.gateway_metered_invocations;
        self.coverage.unmetered += row.unmetered_invocations;
    }

    fn metrics(&self) -> UsageMetrics {
        UsageMetrics {
            model_calls: self.model_calls,
            invocations: self.invocations,
            total_tokens: self.total_tokens,
            estimated_cost_usd: self.estimated_cost_usd,
            average_latency_ms: average(self.duration_ms_sum, self.duration_samples),
            success_rate: success_rate(self.model_calls, self.error_calls),
        }
    }
}

fn average(total: i64, samples: i64) -> Option<f64> {
    (samples > 0).then(|| total as f64 / samples as f64)
}

fn success_rate(calls: i64, errors: i64) -> Option<f64> {
    (calls > 0).then(|| (calls - errors) as f64 / calls as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_rates_remain_unknown_instead_of_zero() {
        assert_eq!(average(0, 0), None);
        assert_eq!(success_rate(0, 0), None);
        assert_eq!(success_rate(4, 1), Some(0.75));
    }
}
