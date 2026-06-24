//! Optional OpenTelemetry tracing. When disabled, all functions are no-ops.
//! Triggered by `--trace` flag or `OTEL_EXPORTER_OTLP_ENDPOINT` env var.
//!
//! Module named `otel` (not `tracing`) to avoid shadowing the `tracing`
//! crate import in this workspace.

use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::trace::Tracer;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{self as sdktrace};

/// Initialises an OTLP gRPC tracer provider.
/// Returns a shutdown callback that must be called before exit.
pub fn init(service_name: &str, version: &str) -> anyhow::Result<impl FnOnce() + use<>> {
    let exporter = opentelemetry_otlp::SpanExporter::builder().with_tonic().build()?;
    let resource = Resource::builder_empty()
        .with_service_name(service_name.to_string())
        .with_attribute(KeyValue::new("service.version", version.to_string()))
        .build();
    let provider = sdktrace::SdkTracerProvider::builder().with_batch_exporter(exporter).with_resource(resource).build();
    global::set_tracer_provider(provider.clone());
    global::set_text_map_propagator(opentelemetry_sdk::propagation::TraceContextPropagator::new());
    Ok(move || {
        let _ = provider.shutdown();
    })
}

/// Starts a root span for a CLI command.
/// Returns a guard that ends the span when dropped.
pub fn start_command(command: &str, version: &str) -> impl Drop {
    use opentelemetry::trace::Span;
    let tracer = global::tracer("agentforge-cli");
    let mut span = tracer.start(format!("cli/{command}"));
    span.set_attribute(KeyValue::new("cli.command", command.to_string()));
    span.set_attribute(KeyValue::new("cli.version", version.to_string()));
    SpanGuard(Some(span))
}

struct SpanGuard(Option<opentelemetry::global::BoxedSpan>);

impl Drop for SpanGuard {
    fn drop(&mut self) {
        if let Some(mut s) = self.0.take() {
            use opentelemetry::trace::Span;
            s.end();
        }
    }
}

/// Injects W3C traceparent into a reqwest `HeaderMap`. No-op when tracing is
/// not active.
pub fn inject_headers(headers: &mut reqwest::header::HeaderMap) {
    use opentelemetry::propagation::Injector;
    let cx = opentelemetry::Context::current();
    struct H<'a>(&'a mut reqwest::header::HeaderMap);
    impl<'a> Injector for H<'a> {
        fn set(&mut self, key: &str, value: String) {
            if let (Ok(name), Ok(v)) = (
                reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                reqwest::header::HeaderValue::from_str(&value),
            ) {
                self.0.insert(name, v);
            }
        }
    }
    global::get_text_map_propagator(|prop| prop.inject_context(&cx, &mut H(headers)));
}

/// Returns the current trace ID or empty string if no active span.
pub fn trace_id() -> String {
    use opentelemetry::trace::TraceContextExt;
    let cx = opentelemetry::Context::current();
    let sc = cx.span().span_context().clone();
    if sc.is_valid() { sc.trace_id().to_string() } else { String::new() }
}
