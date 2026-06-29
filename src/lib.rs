//! earthnet-node — ingests signed [`Observation`](earthnet_protocol::Observation)s,
//! fuses them + reaches consensus, and emits signed
//! [`ConfirmedEvent`](earthnet_protocol::ConfirmedEvent)s that trigger client alarms.
//!
//! Trust model (DESIGN §5): an OFFICIAL source fires on its own; PHONE sources
//! require consensus of ≥ N correlated picks.

pub mod fusion;
pub mod server;

use ed25519_dalek::SigningKey;
use rand::{rngs::OsRng, RngCore};

/// The node's Ed25519 identity. Signs every [`ConfirmedEvent`](earthnet_protocol::ConfirmedEvent)
/// it emits. v0.1 uses an ephemeral key generated at startup; persistence is a later slice.
pub struct NodeIdentity {
    key: SigningKey,
}

impl NodeIdentity {
    /// Generates a fresh in-memory identity. Not persisted.
    pub fn ephemeral() -> Self {
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);
        Self {
            key: SigningKey::from_bytes(&secret),
        }
    }

    /// Raw 32-byte public key.
    pub fn pubkey(&self) -> Vec<u8> {
        self.key.verifying_key().to_bytes().to_vec()
    }

    /// Hex of the public key (safe to log — never log the secret).
    pub fn pubkey_hex(&self) -> String {
        self.pubkey().iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Signing key, for producing ConfirmedEvent signatures.
    pub fn signing_key(&self) -> &SigningKey {
        &self.key
    }
}

/// Random 16-byte identifier (event_id).
pub(crate) fn random_id() -> Vec<u8> {
    let mut id = [0u8; 16];
    OsRng.fill_bytes(&mut id);
    id.to_vec()
}
