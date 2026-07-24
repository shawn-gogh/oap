use std::{future::Future, pin::Pin};

use serde_json::Value;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{session_control, sessions},
    errors::GatewayError,
    http::agent_runtimes::RuntimeCredential,
    proxy::state::AppState,
};

pub(crate) type InvocationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, GatewayError>> + Send + 'a>>;

#[derive(Default)]
pub(crate) struct TraceHeaders {
    traceparent: Option<String>,
    tracestate: Option<String>,
}

impl TraceHeaders {
    pub(crate) fn from_metadata(metadata: &Value) -> Self {
        let Some((traceparent, tracestate)) = super::telemetry::trace_headers(metadata) else {
            return Self::default();
        };
        Self {
            traceparent: Some(traceparent),
            tracestate,
        }
    }

    pub(crate) fn apply(&self, mut request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(traceparent) = self.traceparent.as_deref() {
            request = request.header("traceparent", traceparent);
        }
        if let Some(tracestate) = self.tracestate.as_deref() {
            request = request.header("tracestate", tracestate);
        }
        request
    }
}

#[derive(Clone, Copy)]
pub(crate) struct InvocationContext<'a> {
    pub state: &'a AppState,
    pub pool: &'a PgPool,
    pub row: &'a sessions::schema::SessionRow,
    pub source: &'a Value,
    pub credential: &'a RuntimeCredential,
    pub input: &'a Value,
    pub prompt: &'a str,
    pub agent_name: &'a str,
    pub trace: &'a TraceHeaders,
}

#[derive(Clone, Copy)]
pub(crate) struct InvocationCancellation<'a> {
    pub state: &'a AppState,
    pub pool: &'a PgPool,
    pub row: &'a sessions::schema::SessionRow,
    pub source: &'a Value,
    pub credential: &'a RuntimeCredential,
    pub binding: &'a session_control::schema::SessionInvocationRow,
    pub trace: &'a TraceHeaders,
}

pub(crate) trait InvocationAdapter: Send + Sync {
    fn adapter_id(&self) -> &'static str;
    fn protocol_alias(&self) -> &'static str;

    fn protocol_version(&self) -> &'static str {
        "unverified"
    }

    fn invoke<'a>(&'a self, context: InvocationContext<'a>) -> InvocationFuture<'a, Option<Value>>;

    fn cancel<'a>(&'a self, _context: InvocationCancellation<'a>) -> InvocationFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn abort<'a>(&'a self, context: InvocationCancellation<'a>) -> InvocationFuture<'a, ()> {
        self.cancel(context)
    }
}
