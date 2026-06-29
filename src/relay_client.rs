//! Forwards ConfirmedEvents from the node to a relay's `/events` endpoint.
//!
//! Forwarding is fire-and-forget (spawned) so it never blocks the ingest
//! response. The hot path that matters for latency is node → relay → mobile;
//! this kicks that off the instant fusion confirms an event.

use std::time::Duration;

/// Posts encoded ConfirmedEvent bytes to a relay. A `None` URL disables forwarding.
#[derive(Clone)]
pub struct RelayForwarder {
    client: reqwest::Client,
    events_url: Option<String>,
}

impl RelayForwarder {
    /// `relay_base` e.g. `http://127.0.0.1:8090`; `None` = no relay configured.
    pub fn new(relay_base: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .expect("build reqwest client");
        let events_url = relay_base.map(|b| format!("{}/events", b.trim_end_matches('/')));
        Self { client, events_url }
    }

    /// Whether a relay is configured.
    pub fn is_enabled(&self) -> bool {
        self.events_url.is_some()
    }

    /// Fire-and-forget POST of the encoded ConfirmedEvent to the relay.
    pub fn forward(&self, bytes: Vec<u8>) {
        let Some(url) = self.events_url.clone() else {
            return;
        };
        let client = self.client.clone();
        tokio::spawn(async move {
            match client
                .post(&url)
                .header("content-type", "application/x-protobuf")
                .body(bytes)
                .send()
                .await
            {
                Ok(resp) => {
                    tracing::info!(status = %resp.status(), "forwarded ConfirmedEvent to relay")
                }
                Err(e) => tracing::warn!(error = %e, "relay forward failed"),
            }
        });
    }
}
