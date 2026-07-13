use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

// OAP is self-contained: the model cost map ships baked into the binary
// (include_str! below) instead of being fetched from GitHub at startup.
static BACKUP_JSON: &str = include_str!("model_prices_backup.json");

fn deserialize_opt_u64<'de, D>(d: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => Ok(n.as_u64()),
        serde_json::Value::Null => Ok(None),
        _ => Ok(None),
    }
}

fn deserialize_opt_f64<'de, D>(d: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => Ok(n.as_f64()),
        serde_json::Value::Null => Ok(None),
        _ => Ok(None),
    }
}

fn deserialize_opt_string<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => Ok(Some(s)),
        serde_json::Value::Null => Ok(None),
        _ => Ok(None),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelInfo {
    #[serde(default, deserialize_with = "deserialize_opt_string")]
    pub litellm_provider: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_string")]
    pub mode: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_u64")]
    pub max_tokens: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_opt_u64")]
    pub max_input_tokens: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_opt_u64")]
    pub max_output_tokens: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_opt_f64")]
    pub input_cost_per_token: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_opt_f64")]
    pub output_cost_per_token: Option<f64>,
    pub supports_vision: Option<bool>,
    pub supports_function_calling: Option<bool>,
    pub supports_tool_choice: Option<bool>,
    pub supports_system_prompts: Option<bool>,
    pub supports_parallel_function_calling: Option<bool>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

pub type ModelCostMap = HashMap<String, ModelInfo>;

pub fn load() -> ModelCostMap {
    let map = load_backup();
    tracing::info!("Loaded embedded model cost map ({} entries)", map.len());
    map
}

fn load_backup() -> ModelCostMap {
    match serde_json::from_str(BACKUP_JSON) {
        Ok(map) => map,
        Err(e) => {
            tracing::error!("Failed to parse embedded backup model cost map: {e}");
            HashMap::new()
        }
    }
}
