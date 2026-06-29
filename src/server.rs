//! HTTP ingest surface. Adapters POST signed Observation protobuf bytes to
//! `POST /observations`; the node verifies, feeds the fusion engine, and forwards
//! any resulting ConfirmedEvent to the relay.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use prost::Message as _;

use crate::fusion::{Fusion, IngestError};
use crate::relay_client::RelayForwarder;

/// Shared server state.
#[derive(Clone)]
pub struct AppState {
    pub fusion: Arc<Fusion>,
    pub relay: RelayForwarder,
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

/// Accepts one Observation (raw protobuf body). On a confirmed event, forwards it
/// to the relay. Returns:
///   202 Accepted        — verified and ingested
///   400 Bad Request     — undecodable / bad fields
///   401 Unauthorized    — signature failed
async fn ingest(State(state): State<AppState>, body: Bytes) -> StatusCode {
    match state.fusion.ingest_bytes(&body) {
        Ok(Some(event)) => {
            state.relay.forward(event.encode_to_vec());
            StatusCode::ACCEPTED
        }
        Ok(None) => StatusCode::ACCEPTED,
        Err(IngestError::Signature) => StatusCode::UNAUTHORIZED,
        Err(IngestError::Decode | IngestError::BadFields) => StatusCode::BAD_REQUEST,
    }
}
