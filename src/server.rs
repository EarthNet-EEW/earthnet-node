//! HTTP ingest surface. Adapters POST signed Observation protobuf bytes to
//! `POST /observations`; the node verifies and feeds the fusion engine.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Router,
};

use crate::fusion::{Fusion, IngestError};

/// Builds the router with the fusion engine as shared state.
pub fn app(fusion: Arc<Fusion>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/observations", post(ingest))
        .with_state(fusion)
}

async fn health() -> &'static str {
    "ok"
}

/// Accepts one Observation (raw protobuf body). Returns:
///   202 Accepted        — verified and ingested
///   400 Bad Request     — undecodable / bad fields
///   401 Unauthorized    — signature failed
async fn ingest(State(fusion): State<Arc<Fusion>>, body: Bytes) -> StatusCode {
    match fusion.ingest_bytes(&body) {
        Ok(Some(_event)) => {
            tracing::info!("observation ingested → ConfirmedEvent emitted");
            StatusCode::ACCEPTED
        }
        Ok(None) => StatusCode::ACCEPTED,
        Err(IngestError::Signature) => StatusCode::UNAUTHORIZED,
        Err(IngestError::Decode | IngestError::BadFields) => StatusCode::BAD_REQUEST,
    }
}
