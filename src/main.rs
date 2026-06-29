//! earthnet-node entrypoint: starts the HTTP ingest server.

use std::sync::Arc;

use earthnet_node::{fusion::Fusion, server::app, NodeIdentity};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "earthnet_node=info".into()),
        )
        .init();

    let identity = NodeIdentity::ephemeral();
    tracing::info!(pubkey = %identity.pubkey_hex(), "node identity (ephemeral)");

    let consensus_n: usize = std::env::var("EARTHNET_CONSENSUS_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let fusion = Arc::new(Fusion::new(identity, consensus_n));

    let addr = std::env::var("EARTHNET_NODE_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = TcpListener::bind(&addr).await.expect("bind address");
    tracing::info!(%addr, consensus_n, "earthnet-node listening");

    axum::serve(listener, app(fusion))
        .await
        .expect("server error");
}
