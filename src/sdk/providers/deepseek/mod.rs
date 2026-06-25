//! DeepSeek provider: serves the Anthropic Messages surface by translating to
//! DeepSeek's OpenAI-compatible Chat Completions API.

pub mod anthropic_messages;

pub use anthropic_messages::{init, transformation};
