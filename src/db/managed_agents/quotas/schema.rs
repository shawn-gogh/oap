use serde::Serialize;
use serde_json::Value;

use crate::errors::GatewayError;

#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentQuotaConfig {
    pub budget_usd_monthly: Option<f64>,
    pub max_concurrent_sessions: Option<i64>,
    pub rate_per_minute: Option<i64>,
}

impl AgentQuotaConfig {
    pub fn from_config(config: &Value) -> Result<Self, GatewayError> {
        let parsed = Self {
            budget_usd_monthly: optional_f64(config, "budget_usd_monthly")?,
            max_concurrent_sessions: optional_i64(config, "max_concurrent_sessions")?,
            rate_per_minute: optional_i64(config, "rate_per_minute")?,
        };
        if parsed
            .budget_usd_monthly
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
            || parsed
                .max_concurrent_sessions
                .is_some_and(|value| value <= 0)
            || parsed.rate_per_minute.is_some_and(|value| value <= 0)
        {
            return Err(GatewayError::InvalidJsonMessage(
                "budget and quota limits must be positive numbers".to_owned(),
            ));
        }
        Ok(parsed)
    }
}

fn optional_f64(config: &Value, key: &str) -> Result<Option<f64>, GatewayError> {
    match config.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_f64().map(Some).ok_or_else(|| {
            GatewayError::InvalidJsonMessage(format!("{key} must be a number or null"))
        }),
    }
}

fn optional_i64(config: &Value, key: &str) -> Result<Option<i64>, GatewayError> {
    match config.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_i64().map(Some).ok_or_else(|| {
            GatewayError::InvalidJsonMessage(format!("{key} must be an integer or null"))
        }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::AgentQuotaConfig;

    #[test]
    fn parses_positive_limits_and_rejects_invalid_values() {
        let config = AgentQuotaConfig::from_config(&json!({
            "budget_usd_monthly": 12.5,
            "max_concurrent_sessions": 3,
            "rate_per_minute": 20,
        }))
        .unwrap();
        assert_eq!(config.budget_usd_monthly, Some(12.5));
        assert_eq!(config.max_concurrent_sessions, Some(3));
        assert_eq!(config.rate_per_minute, Some(20));
        assert!(AgentQuotaConfig::from_config(&json!({ "rate_per_minute": 0 })).is_err());
        assert!(AgentQuotaConfig::from_config(&json!({ "max_concurrent_sessions": 1.5 })).is_err());
    }
}
