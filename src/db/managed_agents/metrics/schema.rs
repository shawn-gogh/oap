use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct AgentUsageDayRow {
    pub date: String,
    pub model_calls: i64,
    pub error_calls: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
    pub duration_ms_sum: i64,
    pub duration_samples: i64,
    pub invocations: i64,
    pub gateway_metered_invocations: i64,
    pub unmetered_invocations: i64,
}
