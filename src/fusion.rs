//! Fusion + consensus (v0).
//!
//! Deliberately minimal for the first node slice — refined in a later slice with
//! geospatial correlation and time windows. Current rules:
//!
//! - OFFICIAL + P-wave: emit a ConfirmedEvent immediately (high trust).
//! - PHONE: buffer picks; once ≥ N are buffered, emit one consensus ConfirmedEvent
//!   and clear the buffer.
//!
//! NOT YET MODELED (later slices): spatial/temporal correlation of phone picks,
//! deduplication, magnitude estimation, supersede/revision of events.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use earthnet_protocol::{
    sign, verify, ConfirmedEvent, EvidenceKind, Observation, SourceType, PROTOCOL_VERSION,
};
use prost::Message;

use crate::{random_id, NodeIdentity};

/// Why an ingested observation was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestError {
    /// Bytes were not a valid Observation.
    Decode,
    /// Signature did not verify.
    Signature,
    /// Unsupported protocol version or unusable fields.
    BadFields,
}

struct State {
    /// Buffered phone picks awaiting consensus.
    phone_buffer: Vec<Observation>,
    /// Events emitted so far (inspection / tests).
    emitted: Vec<ConfirmedEvent>,
}

/// The fusion engine. Thread-safe; share via `Arc`.
pub struct Fusion {
    identity: NodeIdentity,
    consensus_n: usize,
    state: Mutex<State>,
}

impl Fusion {
    /// `consensus_n` = how many phone picks trigger a consensus event.
    pub fn new(identity: NodeIdentity, consensus_n: usize) -> Self {
        Self {
            identity,
            consensus_n: consensus_n.max(1),
            state: Mutex::new(State {
                phone_buffer: Vec::new(),
                emitted: Vec::new(),
            }),
        }
    }

    /// Decode + verify + ingest raw Observation bytes.
    pub fn ingest_bytes(&self, bytes: &[u8]) -> Result<Option<ConfirmedEvent>, IngestError> {
        let obs = Observation::decode(bytes).map_err(|_| IngestError::Decode)?;
        verify(&obs).map_err(|_| IngestError::Signature)?;
        self.ingest(obs)
    }

    /// Ingest an already-verified Observation and maybe produce a ConfirmedEvent.
    pub fn ingest(&self, obs: Observation) -> Result<Option<ConfirmedEvent>, IngestError> {
        if obs.protocol_version != PROTOCOL_VERSION {
            return Err(IngestError::BadFields);
        }

        let mut st = self.state.lock().expect("fusion state poisoned");
        let event = match SourceType::try_from(obs.source_type) {
            Ok(SourceType::Official) if obs.p_wave_detected => {
                Some(self.make_event(&[obs], EvidenceKind::Official))
            }
            Ok(SourceType::Official) => None, // official but no P-wave yet
            Ok(SourceType::Phone) => {
                st.phone_buffer.push(obs);
                if st.phone_buffer.len() >= self.consensus_n {
                    let picks = std::mem::take(&mut st.phone_buffer);
                    Some(self.make_event(&picks, EvidenceKind::Consensus))
                } else {
                    None
                }
            }
            _ => return Err(IngestError::BadFields),
        };

        if let Some(ref evt) = event {
            st.emitted.push(evt.clone());
        }
        Ok(event)
    }

    /// Number of events emitted so far.
    pub fn emitted_count(&self) -> usize {
        self.state
            .lock()
            .expect("fusion state poisoned")
            .emitted
            .len()
    }

    /// Builds + signs a ConfirmedEvent from the contributing picks.
    fn make_event(&self, picks: &[Observation], evidence: EvidenceKind) -> ConfirmedEvent {
        let lead = &picks[0];
        let mut evt = ConfirmedEvent {
            protocol_version: PROTOCOL_VERSION,
            event_id: random_id(),
            pubkey: self.identity.pubkey(),
            origin_time_ns: lead.captured_at_ns,
            issued_at_ns: now_ns(),
            epicenter: lead.location.clone(),
            depth_km: 0.0,
            magnitude: lead.reported_magnitude,
            magnitude_uncert: 0.0,
            evidence: evidence as i32,
            num_observations: picks.len() as u32,
            obs_ids: picks.iter().map(|p| p.observation_id.clone()).collect(),
            supersedes: Vec::new(),
            signature: Vec::new(),
        };
        sign(self.identity.signing_key(), &mut evt);
        evt
    }
}

fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}
