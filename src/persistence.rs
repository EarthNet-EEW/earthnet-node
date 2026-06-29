//! Async persistence to PostgreSQL/TimescaleDB.
//!
//! NEVER on the hot path (DESIGN guardrail): records are handed to a background
//! writer via a bounded channel with `try_send` — if the channel is full the
//! record is dropped rather than stalling ingest. A node with no
//! `EARTHNET_DATABASE_URL` runs fully with persistence disabled (no-op).

use earthnet_protocol::{ConfirmedEvent, Observation};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::mpsc;

enum Record {
    Observation(Observation),
    Event(ConfirmedEvent),
}

/// Handle for recording observations/events; cloneable, cheap.
#[derive(Clone)]
pub struct Persistence {
    tx: Option<mpsc::Sender<Record>>,
}

impl Persistence {
    /// A no-op sink (no database configured).
    pub fn disabled() -> Self {
        Self { tx: None }
    }

    /// Connects, runs migrations, and spawns the background writer.
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new().max_connections(4).connect(url).await?;
        migrate(&pool).await?;
        let (tx, mut rx) = mpsc::channel::<Record>(2048);
        tokio::spawn(async move {
            while let Some(rec) = rx.recv().await {
                let res = match rec {
                    Record::Observation(o) => insert_observation(&pool, &o).await,
                    Record::Event(e) => insert_event(&pool, &e).await,
                };
                if let Err(e) = res {
                    tracing::warn!(error = %e, "persist failed");
                }
            }
        });
        Ok(Self { tx: Some(tx) })
    }

    pub fn is_enabled(&self) -> bool {
        self.tx.is_some()
    }

    /// Non-blocking record of a verified observation (dropped if backlogged).
    pub fn record_observation(&self, obs: Observation) {
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(Record::Observation(obs));
        }
    }

    /// Non-blocking record of an emitted confirmed event.
    pub fn record_event(&self, evt: ConfirmedEvent) {
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(Record::Event(evt));
        }
    }
}

async fn migrate(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(include_str!("schema.sql"))
        .execute(pool)
        .await?;
    Ok(())
}

async fn insert_observation(pool: &PgPool, o: &Observation) -> Result<(), sqlx::Error> {
    let geohash = o.location.as_ref().map(|l| l.geohash.clone());
    sqlx::query(
        "INSERT INTO observations \
         (captured_at, observation_id, pubkey, source_type, source_id, geohash, \
          sta_lta_ratio, p_wave_detected, estimated_pga, reported_magnitude) \
         VALUES (to_timestamp($1::double precision / 1e9), $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(o.captured_at_ns)
    .bind(&o.observation_id)
    .bind(&o.pubkey)
    .bind(o.source_type)
    .bind(&o.source_id)
    .bind(geohash)
    .bind(o.sta_lta_ratio)
    .bind(o.p_wave_detected)
    .bind(o.estimated_pga)
    .bind(o.reported_magnitude)
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_event(pool: &PgPool, e: &ConfirmedEvent) -> Result<(), sqlx::Error> {
    let geohash = e.epicenter.as_ref().map(|l| l.geohash.clone());
    let supersedes = if e.supersedes.is_empty() {
        None
    } else {
        Some(e.supersedes.clone())
    };
    sqlx::query(
        "INSERT INTO confirmed_events \
         (issued_at, event_id, origin_time, node_pubkey, epicenter_geohash, \
          magnitude, magnitude_uncert, evidence, num_observations, supersedes) \
         VALUES (to_timestamp($1::double precision / 1e9), $2, \
                 to_timestamp($3::double precision / 1e9), $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(e.issued_at_ns)
    .bind(&e.event_id)
    .bind(e.origin_time_ns)
    .bind(&e.pubkey)
    .bind(geohash)
    .bind(e.magnitude)
    .bind(e.magnitude_uncert)
    .bind(e.evidence)
    .bind(e.num_observations as i64)
    .bind(supersedes)
    .execute(pool)
    .await?;
    Ok(())
}
