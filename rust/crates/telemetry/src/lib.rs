//! Optional OpenTelemetry (OTLP) tracing for the server-side binaries.
//!
//! [`otel_layer`] returns a `tracing` layer that exports spans over OTLP gRPC
//! when `OTEL_EXPORTER_OTLP_ENDPOINT` is set, and `None` — a true no-op — when
//! it is not, so a default deployment keeps its existing JSON logging and pulls
//! in no exporter runtime cost. The exporter/provider setup mirrors the
//! operator CLI's `otel` module; this crate adds the `tracing-opentelemetry`
//! bridge so instrumented spans (`#[instrument]`, `tracing::info_span!`) are
//! exported automatically, and sets the W3C `traceparent` propagator so the
//! API -> NATS -> sidecar -> CLI hops (CN-4) share one trace.

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing::Subscriber;
use tracing_subscriber::Layer;
use tracing_subscriber::registry::LookupSpan;

/// Env var that both enables OTLP export and points at the collector endpoint.
/// This is the standard OpenTelemetry SDK variable, so operators configure it
/// the same way they would for any OTLP-aware service.
pub const OTLP_ENDPOINT_ENV: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

/// True when OTLP export is requested — the endpoint env var is set and not
/// blank. Used to gate the exporter so an unconfigured deployment stays a no-op.
pub fn is_enabled() -> bool {
    std::env::var(OTLP_ENDPOINT_ENV).ok().is_some_and(|value| !value.trim().is_empty())
}

/// Build an OTLP tracing layer for `service_name`, or `None` when export is
/// disabled (endpoint env unset/blank) or the exporter cannot be constructed.
///
/// Returns the layer together with an [`OtelGuard`]: keep the guard alive for
/// the process lifetime (bind it in `main`) so the batch exporter is flushed
/// and shut down cleanly on exit. Dropping it early stops span export.
///
/// Exporter construction failing is treated as "export disabled" rather than a
/// fatal error: an observability collector being unreachable must never stop the
/// service from starting, and the JSON fmt layer keeps logging regardless.
///
/// ```ignore
/// use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};
/// let (otel, _otel_guard) = match agentforge_telemetry::otel_layer::<Registry>("agentforge-server", VERSION) {
///     Some((layer, guard)) => (Some(layer), Some(guard)),
///     None => (None, None),
/// };
/// tracing_subscriber::registry().with(env_filter).with(fmt_layer).with(otel).init();
/// ```
pub fn otel_layer<S>(service_name: &'static str, version: &str) -> Option<(Box<dyn Layer<S> + Send + Sync>, OtelGuard)>
where
    S: Subscriber + for<'span> LookupSpan<'span> + Send + Sync,
{
    if !is_enabled() {
        return None;
    }

    let exporter = match opentelemetry_otlp::SpanExporter::builder().with_tonic().build() {
        Ok(exporter) => exporter,
        Err(err) => {
            // Never fail process start-up because a collector is unreachable;
            // fall back to no export (the fmt layer still logs to stdout).
            eprintln!("agentforge-telemetry: OTLP exporter init failed, span export disabled: {err}");
            return None;
        }
    };

    let resource = Resource::builder_empty()
        .with_service_name(service_name.to_string())
        .with_attribute(KeyValue::new("service.version", version.to_string()))
        .build();

    let provider = SdkTracerProvider::builder().with_batch_exporter(exporter).with_resource(resource).build();

    // W3C `traceparent` propagation so a span started in one service continues
    // in the next hop instead of each service rooting a disconnected trace.
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
    opentelemetry::global::set_tracer_provider(provider.clone());

    let tracer = provider.tracer(service_name);
    let layer = tracing_opentelemetry::layer().with_tracer(tracer).boxed();
    Some((layer, OtelGuard(provider)))
}

/// Flushes and shuts down the tracer provider on drop. Bind it in `main` so it
/// lives for the whole process; dropping it early stops span export.
pub struct OtelGuard(SdkTracerProvider);

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(err) = self.0.shutdown() {
            eprintln!("agentforge-telemetry: tracer provider shutdown error: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::Registry;

    // Both env states are checked in one test (not two) because cargo runs
    // tests in parallel and they mutate the same process-global env var.
    #[test]
    fn export_is_disabled_and_noop_when_endpoint_is_unset_or_blank() {
        let previous = std::env::var(OTLP_ENDPOINT_ENV).ok();

        // Unset -> disabled, no layer built.
        // SAFETY: this test owns the env var for its duration and restores it.
        unsafe { std::env::remove_var(OTLP_ENDPOINT_ENV) };
        assert!(!is_enabled(), "unset endpoint must be disabled");
        assert!(otel_layer::<Registry>("agentforge-test", "0.0.0").is_none(), "unset endpoint must yield no layer");

        // Blank/whitespace -> still disabled (guards against an empty env value).
        unsafe { std::env::set_var(OTLP_ENDPOINT_ENV, "   ") };
        assert!(!is_enabled(), "blank endpoint must be disabled");
        assert!(otel_layer::<Registry>("agentforge-test", "0.0.0").is_none(), "blank endpoint must yield no layer");

        match previous {
            Some(value) => unsafe { std::env::set_var(OTLP_ENDPOINT_ENV, value) },
            None => unsafe { std::env::remove_var(OTLP_ENDPOINT_ENV) },
        }
    }
}
