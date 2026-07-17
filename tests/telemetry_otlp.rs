use std::collections::HashMap;

use litellm_rust::managed_agents::adapters::{
    telemetry::{OpenTelemetryAdapter, TelemetryRuntime},
    types::{InvocationState, InvocationStatus, TelemetryContext},
    TelemetryAdapter,
};
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exports_invocation_trace_and_metrics_over_otlp_http() {
    let collector = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(Vec::new()))
        .mount(&collector)
        .await;
    std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", collector.uri());
    std::env::set_var("OTEL_EXPORTER_OTLP_PROTOCOL", "http/protobuf");
    std::env::set_var("OTEL_METRIC_EXPORT_INTERVAL", "50");
    std::env::set_var("OTEL_BSP_SCHEDULE_DELAY", "50");

    let runtime = TelemetryRuntime::initialize().unwrap();
    assert!(runtime.export_enabled);
    let adapter = OpenTelemetryAdapter::default();
    let context = TelemetryContext {
        trace_id: String::new(),
        span_id: String::new(),
        traceparent: String::new(),
        tracestate: None,
        parent_traceparent: Some(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_owned(),
        ),
        parent_tracestate: None,
        session_id: "session-otel-test".to_owned(),
        turn_id: "turn-otel-test".to_owned(),
        invocation_id: "invocation-otel-test".to_owned(),
        adapter_id: "a2a_v1".to_owned(),
        protocol: "a2a".to_owned(),
        remote_correlation_id: Some("remote-task-test".to_owned()),
        started_at: litellm_rust::db::managed_agents::now_ms(),
        attributes: HashMap::new(),
    };
    let started = adapter.invocation_started(&context).await.unwrap();
    adapter
        .invocation_finished(
            &started,
            &InvocationState {
                status: InvocationStatus::Completed,
                resume_cursor: None,
                error: None,
            },
        )
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    runtime.shutdown();

    let requests = collector.received_requests().await.unwrap();
    assert!(requests
        .iter()
        .any(|request| request.url.path() == "/v1/traces"));
    assert!(requests
        .iter()
        .any(|request| request.url.path() == "/v1/metrics"));

    std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
    std::env::remove_var("OTEL_EXPORTER_OTLP_PROTOCOL");
    std::env::remove_var("OTEL_METRIC_EXPORT_INTERVAL");
    std::env::remove_var("OTEL_BSP_SCHEDULE_DELAY");
}
