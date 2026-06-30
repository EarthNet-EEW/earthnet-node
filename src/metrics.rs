//! Prometheus metrics, exposed at `GET /metrics`.
//!
//! Phase-1 observability: you cannot operate a network you cannot measure. The
//! highest-value signal here is `earthnet_persistence_dropped_total` — the
//! fire-and-forget persistence channel silently drops under load, and until now
//! that was invisible.

use std::sync::OnceLock;

use prometheus::{
    Encoder, Histogram, HistogramOpts, IntCounter, IntCounterVec, Opts, Registry, TextEncoder,
};

/// Process-wide metrics registry and instruments.
pub struct Metrics {
    pub registry: Registry,
    /// Verified observations ingested, by source_type.
    pub observations: IntCounterVec,
    /// ConfirmedEvents emitted, by evidence kind.
    pub events: IntCounterVec,
    /// Ingest rejections, by kind (decode | signature | bad_fields).
    pub ingest_errors: IntCounterVec,
    /// Records the async persistence channel dropped (backlogged).
    pub persistence_dropped: IntCounter,
    /// Ingest handler latency (seconds).
    pub ingest_seconds: Histogram,
}

static METRICS: OnceLock<Metrics> = OnceLock::new();

/// The global metrics instance (lazily initialized).
pub fn metrics() -> &'static Metrics {
    METRICS.get_or_init(|| {
        let registry = Registry::new();
        let observations = IntCounterVec::new(
            Opts::new(
                "earthnet_observations_ingested_total",
                "Verified observations ingested",
            ),
            &["source_type"],
        )
        .expect("metric");
        let events = IntCounterVec::new(
            Opts::new("earthnet_events_emitted_total", "ConfirmedEvents emitted"),
            &["evidence"],
        )
        .expect("metric");
        let ingest_errors = IntCounterVec::new(
            Opts::new("earthnet_ingest_errors_total", "Ingest rejections"),
            &["kind"],
        )
        .expect("metric");
        let persistence_dropped = IntCounter::new(
            "earthnet_persistence_dropped_total",
            "Records dropped because the async persistence channel was full",
        )
        .expect("metric");
        let ingest_seconds = Histogram::with_opts(
            HistogramOpts::new(
                "earthnet_ingest_seconds",
                "Ingest handler latency (seconds)",
            )
            .buckets(vec![
                0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 2.0,
            ]),
        )
        .expect("metric");

        for c in [&observations, &events, &ingest_errors] {
            registry.register(Box::new(c.clone())).expect("register");
        }
        registry
            .register(Box::new(persistence_dropped.clone()))
            .expect("register");
        registry
            .register(Box::new(ingest_seconds.clone()))
            .expect("register");

        Metrics {
            registry,
            observations,
            events,
            ingest_errors,
            persistence_dropped,
            ingest_seconds,
        }
    })
}

/// Renders the registry in Prometheus text exposition format.
pub fn encode() -> String {
    let mut buf = Vec::new();
    let encoder = TextEncoder::new();
    let _ = encoder.encode(&metrics().registry.gather(), &mut buf);
    String::from_utf8(buf).unwrap_or_default()
}
