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

use std::collections::HashMap;

use opentelemetry::KeyValue;
use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
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

/// Serialize the currently-active span context as a W3C `traceparent` string,
/// or `None` when there is no valid recording span.
///
/// Producers (e.g. the orchestrator building a `TaskAssignment`) call this to
/// stamp the current trace onto an outgoing message so the consumer can join it.
/// Returns `None` — never an all-zero `00-000...-00` header — when nothing is
/// being traced (tracing disabled, or no active span), so the caller stores
/// `None` rather than a meaningless placeholder.
///
/// The context is resolved in two steps because server code spans via the
/// `tracing` macros (tower-http request spans, `#[instrument]`), whose
/// OpenTelemetry context lives on the *tracing* span — NOT on
/// [`opentelemetry::Context::current`]. So this prefers the current tracing
/// span's context (via the `tracing-opentelemetry` bridge) and only falls back
/// to the raw OpenTelemetry current context (used by code that manages otel
/// spans directly, e.g. the CLI). Uses the globally installed propagator, which
/// is a no-op unless [`otel_layer`] activated it.
pub fn current_traceparent() -> Option<String> {
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let tracing_context = tracing::Span::current().context();
    let context = if tracing_context.span().span_context().is_valid() {
        tracing_context
    } else {
        opentelemetry::Context::current()
    };
    if !context.span().span_context().is_valid() {
        return None;
    }
    let mut carrier = HashMap::new();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&context, &mut HashMapInjector(&mut carrier));
    });
    carrier.remove("traceparent").filter(|value| !value.is_empty())
}

/// Reconstruct an OpenTelemetry [`Context`](opentelemetry::Context) from a W3C
/// `traceparent` string so a consumer can continue the producer's trace.
///
/// A malformed/empty header extracts to the root context (a fresh trace) rather
/// than panicking, so a bad or truncated value degrades gracefully to a new
/// trace instead of dropping the work. Consumers typically attach the returned
/// context (or use it as the parent of their work span) before processing.
pub fn context_from_traceparent(traceparent: &str) -> opentelemetry::Context {
    let mut carrier = HashMap::new();
    carrier.insert("traceparent".to_string(), traceparent.to_string());
    // Extract against an EXPLICIT root context, not `Context::current()`: on a
    // malformed/empty header the propagator returns its base context unchanged,
    // so using the current context would silently attach the new work to an
    // unrelated span that happens to be active on the caller's thread. A root
    // base makes a bad header degrade to a fresh (invalid) context as promised.
    let root = opentelemetry::Context::new();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract_with_context(&root, &HashMapExtractor(&carrier))
    })
}

/// Injects propagator output into a `HashMap` carrier.
struct HashMapInjector<'a>(&'a mut HashMap<String, String>);

impl Injector for HashMapInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.to_string(), value);
    }
}

/// Reads propagator input from a `HashMap` carrier.
struct HashMapExtractor<'a>(&'a HashMap<String, String>);

impl Extractor for HashMapExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(String::as_str).collect()
    }
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

    #[test]
    fn traceparent_round_trips_the_active_span_context() {
        use opentelemetry::Context;
        use opentelemetry::trace::{SpanContext, SpanId, TraceFlags, TraceId, TraceState};

        // The propagation helpers rely on the globally installed propagator; in a
        // real process `otel_layer` installs it, so install it here for the test.
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

        let span_context = SpanContext::new(
            TraceId::from_hex("0af7651916cd43dd8448eb211c80319c").unwrap(),
            SpanId::from_hex("b7ad6b7169203331").unwrap(),
            TraceFlags::SAMPLED,
            true,
            TraceState::default(),
        );
        let context = Context::new().with_remote_span_context(span_context.clone());

        // With that context active, `current_traceparent` serializes it...
        let traceparent = {
            let _guard = context.attach();
            current_traceparent().expect("a valid active span context yields a traceparent")
        };
        assert!(
            traceparent.contains("0af7651916cd43dd8448eb211c80319c"),
            "traceparent carries the trace id: {traceparent}"
        );
        assert!(traceparent.contains("b7ad6b7169203331"), "traceparent carries the span id: {traceparent}");

        // ...and `context_from_traceparent` reconstructs the same identifiers.
        let extracted = context_from_traceparent(&traceparent);
        assert_eq!(extracted.span().span_context().trace_id(), span_context.trace_id());
        assert_eq!(extracted.span().span_context().span_id(), span_context.span_id());
        assert!(extracted.span().span_context().is_sampled());
    }

    #[test]
    fn current_traceparent_captures_the_active_tracing_span() {
        use tracing_subscriber::layer::SubscriberExt;

        // This is the server case: work is spanned via the `tracing` macros, and
        // the otel context lives on the tracing span (not opentelemetry::current).
        // A provider with no exporter still mints valid, sampled span contexts,
        // which is all the traceparent bridge needs.
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
        let provider = SdkTracerProvider::builder().build();
        let tracer = provider.tracer("agentforge-telemetry-test");
        let subscriber = tracing_subscriber::registry().with(tracing_opentelemetry::layer().with_tracer(tracer));

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("dispatch");
            let _entered = span.enter();
            let traceparent =
                current_traceparent().expect("an active instrumented tracing span must yield a traceparent");
            assert!(traceparent.starts_with("00-"), "W3C traceparent version prefix: {traceparent}");
            // The captured header round-trips back to a valid (joinable) context.
            assert!(context_from_traceparent(&traceparent).span().span_context().is_valid());
        });

        let _ = provider.shutdown();
    }

    #[test]
    fn no_active_span_yields_no_traceparent() {
        // The root context has no valid span, so there is nothing to propagate.
        let _guard = opentelemetry::Context::new().attach();
        assert!(current_traceparent().is_none(), "root context must not produce a traceparent");
    }

    #[test]
    fn malformed_traceparent_extracts_to_a_fresh_context_without_panicking() {
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
        let context = context_from_traceparent("this-is-not-a-valid-traceparent");
        assert!(!context.span().span_context().is_valid(), "garbage header must degrade to an invalid (fresh) context");
    }

    #[test]
    fn malformed_traceparent_does_not_inherit_an_unrelated_active_trace() {
        use opentelemetry::Context;
        use opentelemetry::trace::{SpanContext, SpanId, TraceFlags, TraceId, TraceState};

        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

        // An UNRELATED trace is active on this thread when the consumer extracts...
        let unrelated = SpanContext::new(
            TraceId::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
            SpanId::from_hex("aaaaaaaaaaaaaaaa").unwrap(),
            TraceFlags::SAMPLED,
            true,
            TraceState::default(),
        );
        let _guard = Context::new().with_remote_span_context(unrelated.clone()).attach();

        // ...a bad/empty inbound header must degrade to a FRESH context, never
        // silently attach the work to that unrelated active trace (codex P2).
        for header in ["", "garbage", "00-not-hex-not-hex-00"] {
            let extracted = context_from_traceparent(header);
            assert!(
                !extracted.span().span_context().is_valid(),
                "bad header {header:?} must yield an invalid (fresh) context, not inherit current"
            );
            assert_ne!(
                extracted.span().span_context().trace_id(),
                unrelated.trace_id(),
                "bad header {header:?} must not adopt the unrelated active trace id"
            );
        }
    }
}
