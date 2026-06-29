# earthnet-node

Node for the [EarthNet](https://github.com/devjamez/earthnet-protocol) earthquake
early-warning network. Ingests signed `Observation`s from country adapters and
sensors, fuses them + reaches consensus, and emits signed `ConfirmedEvent`s that
trigger client alarms.

## Trust model (DESIGN §5)

- **OFFICIAL** source (e.g. a CSN/Chile adapter) with a P-wave pick → fires on its own.
- **PHONE** source → requires consensus of ≥ N correlated picks.

## Ingest API

Adapters POST a single signed Observation (raw protobuf bytes) to the node:

```
POST /observations    body = Observation protobuf
  202 Accepted     verified + ingested
  400 Bad Request  undecodable / bad fields
  401 Unauthorized signature failed
GET  /health → "ok"
```

## Run

```sh
cargo run
# env: EARTHNET_NODE_ADDR (default 127.0.0.1:8080), EARTHNET_CONSENSUS_N (default 3)
```

## Status

🟡 v0.1 — first vertical slice: HTTP ingest + signature verification + minimal
fusion. NOT yet modeled: geospatial/temporal consensus correlation, magnitude
estimation, event revision, relay fan-out, identity persistence.

## License

AGPL-3.0-or-later.
