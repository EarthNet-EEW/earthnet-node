use earthnet_node::fusion::{Fusion, IngestError};
use earthnet_node::NodeIdentity;
use earthnet_protocol::{
    sign, verify, EvidenceKind, Location, Observation, SourceType, PROTOCOL_VERSION,
};
use ed25519_dalek::SigningKey;
use prost::Message;
use rand::{rngs::OsRng, RngCore};

fn signed_obs(source: SourceType, p_wave: bool) -> Observation {
    let mut secret = [0u8; 32];
    OsRng.fill_bytes(&mut secret);
    let key = SigningKey::from_bytes(&secret);
    let mut id = [0u8; 16];
    OsRng.fill_bytes(&mut id);
    let mut obs = Observation {
        protocol_version: PROTOCOL_VERSION,
        observation_id: id.to_vec(),
        pubkey: key.verifying_key().to_bytes().to_vec(),
        source_type: source as i32,
        source_id: String::new(),
        captured_at_ns: 1_700_000_000_000_000_000,
        clock_uncert_ms: 10,
        location: Some(Location {
            geohash: "66jd2".into(),
            precision_m: 2400,
        }),
        sta_lta_ratio: 8.0,
        p_wave_detected: p_wave,
        estimated_pga: 0.01,
        reported_magnitude: 5.5,
        signature: Vec::new(),
    };
    sign(&key, &mut obs);
    obs
}

fn fusion(consensus_n: usize) -> Fusion {
    Fusion::new(NodeIdentity::ephemeral(), consensus_n)
}

#[test]
fn official_with_pwave_emits_signed_event() {
    let f = fusion(3);
    let evt = f
        .ingest(signed_obs(SourceType::Official, true))
        .unwrap()
        .expect("official + p-wave must emit");
    assert_eq!(evt.evidence, EvidenceKind::Official as i32);
    assert_eq!(evt.num_observations, 1);
    assert!(verify(&evt).is_ok(), "event must be signed by the node");
}

#[test]
fn official_without_pwave_does_not_emit() {
    let f = fusion(3);
    assert!(f
        .ingest(signed_obs(SourceType::Official, false))
        .unwrap()
        .is_none());
}

#[test]
fn phone_consensus_fires_at_threshold() {
    let f = fusion(3);
    assert!(f
        .ingest(signed_obs(SourceType::Phone, true))
        .unwrap()
        .is_none());
    assert!(f
        .ingest(signed_obs(SourceType::Phone, true))
        .unwrap()
        .is_none());
    let evt = f
        .ingest(signed_obs(SourceType::Phone, true))
        .unwrap()
        .expect("third phone pick must reach consensus");
    assert_eq!(evt.evidence, EvidenceKind::Consensus as i32);
    assert_eq!(evt.num_observations, 3);
    assert!(verify(&evt).is_ok());
    // buffer cleared after firing
    assert!(f
        .ingest(signed_obs(SourceType::Phone, true))
        .unwrap()
        .is_none());
}

#[test]
fn invalid_signature_is_rejected() {
    let f = fusion(3);
    let mut obs = signed_obs(SourceType::Official, true);
    obs.sta_lta_ratio = 999.0; // tamper after signing
    let bytes = obs.encode_to_vec();
    assert_eq!(f.ingest_bytes(&bytes), Err(IngestError::Signature));
}

#[test]
fn undecodable_bytes_rejected() {
    let f = fusion(3);
    assert_eq!(
        f.ingest_bytes(&[0xff, 0xff, 0xff]),
        Err(IngestError::Decode)
    );
}

#[test]
fn wrong_protocol_version_rejected() {
    let f = fusion(3);
    // Re-sign with a bad version so the signature is valid but the version isn't.
    let mut secret = [0u8; 32];
    OsRng.fill_bytes(&mut secret);
    let key = SigningKey::from_bytes(&secret);
    let mut obs = signed_obs(SourceType::Official, true);
    obs.protocol_version = 999;
    obs.pubkey = key.verifying_key().to_bytes().to_vec();
    sign(&key, &mut obs);
    assert_eq!(f.ingest(obs), Err(IngestError::BadFields));
}
