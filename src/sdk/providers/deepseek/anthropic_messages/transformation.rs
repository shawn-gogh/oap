use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use serde_json::Value;

use crate::{
    errors::GatewayError,
    sdk::{
        providers::base::{ProviderRequest, Transformation},
        routing::Deployment,
    },
};

use super::{chat_completion, responses, translation};

/// DeepSeek speaks the OpenAI Chat Completions API. This adapter serves both of
/// the gateway's inbound surfaces from that single upstream:
/// - `/v1/messages` (Anthropic Messages) — `transform_messages_*`
/// - `/v1/responses` (OpenAI Responses, used by opencode/Codex) — `transform_request`,
///   `responses_url`, `transform_responses_response_body`
#[derive(Debug, Default, Clone)]
pub struct DeepSeekChatTransformation;

impl DeepSeekChatTransformation {
    fn chat_completions_url(&self, deployment: &Deployment) -> String {
        format!(
            "{}/v1/chat/completions",
            deployment.api_base.trim_end_matches('/')
        )
    }

    fn auth_headers(&self, deployment: &Deployment) -> Result<HeaderMap, GatewayError> {
        let mut headers = HeaderMap::new();
        let bearer = format!("Bearer {}", deployment.api_key);
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&bearer)
                .map_err(|_| GatewayError::InvalidConfig("invalid api_key".to_owned()))?,
        );
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Ok(headers)
    }

    fn content_type_headers(&self, stream: bool) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let content_type = if stream {
            HeaderValue::from_static("text/event-stream")
        } else {
            HeaderValue::from_static("application/json")
        };
        headers.insert(header::CONTENT_TYPE, content_type);
        headers
    }
}

impl DeepSeekChatTransformation {
    fn chat_completions_request(
        &self,
        translated: Value,
        deployment: &Deployment,
    ) -> Result<ProviderRequest, GatewayError> {
        let stream = translated
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(ProviderRequest {
            body: serde_json::to_vec(&translated)?,
            headers: self.auth_headers(deployment)?,
            stream,
        })
    }
}

impl Transformation for DeepSeekChatTransformation {
    // ---- OpenAI Responses inbound surface (/v1/responses) ----

    fn transform_request(
        &self,
        body: Value,
        deployment: &Deployment,
        _inbound_headers: &HeaderMap,
    ) -> Result<ProviderRequest, GatewayError> {
        let translated = responses::openai_responses_to_chat_completions(body, deployment);
        self.chat_completions_request(translated, deployment)
    }

    fn transform_response_headers(&self, _upstream: &HeaderMap, stream: bool) -> HeaderMap {
        self.content_type_headers(stream)
    }

    fn responses_url(&self, deployment: &Deployment) -> String {
        self.chat_completions_url(deployment)
    }

    fn transforms_responses_response_body(&self) -> bool {
        true
    }

    fn transform_responses_response_body(
        &self,
        body: Vec<u8>,
        status: StatusCode,
        stream: bool,
        deployment: &Deployment,
        content_type: Option<&str>,
    ) -> Result<Vec<u8>, GatewayError> {
        if !status.is_success() {
            return Ok(body);
        }
        let parsed = chat_completion::parse_chat_completion(&body, content_type)?;
        if stream {
            Ok(responses::chat_completion_to_openai_response_sse(&parsed, deployment).into_bytes())
        } else {
            Ok(serde_json::to_vec(
                &responses::chat_completion_to_openai_response(&parsed, deployment),
            )?)
        }
    }

    // ---- Anthropic Messages inbound surface (/v1/messages) ----

    fn messages_url(&self, deployment: &Deployment) -> String {
        self.chat_completions_url(deployment)
    }

    fn transform_messages_request(
        &self,
        body: Value,
        deployment: &Deployment,
        _inbound_headers: &HeaderMap,
    ) -> Result<ProviderRequest, GatewayError> {
        let translated = translation::anthropic_messages_to_chat_completions(body, deployment);
        self.chat_completions_request(translated, deployment)
    }

    fn transform_messages_response_headers(&self, _upstream: &HeaderMap, stream: bool) -> HeaderMap {
        self.content_type_headers(stream)
    }

    fn transforms_messages_response_body(&self) -> bool {
        true
    }

    fn transform_messages_response_body(
        &self,
        body: Vec<u8>,
        status: StatusCode,
        stream: bool,
        deployment: &Deployment,
        content_type: Option<&str>,
    ) -> Result<Vec<u8>, GatewayError> {
        if !status.is_success() {
            return Ok(body);
        }
        let parsed = chat_completion::parse_chat_completion(&body, content_type)?;
        if stream {
            Ok(translation::chat_completion_to_anthropic_sse(&parsed, deployment).into_bytes())
        } else {
            Ok(serde_json::to_vec(
                &translation::chat_completion_to_anthropic_message(&parsed, deployment),
            )?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn deployment() -> Deployment {
        Deployment {
            provider_id: "deepseek".to_owned(),
            upstream_model: "deepseek-chat".to_owned(),
            api_base: "https://api.deepseek.com".to_owned(),
            api_key: "sk-upstream".to_owned(),
        }
    }

    #[test]
    fn messages_url_targets_chat_completions() {
        assert_eq!(
            DeepSeekChatTransformation.messages_url(&deployment()),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    #[test]
    fn messages_request_rewrites_model_and_sets_bearer() {
        let req = DeepSeekChatTransformation
            .transform_messages_request(
                json!({ "model": "deepseek-chat", "messages": [{ "role": "user", "content": "hi" }] }),
                &deployment(),
                &HeaderMap::new(),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(body["model"], "deepseek-chat");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert_eq!(
            req.headers.get(header::AUTHORIZATION).unwrap(),
            "Bearer sk-upstream"
        );
        assert!(!req.stream);
    }

    #[test]
    fn non_stream_response_body_becomes_anthropic_message() {
        let upstream = json!({
            "id": "chatcmpl-1",
            "model": "deepseek-chat",
            "choices": [{ "message": { "role": "assistant", "content": "hi there" }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 4, "completion_tokens": 2 }
        });
        let out = DeepSeekChatTransformation
            .transform_messages_response_body(
                upstream.to_string().into_bytes(),
                StatusCode::OK,
                false,
                &deployment(),
                Some("application/json"),
            )
            .unwrap();
        let msg: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(msg["type"], "message");
        assert_eq!(msg["content"][0]["text"], "hi there");
        assert_eq!(msg["stop_reason"], "end_turn");
    }

    #[test]
    fn error_status_passes_body_through_untouched() {
        let body = b"{\"error\":\"nope\"}".to_vec();
        let out = DeepSeekChatTransformation
            .transform_messages_response_body(
                body.clone(),
                StatusCode::BAD_REQUEST,
                false,
                &deployment(),
                Some("application/json"),
            )
            .unwrap();
        assert_eq!(out, body);
    }

    #[test]
    fn responses_inbound_translates_to_chat_completions() {
        let req = DeepSeekChatTransformation
            .transform_request(
                json!({
                    "model": "deepseek-chat",
                    "stream": true,
                    "input": [{ "role": "user", "content": [{ "type": "input_text", "text": "hi" }] }]
                }),
                &deployment(),
                &HeaderMap::new(),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(body["model"], "deepseek-chat");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert!(req.stream);
    }

    #[test]
    fn responses_url_targets_chat_completions() {
        assert_eq!(
            DeepSeekChatTransformation.responses_url(&deployment()),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    #[test]
    fn responses_body_becomes_openai_response() {
        let upstream = json!({
            "id": "chatcmpl-1",
            "model": "deepseek-chat",
            "choices": [{ "message": { "role": "assistant", "content": "hi there" }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 4, "completion_tokens": 2 }
        });
        let out = DeepSeekChatTransformation
            .transform_responses_response_body(
                upstream.to_string().into_bytes(),
                StatusCode::OK,
                false,
                &deployment(),
                Some("application/json"),
            )
            .unwrap();
        let resp: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(resp["object"], "response");
        assert_eq!(resp["output"][0]["content"][0]["text"], "hi there");
    }
}
