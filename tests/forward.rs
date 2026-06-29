//! Proves the node forwards a confirmed event to a relay's /events endpoint.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::Router;
use earthnet_node::fusion::Fusion;
use earthnet_node::relay_client::RelayForwarder;
use earthnet_node::server::{app, AppState};
use earthnet_node::NodeIdentity;
use earthnet_protocol::{
    sign, ConfirmedEvent, Location, Observation, SourceType, PROTOCOL_VERSION,
};
use ed25519_dalek::SigningKey;
use prost::Message;
use rand::{rngs::OsRng, RngCore};
use tokio::net::TcpListener;
use tower::ServiceExt;

type Store = Arc<Mutex<Vec<Vec<u8>>>>;

async fn capture(State(store): State<Store>, body: Bytes) -> StatusCode {
    store.lock().unwrap().push(body.to_vec());
    StatusCode::ACCEPTED
}

fn signed_official_obs() -> Vec<u8> {
    let mut secret = [0u8; 32];
    OsRng.fill_bytes(&mut secret);
    let key = SigningKey::from_bytes(&secret);
    let mut obs = Observation {
        protocol_version: PROTOCOL_VERSION,
        observation_id: vec![3u8; 16],
        pubkey: key.verifying_key().to_bytes().to_vec(),
        source_type: SourceType::Official as i32,
        source_id: "CX:PB01".into(),
        captured_at_ns: 1_700_000_000_000_000_000,
        clock_uncert_ms: 5,
        location: Some(Location {
            geohash: "66jd2".into(),
            precision_m: 100,
        }),
        sta_lta_ratio: 12.0,
        p_wave_detected: true,
        estimated_pga: 0.05,
        reported_magnitude: 6.0,
        signature: Vec::new(),
    };
    sign(&key, &mut obs);
    obs.encode_to_vec()
}

#[tokio::test]
async fn node_forwards_confirmed_event_to_relay() {
    // Mock relay capturing posted bodies.
    let store: Store = Arc::new(Mutex::new(Vec::new()));
    let relay_app = Router::new()
        .route("/events", post(capture))
        .with_state(store.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, relay_app).await.unwrap();
    });

    // Node wired to forward to the mock relay.
    let state = AppState {
        fusion: Arc::new(Fusion::new(NodeIdentity::ephemeral(), 3, 100.0, 30)),
        relay: RelayForwarder::new(Some(format!("http://{addr}"))),
    };

    let resp = app(state)
        .oneshot(
            Request::post("/observations")
                .body(Body::from(signed_official_obs()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    // Wait for the fire-and-forget forward to land.
    let mut got = None;
    for _ in 0..100 {
        if let Some(bytes) = store.lock().unwrap().first().cloned() {
            got = Some(bytes);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let bytes = got.expect("relay never received the forwarded event");
    let evt = ConfirmedEvent::decode(bytes.as_slice()).expect("relay got a valid ConfirmedEvent");
    assert_eq!(evt.num_observations, 1);
}
