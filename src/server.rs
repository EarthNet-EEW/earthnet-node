//! HTTP ingest surface. Adapters POST signed Observation protobuf bytes to
//! `POST /observations`; the node verifies, persists (async), feeds the fusion
//! engine, and forwards any resulting ConfirmedEvent to the relay.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use earthnet_protocol::{verify, Observation};
use prost::Message as _;

use std::time::Instant;

use crate::fusion::{Fusion, IngestError};
use crate::metrics::metrics;
use crate::persistence::Persistence;
use crate::relay_client::RelayForwarder;

/// Shared server state.
#[derive(Clone)]
pub struct AppState {
    pub fusion: Arc<Fusion>,
    pub relay: RelayForwarder,
    pub persistence: Persistence,
}

/// Builds the router.
pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics_handler))
        .route("/observations", post(ingest))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

/// Prometheus metrics in text exposition format.
async fn metrics_handler() -> impl axum::response::IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        crate::metrics::encode(),
    )
}

/// Accepts one Observation (raw protobuf body):
///   202 Accepted        — verified, persisted, ingested
///   400 Bad Request     — undecodable / bad fields
///   401 Unauthorized    — signature failed
async fn ingest(State(state): State<AppState>, body: Bytes) -> StatusCode {
    let start = Instant::now();
    let m = metrics();
    let obs = match Observation::decode(body.as_ref()) {
        Ok(o) => o,
        Err(_) => {
            m.ingest_errors.with_label_values(&["decode"]).inc();
            return StatusCode::BAD_REQUEST;
        }
    };
    if verify(&obs).is_err() {
        m.ingest_errors.with_label_values(&["signature"]).inc();
        return StatusCode::UNAUTHORIZED;
    }
    m.observations
        .with_label_values(&[&obs.source_type.to_string()])
        .inc();

    // Persist every verified observation (async, off the hot path).
    state.persistence.record_observation(obs.clone());

    let code = match state.fusion.ingest(obs) {
        Ok(Some(event)) => {
            m.events
                .with_label_values(&[&event.evidence.to_string()])
                .inc();
            state.persistence.record_event(event.clone());
            // Consensus events update reputation — mirror the snapshot to the DB.
            if event.evidence == earthnet_protocol::EvidenceKind::Consensus as i32 {
                state
                    .persistence
                    .record_reputation(state.fusion.reputation_snapshot());
            }
            state.relay.forward(event.encode_to_vec());
            StatusCode::ACCEPTED
        }
        Ok(None) => StatusCode::ACCEPTED,
        Err(IngestError::BadFields) => {
            m.ingest_errors.with_label_values(&["bad_fields"]).inc();
            StatusCode::BAD_REQUEST
        }
        // signature already checked above; any decode/other error is a bad request
        Err(IngestError::Decode | IngestError::Signature) => StatusCode::BAD_REQUEST,
    };
    m.ingest_seconds.observe(start.elapsed().as_secs_f64());
    code
}
