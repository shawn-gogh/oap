use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use opentelemetry::{
    global,
    metrics::{Counter, Histogram, UpDownCounter},
    propagation::TextMapPropagator,
    trace::{Span, SpanKind, Status, TraceContextExt, Tracer},
    Context, KeyValue,
};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use sqlx::PgPool;

use super::{
    types::{InvocationState, InvocationStatus, TelemetryContext},
    AdapterError, AdapterFuture, TelemetryAdapter,
};
use crate::{
    db::managed_agents::session_control::schema::SessionInvocationRow, errors::GatewayError,
};

static ADAPTER: OnceLock<OpenTelemetryAdapter> = OnceLock::new();

pub struct TelemetryRuntime {
    tracer_provider: opentelemetry_sdk::trace::SdkTracerProvider,
    meter_provider: opentelemetry_sdk::metrics::SdkMeterProvider,
    pub export_enabled: bool,
}

impl TelemetryRuntime {
    pub fn initialize() -> Result<Self, AdapterError> {
        global::set_text_map_propagator(TraceContextPropagator::new());
        let service_name = std::env::var("OTEL_SERVICE_NAME")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "litellm-agent-control-plane".to_owned());
        let resource = opentelemetry_sdk::Resource::builder()
            .with_service_name(service_name)
            .build();
        let (trace_export_enabled, metric_export_enabled) = configured_signals();
        let export_enabled = trace_export_enabled || metric_export_enabled;

        let tracer_provider = if trace_export_enabled {
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .build()
                .map_err(|error| AdapterError::InvalidConfiguration(error.to_string()))?;
            let processor = opentelemetry_sdk::trace::span_processor_with_async_runtime::BatchSpanProcessor::builder(
                exporter,
                opentelemetry_sdk::runtime::Tokio,
            )
            .build();
            opentelemetry_sdk::trace::SdkTracerProvider::builder()
                .with_resource(resource.clone())
                .with_span_processor(processor)
                .build()
        } else {
            opentelemetry_sdk::trace::SdkTracerProvider::builder()
                .with_resource(resource.clone())
                .build()
        };
        global::set_tracer_provider(tracer_provider.clone());

        let meter_provider = if metric_export_enabled {
            let exporter = opentelemetry_otlp::MetricExporter::builder()
                .with_http()
                .build()
                .map_err(|error| AdapterError::InvalidConfiguration(error.to_string()))?;
            let reader = opentelemetry_sdk::metrics::periodic_reader_with_async_runtime::PeriodicReader::builder(
                exporter,
                opentelemetry_sdk::runtime::Tokio,
            )
            .build();
            opentelemetry_sdk::metrics::SdkMeterProvider::builder()
                .with_resource(resource)
                .with_reader(reader)
                .build()
        } else {
            opentelemetry_sdk::metrics::SdkMeterProvider::builder()
                .with_resource(resource)
                .build()
        };
        global::set_meter_provider(meter_provider.clone());

        Ok(Self {
            tracer_provider,
            meter_provider,
            export_enabled,
        })
    }

    pub fn shutdown(&self) {
        if let Err(error) = self.meter_provider.shutdown() {
            tracing::warn!(%error, "OpenTelemetry metric shutdown failed");
        }
        if let Err(error) = self.tracer_provider.shutdown() {
            tracing::warn!(%error, "OpenTelemetry trace shutdown failed");
        }
    }
}

struct ActiveInvocation {
    span: opentelemetry::global::BoxedSpan,
}

pub struct OpenTelemetryAdapter {
    active: Mutex<HashMap<String, ActiveInvocation>>,
    started: Counter<u64>,
    finished: Counter<u64>,
    active_count: UpDownCounter<i64>,
    duration_seconds: Histogram<f64>,
}

impl Default for OpenTelemetryAdapter {
    fn default() -> Self {
        let meter = global::meter("lap.agent-control-plane");
        Self {
            active: Mutex::new(HashMap::new()),
            started: meter
                .u64_counter("lap.agent.invocation.started")
                .with_description("Managed agent Invocations started")
                .build(),
            finished: meter
                .u64_counter("lap.agent.invocation.finished")
                .with_description("Managed agent Invocations reaching a terminal state")
                .build(),
            active_count: meter
                .i64_up_down_counter("lap.agent.invocation.active")
                .with_description("Managed agent Invocations currently active")
                .build(),
            duration_seconds: meter
                .f64_histogram("lap.agent.invocation.duration")
                .with_unit("s")
                .with_description("Managed agent Invocation terminal latency")
                .build(),
        }
    }
}

impl TelemetryAdapter for OpenTelemetryAdapter {
    fn invocation_started<'a>(
        &'a self,
        context: &'a TelemetryContext,
    ) -> AdapterFuture<'a, TelemetryContext> {
        Box::pin(async move {
            let parent = extract_parent(
                context.parent_traceparent.as_deref(),
                context.parent_tracestate.as_deref(),
            );
            let tracer = global::tracer("lap.agent-control-plane");
            let mut span = tracer
                .span_builder("agent.invocation")
                .with_kind(SpanKind::Client)
                .with_attributes(span_attributes(context))
                .start_with_context(&tracer, &parent);
            let span_context = span.span_context().clone();
            let parent_span = parent.span();
            let parent_context = parent_span.span_context();
            let generated_child = span_context.is_valid()
                && (!parent_context.is_valid()
                    || span_context.span_id() != parent_context.span_id());
            let (trace_id, span_id, traceparent, tracestate) = if generated_child {
                let trace_state = span_context.trace_state().header();
                (
                    span_context.trace_id().to_string(),
                    span_context.span_id().to_string(),
                    format!(
                        "00-{}-{}-{:02x}",
                        span_context.trace_id(),
                        span_context.span_id(),
                        span_context.trace_flags().to_u8()
                    ),
                    (!trace_state.is_empty()).then_some(trace_state),
                )
            } else {
                fallback_context(&parent)
            };
            span.set_attribute(KeyValue::new("lap.trace_id", trace_id.clone()));
            let mut active = self
                .active
                .lock()
                .map_err(|_| AdapterError::Storage("telemetry span registry lock".to_owned()))?;
            active.insert(context.invocation_id.clone(), ActiveInvocation { span });
            drop(active);

            let attributes = metric_attributes(context, None);
            self.started.add(1, &attributes);
            self.active_count.add(1, &attributes);
            let mut started = context.clone();
            started.trace_id = trace_id;
            started.span_id = span_id;
            started.traceparent = traceparent;
            started.tracestate = tracestate;
            Ok(started)
        })
    }

    fn invocation_finished<'a>(
        &'a self,
        context: &'a TelemetryContext,
        state: &'a InvocationState,
    ) -> AdapterFuture<'a, ()> {
        Box::pin(async move {
            let status = invocation_status(state.status);
            let mut active = self
                .active
                .lock()
                .map_err(|_| AdapterError::Storage("telemetry span registry lock".to_owned()))?;
            let was_active = if let Some(mut active_span) = active.remove(&context.invocation_id) {
                active_span
                    .span
                    .set_attribute(KeyValue::new("lap.invocation.status", status));
                if let Some(remote_id) = context.remote_correlation_id.as_deref() {
                    active_span.span.set_attribute(KeyValue::new(
                        "lap.remote.correlation_id",
                        remote_id.to_owned(),
                    ));
                }
                match state.status {
                    InvocationStatus::Completed => active_span.span.set_status(Status::Ok),
                    _ => active_span.span.set_status(Status::error(
                        state
                            .error
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| status.to_owned()),
                    )),
                }
                active_span.span.end();
                true
            } else {
                false
            };
            drop(active);

            let attributes = metric_attributes(context, Some(status));
            self.finished.add(1, &attributes);
            if was_active {
                self.active_count.add(-1, &metric_attributes(context, None));
            }
            let duration = crate::db::managed_agents::now_ms()
                .saturating_sub(context.started_at)
                .max(0) as f64
                / 1000.0;
            self.duration_seconds.record(duration, &attributes);
            Ok(())
        })
    }
}

pub async fn start_invocation(
    pool: &PgPool,
    invocation: &mut SessionInvocationRow,
    parent_traceparent: Option<&str>,
    parent_tracestate: Option<&str>,
) -> Result<(), GatewayError> {
    let context = TelemetryContext {
        trace_id: String::new(),
        span_id: String::new(),
        traceparent: String::new(),
        tracestate: None,
        parent_traceparent: parent_traceparent.map(str::to_owned),
        parent_tracestate: parent_tracestate.map(str::to_owned),
        session_id: invocation.session_id.clone(),
        turn_id: invocation.turn_id.clone(),
        invocation_id: invocation.id.clone(),
        adapter_id: invocation.adapter_id.clone(),
        protocol: invocation.protocol.clone(),
        remote_correlation_id: remote_correlation_id(invocation),
        started_at: crate::db::managed_agents::now_ms(),
        attributes: HashMap::from([
            (
                "lap.protocol.version".to_owned(),
                invocation.protocol_version.clone(),
            ),
            ("lap.invocation.role".to_owned(), invocation.role.clone()),
        ]),
    };
    let started = ADAPTER
        .get_or_init(OpenTelemetryAdapter::default)
        .invocation_started(&context)
        .await
        .map_err(adapter_error)?;
    let telemetry = serde_json::to_value(&started)?;
    let update = sqlx::query(
        r#"
        UPDATE "LiteLLM_SessionInvocationsTable"
        SET metadata = jsonb_set(metadata, '{telemetry}', $2, true), updated_at = $3
        WHERE id = $1
        "#,
    )
    .bind(&invocation.id)
    .bind(&telemetry)
    .bind(crate::db::managed_agents::now_ms())
    .execute(pool)
    .await;
    if let Err(error) = update {
        let _ = ADAPTER
            .get_or_init(OpenTelemetryAdapter::default)
            .invocation_finished(
                &started,
                &InvocationState {
                    status: InvocationStatus::Failed,
                    resume_cursor: None,
                    error: Some(serde_json::json!({"code": "telemetry_persistence_failed"})),
                },
            )
            .await;
        return Err(GatewayError::Database(error));
    }
    invocation.metadata["telemetry"] = telemetry;
    Ok(())
}

pub async fn finish_turn(
    pool: &PgPool,
    turn_id: &str,
    status: &str,
    error: Option<&serde_json::Value>,
) -> Result<(), GatewayError> {
    let invocations = sqlx::query_as::<_, SessionInvocationRow>(
        r#"SELECT * FROM "LiteLLM_SessionInvocationsTable" WHERE turn_id = $1"#,
    )
    .bind(turn_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)?;
    let status = parse_status(status)?;
    for invocation in invocations {
        let Some(value) = invocation.metadata.get("telemetry") else {
            continue;
        };
        let mut context = serde_json::from_value::<TelemetryContext>(value.clone())?;
        context.remote_correlation_id = remote_correlation_id(&invocation);
        ADAPTER
            .get_or_init(OpenTelemetryAdapter::default)
            .invocation_finished(
                &context,
                &InvocationState {
                    status,
                    resume_cursor: invocation.resume_cursor.clone(),
                    error: error.cloned().or(invocation.error_json.clone()),
                },
            )
            .await
            .map_err(adapter_error)?;
    }
    Ok(())
}

pub fn trace_headers(metadata: &serde_json::Value) -> Option<(String, Option<String>)> {
    let telemetry = metadata.get("telemetry")?;
    let traceparent = telemetry.get("traceparent")?.as_str()?.to_owned();
    let tracestate = telemetry
        .get("tracestate")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Some((traceparent, tracestate))
}

fn remote_correlation_id(invocation: &SessionInvocationRow) -> Option<String> {
    invocation
        .remote_task_id
        .clone()
        .or_else(|| invocation.remote_context_id.clone())
        .or_else(|| invocation.remote_session_id.clone())
        .or_else(|| invocation.remote_agent_id.clone())
}

fn parse_status(status: &str) -> Result<InvocationStatus, GatewayError> {
    match status {
        "queued" => Ok(InvocationStatus::Queued),
        "running" => Ok(InvocationStatus::Running),
        "waiting_input" => Ok(InvocationStatus::WaitingInput),
        "waiting_approval" => Ok(InvocationStatus::WaitingApproval),
        "cancelling" => Ok(InvocationStatus::Cancelling),
        "completed" => Ok(InvocationStatus::Completed),
        "failed" => Ok(InvocationStatus::Failed),
        "rejected" => Ok(InvocationStatus::Rejected),
        "cancelled" => Ok(InvocationStatus::Cancelled),
        "timed_out" => Ok(InvocationStatus::TimedOut),
        _ => Err(GatewayError::BadRequest(format!(
            "invalid telemetry invocation status {status}"
        ))),
    }
}

fn adapter_error(error: AdapterError) -> GatewayError {
    GatewayError::SandboxError(error.to_string())
}

fn extract_parent(traceparent: Option<&str>, tracestate: Option<&str>) -> Context {
    let Some(traceparent) = traceparent.map(str::trim).filter(|value| !value.is_empty()) else {
        return Context::new();
    };
    let mut carrier = HashMap::from([("traceparent".to_owned(), traceparent.to_owned())]);
    if let Some(tracestate) = tracestate.map(str::trim).filter(|value| !value.is_empty()) {
        carrier.insert("tracestate".to_owned(), tracestate.to_owned());
    }
    TraceContextPropagator::new().extract(&carrier)
}

fn span_attributes(context: &TelemetryContext) -> Vec<KeyValue> {
    let mut attributes = vec![
        KeyValue::new("lap.session.id", context.session_id.clone()),
        KeyValue::new("lap.turn.id", context.turn_id.clone()),
        KeyValue::new("lap.invocation.id", context.invocation_id.clone()),
        KeyValue::new("lap.adapter.id", context.adapter_id.clone()),
        KeyValue::new("lap.protocol", context.protocol.clone()),
    ];
    attributes.extend(
        context
            .attributes
            .iter()
            .map(|(key, value)| KeyValue::new(key.clone(), value.clone())),
    );
    attributes
}

fn metric_attributes(context: &TelemetryContext, status: Option<&str>) -> Vec<KeyValue> {
    let mut attributes = vec![
        KeyValue::new("lap.adapter.id", context.adapter_id.clone()),
        KeyValue::new("lap.protocol", context.protocol.clone()),
    ];
    if let Some(status) = status {
        attributes.push(KeyValue::new("lap.invocation.status", status.to_owned()));
    }
    attributes
}

fn fallback_context(parent: &Context) -> (String, String, String, Option<String>) {
    let parent_span = parent.span();
    let parent_context = parent_span.span_context();
    let trace_id = if parent_context.is_valid() {
        parent_context.trace_id().to_string()
    } else {
        uuid::Uuid::new_v4().simple().to_string()
    };
    let span_id = uuid::Uuid::new_v4().simple().to_string()[..16].to_owned();
    let flags = parent_context.trace_flags().to_u8();
    let traceparent = format!("00-{trace_id}-{span_id}-{flags:02x}");
    let tracestate = parent_context.trace_state().header();
    (
        trace_id,
        span_id,
        traceparent,
        (!tracestate.is_empty()).then_some(tracestate),
    )
}

fn invocation_status(status: InvocationStatus) -> &'static str {
    match status {
        InvocationStatus::Queued => "queued",
        InvocationStatus::Running => "running",
        InvocationStatus::WaitingInput => "waiting_input",
        InvocationStatus::WaitingApproval => "waiting_approval",
        InvocationStatus::Cancelling => "cancelling",
        InvocationStatus::Completed => "completed",
        InvocationStatus::Failed => "failed",
        InvocationStatus::Rejected => "rejected",
        InvocationStatus::Cancelled => "cancelled",
        InvocationStatus::TimedOut => "timed_out",
    }
}

fn configured_signals() -> (bool, bool) {
    if std::env::var("OTEL_SDK_DISABLED").is_ok_and(|value| value.eq_ignore_ascii_case("true")) {
        return (false, false);
    }
    let configured = |name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty());
    let common = configured("OTEL_EXPORTER_OTLP_ENDPOINT");
    (
        common || configured("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT"),
        common || configured("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT"),
    )
}

#[cfg(test)]
mod tests {
    use super::{extract_parent, fallback_context};
    use opentelemetry::trace::TraceContextExt;

    #[test]
    fn accepts_valid_w3c_parent_and_rejects_invalid_parent() {
        opentelemetry::global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );
        let valid = extract_parent(
            Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
            None,
        );
        assert!(valid.span().span_context().is_valid());
        let invalid = extract_parent(Some("not-a-traceparent"), None);
        assert!(!invalid.span().span_context().is_valid());
    }

    #[test]
    fn fallback_context_is_w3c_shaped() {
        let (trace_id, span_id, traceparent, _) = fallback_context(&opentelemetry::Context::new());
        assert_eq!(trace_id.len(), 32);
        assert_eq!(span_id.len(), 16);
        assert_eq!(traceparent.len(), 55);
    }
}
