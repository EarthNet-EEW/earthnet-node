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

use crate::fusion::{Fusion, IngestError};
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
        .route("/observations", post(ingest))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

/// Accepts one Observation (raw protobuf body):
///   202 Accepted        — verified, persisted, ingested
///   400 Bad Request     — undecodable / bad fields
///   401 Unauthorized    — signature failed
async fn ingest(State(state): State<AppState>, body: Bytes) -> StatusCode {
    let obs = match Observation::decode(body.as_ref()) {
        Ok(o) => o,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    if verify(&obs).is_err() {
        return StatusCode::UNAUTHORIZED;
    }

    // Persist every verified observation (async, off the hot path).
    state.persistence.record_observation(obs.clone());

    match state.fusion.ingest(obs) {
        Ok(Some(event)) => {
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
        Err(IngestError::BadFields) => StatusCode::BAD_REQUEST,
        // signature already checked above; any decode/other error is a bad request
        Err(IngestError::Decode | IngestError::Signature) => StatusCode::BAD_REQUEST,
    }
}
