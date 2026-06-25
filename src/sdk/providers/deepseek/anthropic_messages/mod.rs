mod chat_completion;
mod responses;
pub mod transformation;
mod translation;

use crate::sdk::providers::base::ProviderRegistry;
use transformation::DeepSeekChatTransformation;

const DEEPSEEK_API_BASE: &str = "https://api.deepseek.com";

pub fn init(registry: &mut ProviderRegistry) {
    registry.register("deepseek", DEEPSEEK_API_BASE, DeepSeekChatTransformation);
}
