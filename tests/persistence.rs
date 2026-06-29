//! Persistence integration test. Runs ONLY when EARTHNET_TEST_DATABASE_URL is
//! set (e.g. the docker-compose TimescaleDB); otherwise it skips, so CI without
//! a database is unaffected.
//!
//! Run locally:
//!   docker compose up -d
//!   EARTHNET_TEST_DATABASE_URL=postgres://postgres:earthnet@127.0.0.1:5433/earthnet \
//!     cargo test --test persistence -- --nocapture

use std::time::Duration;

use earthnet_node::persistence::Persistence;
use earthnet_protocol::{sign, Location, Observation, SourceType, PROTOCOL_VERSION};
use ed25519_dalek::SigningKey;
use rand::{rngs::OsRng, RngCore};
use sqlx::Row;

fn signed_observation() -> Observation {
    let mut secret = [0u8; 32];
    OsRng.fill_bytes(&mut secret);
    let key = SigningKey::from_bytes(&secret);
    let mut id = [0u8; 16];
    OsRng.fill_bytes(&mut id);
    let mut obs = Observation {
        protocol_version: PROTOCOL_VERSION,
        observation_id: id.to_vec(),
        pubkey: key.verifying_key().to_bytes().to_vec(),
        source_type: SourceType::Official as i32,
        source_id: "CX:PB01".into(),
        captured_at_ns: 1_700_000_000_000_000_000,
        clock_uncert_ms: 5,
        location: Some(Location {
            geohash: "66jd2".into(),
            precision_m: 100,
        }),
        sta_lta_ratio: 9.0,
        p_wave_detected: true,
        estimated_pga: 0.03,
        reported_magnitude: 0.0,
        signature: Vec::new(),
    };
    sign(&key, &mut obs);
    obs
}

#[tokio::test]
async fn records_observation_to_timescaledb() {
    let Ok(url) = std::env::var("EARTHNET_TEST_DATABASE_URL") else {
        eprintln!("skip: EARTHNET_TEST_DATABASE_URL not set");
        return;
    };

    let pool = sqlx::postgres::PgPool::connect(&url)
        .await
        .expect("connect");
    let before: i64 = sqlx::query("SELECT count(*) FROM observations")
        .fetch_one(&pool)
        .await
        .map(|r| r.get::<i64, _>(0))
        .unwrap_or(0);

    let persistence = Persistence::connect(&url)
        .await
        .expect("persistence connect");
    persistence.record_observation(signed_observation());

    // let the async writer flush
    tokio::time::sleep(Duration::from_millis(800)).await;

    let after: i64 = sqlx::query("SELECT count(*) FROM observations")
        .fetch_one(&pool)
        .await
        .map(|r| r.get::<i64, _>(0))
        .expect("count");
    assert!(after > before, "observation should have been persisted");
}
