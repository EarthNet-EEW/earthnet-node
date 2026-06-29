-- EarthNet node persistence schema (PostgreSQL + TimescaleDB).
-- Applied idempotently on node startup. Off the hot path.

CREATE EXTENSION IF NOT EXISTS timescaledb;

CREATE TABLE IF NOT EXISTS observations (
    captured_at        timestamptz NOT NULL,
    observation_id     bytea       NOT NULL,
    pubkey             bytea       NOT NULL,
    source_type        int         NOT NULL,
    source_id          text,
    geohash            text,
    sta_lta_ratio      real,
    p_wave_detected    boolean,
    estimated_pga      real,
    reported_magnitude real
);
SELECT create_hypertable('observations', 'captured_at', if_not_exists => TRUE);

CREATE TABLE IF NOT EXISTS confirmed_events (
    issued_at         timestamptz NOT NULL,
    event_id          bytea       NOT NULL,
    origin_time       timestamptz,
    node_pubkey       bytea       NOT NULL,
    epicenter_geohash text,
    magnitude         real,
    magnitude_uncert  real,
    evidence          int,
    num_observations  bigint,
    supersedes        bytea
);
SELECT create_hypertable('confirmed_events', 'issued_at', if_not_exists => TRUE);

-- Identity reputation mirror (key-value, for queries/dashboard). The node's
-- authoritative store stays in-memory + the TSV file; this is updated async.
CREATE TABLE IF NOT EXISTS reputation (
    pubkey     bytea PRIMARY KEY,
    weight     real        NOT NULL,
    updated_at timestamptz NOT NULL
);
