/// OpenTelemetry integration for exporting traces, metrics, and logs to a LGTM stack.
///
/// When enabled, telemetry data flows via OTLP HTTP to a configurable endpoint
/// (default: `http://localhost:4318`). When disabled or if the endpoint is
/// unreachable, the app works normally — no crash, no retry spam.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::LogExporter;
use opentelemetry_otlp::MetricExporter;
use opentelemetry_otlp::SpanExporter;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

static TELEMETRY_INITIALIZED: AtomicBool = AtomicBool::new(false);
static LOGGER_PROVIDER: OnceLock<SdkLoggerProvider> = OnceLock::new();
static METER_PROVIDER: OnceLock<SdkMeterProvider> = OnceLock::new();
static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

const SERVICE_NAME: &str = "buttervoice";

fn build_resource() -> Resource {
    let version = env!("CARGO_PKG_VERSION");
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());
    Resource::builder()
        .with_attributes([
            KeyValue::new("service.name", SERVICE_NAME),
            KeyValue::new("service.version", version),
            KeyValue::new("os.type", "darwin"),
            KeyValue::new("host.name", hostname),
        ])
        .build()
}

/// Initialize the full OpenTelemetry pipeline (traces + metrics + logs).
///
/// If `enabled` is false, only a minimal `tracing` subscriber is set up for
/// stderr output (no OTel export). This is the default for users who don't
/// have a LGTM stack running.
pub fn init(enabled: bool, otlp_endpoint: &str) {
    if TELEMETRY_INITIALIZED.swap(true, Ordering::SeqCst) {
        return; // already initialized
    }

    if !enabled {
        eprintln!("[telemetry] OpenTelemetry disabled (telemetry_enabled=false)");
        init_stderr_only();
        return;
    }

    let endpoint = otlp_endpoint.trim_end_matches('/').to_string();
    let resource = build_resource();

    // ── Traces ──
    let tracer_provider = match SpanExporter::builder()
        .with_http()
        .with_endpoint(format!("{endpoint}/v1/traces"))
        .with_timeout(Duration::from_secs(5))
        .build()
    {
        Ok(exporter) => {
            let provider = SdkTracerProvider::builder()
                .with_resource(resource.clone())
                .with_batch_exporter(exporter)
                .build();
            global::set_tracer_provider(provider.clone());
            Some(provider)
        }
        Err(err) => {
            eprintln!("[telemetry] failed to create span exporter: {err}");
            None
        }
    };

    // ── Metrics ──
    let meter_provider = match MetricExporter::builder()
        .with_http()
        .with_endpoint(format!("{endpoint}/v1/metrics"))
        .with_timeout(Duration::from_secs(5))
        .build()
    {
        Ok(exporter) => {
            let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(exporter)
                .with_interval(Duration::from_secs(15))
                .build();
            let provider = SdkMeterProvider::builder()
                .with_resource(resource.clone())
                .with_reader(reader)
                .build();
            global::set_meter_provider(provider.clone());
            Some(provider)
        }
        Err(err) => {
            eprintln!("[telemetry] failed to create metric exporter: {err}");
            None
        }
    };

    // ── Logs ──
    let logger_provider = match LogExporter::builder()
        .with_http()
        .with_endpoint(format!("{endpoint}/v1/logs"))
        .with_timeout(Duration::from_secs(5))
        .build()
    {
        Ok(exporter) => {
            let provider = SdkLoggerProvider::builder()
                .with_resource(resource)
                .with_batch_exporter(exporter)
                .build();
            Some(provider)
        }
        Err(err) => {
            eprintln!("[telemetry] failed to create log exporter: {err}");
            None
        }
    };

    // ── tracing subscriber ──
    // Layers: fmt (stderr) + OpenTelemetry traces + OpenTelemetry logs
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_level(true)
        .compact();

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("buttervoice=info"));

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer);

    if let Some(ref tp) = tracer_provider {
        let otel_trace_layer = tracing_opentelemetry::layer().with_tracer(tp.tracer(SERVICE_NAME));

        if let Some(ref lp) = logger_provider {
            let otel_log_layer =
                opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(lp);
            registry.with(otel_trace_layer).with(otel_log_layer).init();
        } else {
            registry.with(otel_trace_layer).init();
        }
    } else if let Some(ref lp) = logger_provider {
        let otel_log_layer =
            opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(lp);
        registry.with(otel_log_layer).init();
    } else {
        registry.init();
    }

    // Store providers for shutdown
    if let Some(tp) = tracer_provider {
        let _ = TRACER_PROVIDER.set(tp);
    }
    if let Some(mp) = meter_provider {
        let _ = METER_PROVIDER.set(mp);
    }
    if let Some(lp) = logger_provider {
        let _ = LOGGER_PROVIDER.set(lp);
    }

    eprintln!(
        "[telemetry] OpenTelemetry initialized: traces={}, metrics={}, logs={}, endpoint={endpoint}",
        TRACER_PROVIDER.get().is_some(),
        METER_PROVIDER.get().is_some(),
        LOGGER_PROVIDER.get().is_some(),
    );
    tracing::info!(
        endpoint = %endpoint,
        "OpenTelemetry initialized"
    );
}

fn init_stderr_only() {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_level(true)
        .compact();

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("buttervoice=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}

// ── Metrics recording helpers ──

/// Record dictation metrics via the global OTel meter.
/// Called from `finish_complete_transcription` in `app_state.rs`.
pub fn record_dictation_metrics(
    total_duration_ms: u64,
    recording_duration_ms: u64,
    transcription_duration_ms: u64,
    post_process_duration_ms: u64,
    post_process_edits_applied: u32,
    post_process_edits_rejected: u32,
    cleanup_duration_ms: u64,
    succeeded: bool,
    model_id: &str,
    audio_batches_dropped: u64,
) {
    let meter = global::meter(SERVICE_NAME);

    // Dictation counter
    let dictation_counter = meter.u64_counter("dictation.total").build();
    let status = if succeeded { "success" } else { "failure" };
    dictation_counter.add(1, &[KeyValue::new("status", status)]);

    // Latency histogram
    let latency_hist = meter.f64_histogram("dictation.latency_ms").build();
    latency_hist.record(total_duration_ms as f64, &[KeyValue::new("stage", "total")]);
    latency_hist.record(
        recording_duration_ms as f64,
        &[KeyValue::new("stage", "recording")],
    );
    latency_hist.record(
        transcription_duration_ms as f64,
        &[
            KeyValue::new("stage", "transcription"),
            KeyValue::new("model_id", model_id.to_string()),
        ],
    );
    if post_process_duration_ms > 0 {
        latency_hist.record(
            post_process_duration_ms as f64,
            &[KeyValue::new("stage", "post_process")],
        );
    }
    if cleanup_duration_ms > 0 {
        latency_hist.record(
            cleanup_duration_ms as f64,
            &[KeyValue::new("stage", "llm_cleanup")],
        );
    }

    // Post-processing edit counters
    if post_process_edits_applied > 0 || post_process_edits_rejected > 0 {
        let edits_counter = meter.u64_counter("dictation.pp_edits").build();
        edits_counter.add(
            post_process_edits_applied as u64,
            &[KeyValue::new("outcome", "applied")],
        );
        edits_counter.add(
            post_process_edits_rejected as u64,
            &[KeyValue::new("outcome", "rejected")],
        );
    }

    // RTF (Real-Time Factor) — processing time / audio duration
    if recording_duration_ms > 0 {
        let rtf = meter.f64_gauge("dictation.rtf").build();
        let processing_time =
            transcription_duration_ms + post_process_duration_ms + cleanup_duration_ms;
        rtf.record(
            processing_time as f64 / recording_duration_ms as f64,
            &[KeyValue::new("model_id", model_id.to_string())],
        );
    }

    // Audio capture overflow — batches dropped due to processing backpressure
    if audio_batches_dropped > 0 {
        let overflow_counter = meter.u64_counter("dictation.audio_batches_dropped").build();
        overflow_counter.add(audio_batches_dropped, &[]);
    }
}

// ── LLM metrics ──

/// Record the number of LLM attempts made during a single guard execution.
pub fn record_llm_attempts(count: u64) {
    let meter = global::meter(SERVICE_NAME);
    let counter = meter.u64_counter("dictation.llm.attempts").build();
    counter.add(count, &[]);
}

/// Record a successful LLM cleanup result.
pub fn record_llm_result_success() {
    let meter = global::meter(SERVICE_NAME);
    let counter = meter.u64_counter("dictation.llm.result").build();
    counter.add(1, &[KeyValue::new("outcome", "success")]);
}

/// Record a failed LLM cleanup result with the specific error code.
pub fn record_llm_result_fail(error_code: &str) {
    let meter = global::meter(SERVICE_NAME);
    let counter = meter.u64_counter("dictation.llm.result").build();
    counter.add(
        1,
        &[
            KeyValue::new("outcome", "fail"),
            KeyValue::new("error_code", error_code.to_string()),
        ],
    );
}

// ── VAD metrics ──

/// Record a VAD run.
pub fn record_vad_run() {
    let meter = global::meter(SERVICE_NAME);
    let counter = meter.u64_counter("dictation.vad.runs").build();
    counter.add(1, &[]);
}

/// Record a dictation skipped because VAD detected no speech.
pub fn record_no_speech_skip() {
    let meter = global::meter(SERVICE_NAME);
    let counter = meter.u64_counter("dictation.no_speech_skips").build();
    counter.add(1, &[]);
}

/// Record a transcript altered or dropped by the hallucination filter.
pub fn record_hallucination_filtered() {
    let meter = global::meter(SERVICE_NAME);
    let counter = meter
        .u64_counter("dictation.hallucinations_filtered")
        .build();
    counter.add(1, &[]);
}

/// Record a VAD result with its status and, when trimmed, the milliseconds removed.
pub fn record_vad_result(status: &str, trimmed_ms: u64) {
    let meter = global::meter(SERVICE_NAME);

    let counter = meter.u64_counter("dictation.vad.result").build();
    counter.add(1, &[KeyValue::new("status", status.to_string())]);

    if trimmed_ms > 0 {
        let hist = meter.f64_histogram("dictation.vad.trimmed_ms").build();
        hist.record(trimmed_ms as f64, &[]);
    }
}

// ── Post-processing metrics ──

/// Record a post-processing pipeline run.
pub fn record_pp_run() {
    let meter = global::meter(SERVICE_NAME);
    let counter = meter.u64_counter("dictation.pp.runs").build();
    counter.add(1, &[]);
}

/// Record the normalized edit distance between raw transcription and
/// post-processed output. Result is 0.0 (identical) to 1.0 (completely different).
pub fn record_pp_edit_distance(raw_text: &str, processed_text: &str) {
    let similarity = strsim::normalized_levenshtein(raw_text, processed_text);
    let distance = 1.0 - similarity;

    let meter = global::meter(SERVICE_NAME);
    let hist = meter
        .f64_histogram("dictation.pp.edit_distance")
        .with_boundaries(vec![
            0.0, 0.01, 0.02, 0.03, 0.05, 0.08, 0.10, 0.15, 0.20, 0.30, 0.50, 0.75, 1.0,
        ])
        .build();
    hist.record(distance, &[]);
}

// ── Spell correction metrics ──

/// Record spell correction metrics from a dictation's post-processing results.
/// `corrections` contains tuples of (edit_distance, confidence) for each applied
/// spell correction.
pub fn record_spell_corrections(corrections: &[(i32, f32)]) {
    if corrections.is_empty() {
        return;
    }

    let meter = global::meter(SERVICE_NAME);

    let correction_counter = meter.u64_counter("dictation.spell.corrections").build();
    correction_counter.add(corrections.len() as u64, &[]);

    let distance_counter = meter.u64_counter("dictation.spell.by_distance").build();
    let confidence_hist = meter
        .f64_histogram("dictation.spell.confidence")
        .with_boundaries(vec![
            0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.85, 0.9, 0.95, 1.0,
        ])
        .build();

    for (distance, confidence) in corrections {
        distance_counter.add(1, &[KeyValue::new("distance", format!("{distance}"))]);
        confidence_hist.record(*confidence as f64, &[]);
    }
}

/// Flush all pending telemetry data and shut down providers.
/// Call this on app exit.
/// Test connectivity to an OTLP endpoint by sending an empty trace export.
/// Returns Ok with a message on success, or Err with the failure reason.
pub fn test_connection(endpoint: &str) -> Result<String, String> {
    let endpoint = endpoint.trim().trim_end_matches('/');
    if endpoint.is_empty() {
        return Err("Endpoint is empty.".to_string());
    }

    // The OTLP HTTP trace receiver lives at {endpoint}/v1/traces.
    // Send an empty ExportTraceServiceRequest (valid protobuf: zero bytes).
    // A reachable collector returns 200 OK; unreachable returns a network error.
    let url = format!("{endpoint}/v1/traces");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/x-protobuf")
        .body(Vec::<u8>::new())
        .send()
        .map_err(|e| {
            if e.is_connect() {
                format!("Connection refused at {url} — is the LGTM stack running?")
            } else if e.is_timeout() {
                format!("Connection timed out at {url}")
            } else {
                format!("Request failed: {e}")
            }
        })?;

    let status = response.status();
    if status.is_success() {
        Ok(format!("Connected to {endpoint} (HTTP {status})"))
    } else {
        let body = response.text().unwrap_or_default();
        Err(format!("Endpoint returned HTTP {status}: {body}"))
    }
}

/// Flush all pending telemetry data and shut down providers.
/// Call this on app exit.
pub fn shutdown() {
    if !TELEMETRY_INITIALIZED.load(Ordering::SeqCst) {
        return;
    }

    if let Some(tp) = TRACER_PROVIDER.get() {
        if let Err(err) = tp.shutdown() {
            eprintln!("[telemetry] tracer shutdown error: {err}");
        }
    }
    if let Some(mp) = METER_PROVIDER.get() {
        if let Err(err) = mp.shutdown() {
            eprintln!("[telemetry] meter shutdown error: {err}");
        }
    }
    if let Some(lp) = LOGGER_PROVIDER.get() {
        if let Err(err) = lp.shutdown() {
            eprintln!("[telemetry] logger shutdown error: {err}");
        }
    }
}
