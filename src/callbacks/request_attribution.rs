use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestAttribution {
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub invocation_id: Option<String>,
    pub purpose: String,
}

impl RequestAttribution {
    pub fn api() -> Self {
        Self {
            session_id: None,
            agent_id: None,
            invocation_id: None,
            purpose: "api".to_owned(),
        }
    }
}
