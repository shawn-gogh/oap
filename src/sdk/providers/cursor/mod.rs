pub(crate) mod list_model;
pub mod runtime;

use crate::sdk::{
    agents::AgentRuntime,
    providers::base::{models::ModelEndpointRegistry, runtime::RuntimeAdapterBindings},
};

pub(crate) fn register_runtime_adapters(registry: &mut RuntimeAdapterBindings) {
    registry.register(
        AgentRuntime::Cursor,
        runtime::RUNTIME_ID,
        runtime::CursorRuntime,
    );
}

pub(crate) fn register_model_endpoints(registry: &mut ModelEndpointRegistry) {
    registry.register(
        AgentRuntime::Cursor,
        runtime::RUNTIME_ID,
        list_model::CursorModels,
    );
}
