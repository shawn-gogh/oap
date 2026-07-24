use std::{collections::HashMap, sync::Arc};

use reqwest::{header, Method};
use serde::Serialize;
use serde_json::Value;

use super::{
    client_state::ClientState,
    events::{stream_events, AgentEventStream},
    resources::Beta,
    responses::{ensure_success, response_json},
    runtime_config::{configured_http_client, runtime_configs, RuntimeConfig},
    session_context::SessionContext,
    types::{AgentRuntime, AgentSdkError, LapConfig, ManagedSessionRef},
};
use crate::{
    managed_agents::adapters::registry::AgentAdapterRegistry,
    sdk::{providers, providers::base::runtime::RuntimeAdapter},
};

#[derive(Clone)]
pub struct Lap {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    runtimes: HashMap<AgentRuntime, RuntimeConfig>,
    state: Arc<ClientState>,
    elastic_default_agent_id: Option<String>,
    agent_adapters: Result<Arc<AgentAdapterRegistry>, String>,
}

impl Lap {
    pub fn new(config: LapConfig) -> Self {
        let default = config.elastic_default_agent_id.clone();
        Self::with_http(configured_http_client(), runtime_configs(config), default)
    }

    pub(crate) fn with_http_client_and_registry(
        config: LapConfig,
        http: reqwest::Client,
        agent_adapters: Arc<AgentAdapterRegistry>,
    ) -> Self {
        let default = config.elastic_default_agent_id.clone();
        Self::with_http_and_registry(http, runtime_configs(config), default, Ok(agent_adapters))
    }

    pub fn register_session(&self, session: ManagedSessionRef) -> Result<(), AgentSdkError> {
        let session_id = session.session_id.clone();
        let context = self
            .adapter(session.lap_agent_runtime)?
            .session_context(session);
        self.remember_session_context(&session_id, context)
    }

    fn with_http(
        http: reqwest::Client,
        runtimes: HashMap<AgentRuntime, RuntimeConfig>,
        elastic_default_agent_id: Option<String>,
    ) -> Self {
        Self::with_http_and_registry(
            http,
            runtimes,
            elastic_default_agent_id,
            providers::default_agent_adapter_registry(),
        )
    }

    fn with_http_and_registry(
        http: reqwest::Client,
        runtimes: HashMap<AgentRuntime, RuntimeConfig>,
        elastic_default_agent_id: Option<String>,
        agent_adapters: Result<Arc<AgentAdapterRegistry>, String>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                http,
                runtimes,
                state: ClientState::new(),
                elastic_default_agent_id,
                agent_adapters,
            }),
        }
    }

    pub fn beta(&self) -> Beta<'_> {
        Beta { client: self }
    }

    pub(crate) async fn post<T: Serialize>(
        &self,
        runtime: AgentRuntime,
        path: &str,
        body: &T,
    ) -> Result<Value, AgentSdkError> {
        let response = self
            .request(runtime, Method::POST, path)?
            .json(body)
            .send()
            .await?;
        response_json(response).await
    }

    pub(crate) async fn post_for_session<T: Serialize>(
        &self,
        runtime: AgentRuntime,
        path: &str,
        body: &T,
        session_id: &str,
    ) -> Result<Value, AgentSdkError> {
        let response = self
            .request_for_session(runtime, Method::POST, path, session_id)?
            .json(body)
            .send()
            .await?;
        response_json(response).await
    }

    pub(crate) async fn get(
        &self,
        runtime: AgentRuntime,
        path: &str,
    ) -> Result<Value, AgentSdkError> {
        let response = self.request(runtime, Method::GET, path)?.send().await?;
        response_json(response).await
    }

    pub(crate) async fn get_for_session(
        &self,
        runtime: AgentRuntime,
        path: &str,
        session_id: &str,
    ) -> Result<Value, AgentSdkError> {
        let response = self
            .request_for_session(runtime, Method::GET, path, session_id)?
            .send()
            .await?;
        response_json(response).await
    }

    pub(crate) async fn delete(
        &self,
        runtime: AgentRuntime,
        path: &str,
    ) -> Result<Value, AgentSdkError> {
        let response = self.request(runtime, Method::DELETE, path)?.send().await?;
        response_json(response).await
    }

    pub(crate) async fn stream_for_session(
        &self,
        runtime: AgentRuntime,
        path: &str,
        session_id: &str,
    ) -> Result<AgentEventStream, AgentSdkError> {
        let response = self
            .request_for_session(runtime, Method::GET, path, session_id)?
            .header(header::ACCEPT, "text/event-stream")
            .send()
            .await?;
        let stream = stream_events(ensure_success(response).await?);
        Ok(self.adapter(runtime)?.normalize_stream(stream))
    }

    pub(crate) async fn stream_post_for_session<T: Serialize>(
        &self,
        runtime: AgentRuntime,
        path: &str,
        body: &T,
        session_id: &str,
    ) -> Result<AgentEventStream, AgentSdkError> {
        let response = self
            .request_for_session(runtime, Method::POST, path, session_id)?
            .header(header::ACCEPT, "text/event-stream")
            .json(body)
            .send()
            .await?;
        let stream = stream_events(ensure_success(response).await?);
        Ok(self.adapter(runtime)?.normalize_stream(stream))
    }

    pub(crate) fn request(
        &self,
        runtime: AgentRuntime,
        method: Method,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, AgentSdkError> {
        let config = self
            .inner
            .runtimes
            .get(&runtime)
            .ok_or(AgentSdkError::RuntimeNotConfigured(runtime))?;
        let request = self
            .inner
            .http
            .request(method, format!("{}{}", config.base_url, path))
            .header(header::CONTENT_TYPE, "application/json");
        Ok(config.authorize(request))
    }

    fn request_for_session(
        &self,
        runtime: AgentRuntime,
        method: Method,
        path: &str,
        session_id: &str,
    ) -> Result<reqwest::RequestBuilder, AgentSdkError> {
        let mut request = self.request(runtime, method, path)?;
        if let Some((traceparent, tracestate)) = self.inner.state.trace_headers(session_id)? {
            request = request.header("traceparent", traceparent);
            if let Some(tracestate) = tracestate {
                request = request.header("tracestate", tracestate);
            }
        }
        Ok(request)
    }

    pub(super) fn adapter(
        &self,
        runtime: AgentRuntime,
    ) -> Result<Arc<dyn RuntimeAdapter>, AgentSdkError> {
        let registry = self
            .inner
            .agent_adapters
            .as_ref()
            .map_err(|error| AgentSdkError::InvalidRequest(error.clone()))?;
        registry
            .managed_runtime_adapter(runtime)
            .ok_or(AgentSdkError::RuntimeNotConfigured(runtime))
    }

    pub(super) fn default_runtime(&self) -> Result<AgentRuntime, AgentSdkError> {
        if self.inner.runtimes.len() == 1 {
            self.inner
                .runtimes
                .keys()
                .copied()
                .next()
                .ok_or(AgentSdkError::NoRuntimesConfigured)
        } else if self.inner.runtimes.is_empty() {
            Err(AgentSdkError::NoRuntimesConfigured)
        } else {
            Err(AgentSdkError::RuntimeRequired)
        }
    }

    pub(super) fn runtime_for_session(
        &self,
        session_id: &str,
    ) -> Result<AgentRuntime, AgentSdkError> {
        match self.inner.state.runtime_for_session(session_id)? {
            Some(runtime) => Ok(runtime),
            None => self.default_runtime(),
        }
    }

    pub(crate) fn context_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionContext>, AgentSdkError> {
        self.inner.state.context_for_session(session_id)
    }

    pub(crate) fn remember_cursor_run(
        &self,
        agent_id: &str,
        run_id: &str,
    ) -> Result<(), AgentSdkError> {
        self.inner.state.remember_cursor_run(agent_id, run_id)
    }

    pub(crate) fn cursor_run_for_agent(
        &self,
        agent_id: &str,
    ) -> Result<Option<String>, AgentSdkError> {
        self.inner.state.cursor_run_for_agent(agent_id)
    }

    pub(crate) fn elastic_default_agent_id(&self) -> Option<String> {
        self.inner.elastic_default_agent_id.clone()
    }

    pub(crate) fn remember_pending_turn(
        &self,
        session_id: &str,
        prompt: &str,
    ) -> Result<(), AgentSdkError> {
        self.inner.state.remember_pending_turn(session_id, prompt)
    }

    pub(crate) fn take_pending_turn(
        &self,
        session_id: &str,
    ) -> Result<Option<String>, AgentSdkError> {
        self.inner.state.take_pending_turn(session_id)
    }

    pub(crate) fn remember_agent_meta(
        &self,
        agent_id: &str,
        meta: serde_json::Value,
    ) -> Result<(), AgentSdkError> {
        self.inner.state.remember_agent_meta(agent_id, meta)
    }

    pub(crate) fn agent_meta(
        &self,
        agent_id: &str,
    ) -> Result<Option<serde_json::Value>, AgentSdkError> {
        self.inner.state.agent_meta(agent_id)
    }

    pub(crate) fn remember_session_context(
        &self,
        session_id: &str,
        context: SessionContext,
    ) -> Result<(), AgentSdkError> {
        self.inner
            .state
            .remember_session_context(session_id, context)
    }

    pub(crate) fn remember_trace_headers(
        &self,
        session_id: &str,
        traceparent: String,
        tracestate: Option<String>,
    ) -> Result<(), AgentSdkError> {
        self.inner
            .state
            .remember_trace_headers(session_id, traceparent, tracestate)
    }

    pub(crate) fn remember_session(
        &self,
        session_id: &str,
        runtime: AgentRuntime,
    ) -> Result<(), AgentSdkError> {
        self.remember_session_context(
            session_id,
            SessionContext {
                runtime,
                provider_session_id: Some(session_id.to_owned()),
                agent_id: None,
                run_id: None,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use reqwest::Method;

    use super::Lap;
    use crate::sdk::agents::{AgentRuntime, LapConfig};

    #[test]
    fn session_request_propagates_w3c_trace_headers() {
        let client = Lap::new(LapConfig {
            cursor_api_key: Some("test-key".to_owned()),
            cursor_base_url: "https://cursor.example".to_owned(),
            ..Default::default()
        });
        client
            .remember_trace_headers(
                "session-1",
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_owned(),
                Some("vendor=value".to_owned()),
            )
            .unwrap();
        let request = client
            .request_for_session(
                AgentRuntime::Cursor,
                Method::POST,
                "/v1/agents/a/runs",
                "session-1",
            )
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            request.headers()["traceparent"],
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        );
        assert_eq!(request.headers()["tracestate"], "vendor=value");
    }
}
