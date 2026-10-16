//! Structured logging and metrics for taut services.
//!
//! [`init`] installs, once per process, a `tracing` subscriber and a Prometheus recorder.
//! In JSON mode every log line is one object with the fields fixed by the project:
//! `ts`, `level`, `service`, `target`, `span`, `msg`, plus every field recorded on the
//! enclosing spans (`trace_id`, `tenant_id`, `monitor_id`, ...). That is what makes a
//! single `trace_id` search in Kibana show a check travelling through all services.

use std::fmt;
use std::sync::{Mutex, PoisonError};

use chrono::{SecondsFormat, Utc};
use metrics_exporter_prometheus::{BuildError, Matcher, PrometheusBuilder, PrometheusHandle};
use serde_json::{Map, Value};
use taut_config::{LogFormat, TelemetrySettings};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::{SubscriberInitExt, TryInitError};

/// Histogram buckets for `http_request_duration_seconds`, in seconds.
const LATENCY_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Failure while installing the subscriber or the metrics recorder.
#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    /// A `tracing` subscriber was already installed by someone else.
    #[error("failed to install tracing subscriber: {0}")]
    Tracing(#[from] TryInitError),
    /// The Prometheus recorder could not be built or installed.
    #[error("failed to install metrics recorder: {0}")]
    Metrics(#[from] BuildError),
}

/// Handle to the installed telemetry. Cloning is cheap.
#[derive(Clone)]
pub struct Telemetry {
    metrics: PrometheusHandle,
}

impl Telemetry {
    /// Handle used by the `/metrics` endpoint.
    pub fn metrics(&self) -> &PrometheusHandle {
        &self.metrics
    }

    /// Renders all registered metrics in the Prometheus text format.
    pub fn render_metrics(&self) -> String {
        self.metrics.render()
    }
}

static INSTALLED: Mutex<Option<Telemetry>> = Mutex::new(None);

/// Installs logging and metrics for `service`.
///
/// The first call in a process wins; later calls return the same handle so test binaries
/// that run several tests in one process keep working.
pub fn init(
    service: &'static str,
    settings: &TelemetrySettings,
) -> Result<Telemetry, TelemetryError> {
    let mut installed = INSTALLED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(telemetry) = installed.as_ref() {
        return Ok(telemetry.clone());
    }
    let telemetry = install(service, settings)?;
    *installed = Some(telemetry.clone());
    Ok(telemetry)
}

fn install(
    service: &'static str,
    settings: &TelemetrySettings,
) -> Result<Telemetry, TelemetryError> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(SpanFieldsLayer);
    match settings.log_format {
        LogFormat::Json => {
            registry
                .with(tracing_subscriber::fmt::layer().event_format(JsonFormat { service }))
                .try_init()?;
        }
        LogFormat::Pretty => registry
            .with(tracing_subscriber::fmt::layer().with_target(false))
            .try_init()?,
    }

    let metrics = PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("http_request_duration_seconds".to_owned()),
            LATENCY_BUCKETS,
        )?
        .install_recorder()?;

    Ok(Telemetry { metrics })
}

/// Span fields captured as JSON so the event formatter can attach them to every line.
#[derive(Default)]
struct SpanFields(Map<String, Value>);

/// Layer that records span fields into [`SpanFields`] when spans are created or updated.
struct SpanFieldsLayer;

impl<S> Layer<S> for SpanFieldsLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let mut fields = SpanFields::default();
        attrs.record(&mut JsonVisitor(&mut fields.0));
        span.extensions_mut().insert(fields);
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let mut extensions = span.extensions_mut();
        if let Some(fields) = extensions.get_mut::<SpanFields>() {
            values.record(&mut JsonVisitor(&mut fields.0));
        }
    }
}

/// Collects `tracing` fields into a JSON map.
struct JsonVisitor<'a>(&'a mut Map<String, Value>);

impl Visit for JsonVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0
            .insert(field.name().to_owned(), Value::from(format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.0
            .insert(field.name().to_owned(), Value::from(value.to_string()));
    }
}

/// Event formatter that writes one JSON object per line.
struct JsonFormat {
    service: &'static str,
}

impl<S, N> FormatEvent<S, N> for JsonFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let mut out = Map::new();
        out.insert(
            "ts".to_owned(),
            Value::from(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)),
        );
        out.insert(
            "level".to_owned(),
            Value::from(meta.level().as_str().to_ascii_lowercase()),
        );
        out.insert("service".to_owned(), Value::from(self.service));
        out.insert("target".to_owned(), Value::from(meta.target()));

        if let Some(scope) = ctx.event_scope() {
            let mut innermost = None;
            // Outermost first, so a field set by an inner span overrides the outer value.
            for span in scope.from_root() {
                if let Some(fields) = span.extensions().get::<SpanFields>() {
                    for (key, value) in &fields.0 {
                        out.insert(key.clone(), value.clone());
                    }
                }
                innermost = Some(span.name());
            }
            if let Some(name) = innermost {
                out.insert("span".to_owned(), Value::from(name));
            }
        }

        let mut fields = Map::new();
        event.record(&mut JsonVisitor(&mut fields));
        if let Some(message) = fields.remove("message") {
            out.insert("msg".to_owned(), message);
        }
        out.extend(fields);

        writeln!(writer, "{}", Value::Object(out))
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::Arc;

    use super::*;

    #[test]
    fn init_is_idempotent() {
        let settings = TelemetrySettings {
            log_format: LogFormat::Json,
        };
        let first = init("taut-test", &settings).expect("first init succeeds");
        let second = init("taut-test", &settings).expect("second init reuses the first");
        assert_eq!(first.render_metrics(), second.render_metrics());
    }

    /// Writer that appends to a shared buffer so the test can read what was logged.
    struct BufferWriter(Arc<Mutex<Vec<u8>>>);

    impl io::Write for BufferWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn json_lines_carry_span_fields_and_message() {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&buffer);
        let subscriber = tracing_subscriber::registry().with(SpanFieldsLayer).with(
            tracing_subscriber::fmt::layer()
                .event_format(JsonFormat {
                    service: "taut-test",
                })
                .with_writer(move || BufferWriter(Arc::clone(&sink))),
        );

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("request", trace_id = "01ARZ3NDEKTSV4RRFFQ69G5FAV");
            let _guard = span.enter();
            tracing::info!(count = 3_u64, ok = true, "hello {}", "world");
        });

        let output = String::from_utf8(buffer.lock().unwrap().clone()).expect("utf-8");
        let line: Value = serde_json::from_str(output.trim()).expect("one JSON object");
        assert_eq!(line["msg"], "hello world");
        assert_eq!(line["level"], "info");
        assert_eq!(line["service"], "taut-test");
        assert_eq!(line["span"], "request");
        assert_eq!(line["trace_id"], "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        assert_eq!(line["count"], 3);
        assert_eq!(line["ok"], true);
        assert!(line["ts"].is_string());
        assert_eq!(line["target"], "taut_telemetry::tests");
    }
}
