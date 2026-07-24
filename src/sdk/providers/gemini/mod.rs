pub mod runtime;

use crate::sdk::{agents::AgentRuntime, providers::base::runtime::RuntimeAdapterBindings};

pub(crate) fn register_runtime_adapters(registry: &mut RuntimeAdapterBindings) {
    registry.register(
        AgentRuntime::GeminiAntigravity,
        runtime::RUNTIME_ID,
        runtime::GeminiAntigravityRuntime,
    );
}
