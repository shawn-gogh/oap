use serde_json::Value;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{registry, sessions::schema::SessionRow},
    errors::GatewayError,
    managed_agents::adapters::types::{InteractionProfileV1, PrimarySurface},
};

pub struct RuntimeContractSnapshot {
    pub input_schema: Value,
    pub output_schema: Value,
    pub interaction_profile: Value,
}

pub async fn resolve(
    pool: &PgPool,
    session: &SessionRow,
    structured_input: bool,
) -> Result<RuntimeContractSnapshot, GatewayError> {
    let config = if let Some(agent_id) = session.agent_id.as_deref() {
        registry::repository::get(pool, agent_id)
            .await?
            .map(|agent| agent.config)
    } else {
        None
    };
    Ok(snapshot(config.as_ref(), structured_input))
}

fn snapshot(config: Option<&Value>, structured_input: bool) -> RuntimeContractSnapshot {
    let mut profile = config
        .and_then(|config| config.get("interaction_profile"))
        .cloned()
        .and_then(|value| serde_json::from_value::<InteractionProfileV1>(value).ok())
        .unwrap_or_default();

    // A confirmed runtime mapping is newer evidence than the discovery-time
    // profile. This is especially important for LangGraph, whose real schemas
    // are fetched only when the operator confirms execution mapping.
    if let Some(mapping) = config
        .and_then(|config| config.pointer("/source/raw/x-lap-runtime"))
        .and_then(Value::as_object)
    {
        if let Some(schema) = mapping.get("input_schema") {
            profile.input_schema = schema.clone();
        }
        if let Some(schema) = mapping.get("output_schema") {
            profile.output_schema = schema.clone();
        }
    }

    profile.primary_surface = if structured_input {
        PrimarySurface::Run
    } else {
        PrimarySurface::Conversation
    };
    let input_schema = profile.input_schema.clone();
    let output_schema = profile.output_schema.clone();
    let interaction_profile =
        serde_json::to_value(profile).expect("InteractionProfileV1 must always serialize to JSON");
    RuntimeContractSnapshot {
        input_schema,
        output_schema,
        interaction_profile,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::snapshot;

    #[test]
    fn uses_persisted_interaction_profile() {
        let snapshot = snapshot(
            Some(&json!({
                "interaction_profile": {
                    "execution_mode": "blocking",
                    "input_schema": {"type": "object", "required": ["topic"]},
                    "output_schema": {"type": "string"},
                    "progress_mode": "steps",
                    "supports_retry": true
                }
            })),
            true,
        );

        assert_eq!(snapshot.input_schema["required"][0], "topic");
        assert_eq!(snapshot.output_schema["type"], "string");
        assert_eq!(snapshot.interaction_profile["primary_surface"], "run");
        assert_eq!(snapshot.interaction_profile["execution_mode"], "blocking");
    }

    #[test]
    fn confirmed_mapping_schemas_override_discovery_snapshot() {
        let snapshot = snapshot(
            Some(&json!({
                "interaction_profile": {
                    "input_schema": {"type": "object"},
                    "output_schema": {}
                },
                "source": {"raw": {"x-lap-runtime": {
                    "input_schema": {"type": "object", "required": ["messages"]},
                    "output_schema": {"type": "object", "required": ["answer"]}
                }}}
            })),
            false,
        );

        assert_eq!(snapshot.input_schema["required"][0], "messages");
        assert_eq!(snapshot.output_schema["required"][0], "answer");
        assert_eq!(
            snapshot.interaction_profile["primary_surface"],
            "conversation"
        );
    }
}
