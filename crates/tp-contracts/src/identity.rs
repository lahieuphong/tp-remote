//! Device identity contract (ARCH-00 §4.7). FUTURE implementation: OS-keystore-backed
//! Ed25519 identity (NET-03, U-16). The private key never leaves the implementation.

use std::fmt;

use crate::error::CoreError;
use crate::id::DeviceId;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdentityPublicKey(pub [u8; 32]);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature(pub [u8; 64]);

impl fmt::Debug for IdentityPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IdentityPublicKey({:02x}{:02x}{:02x}{:02x}…)",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Signature(<64 bytes>)")
    }
}

/// Domain separation for signatures: a signature for one purpose can never be replayed
/// as a signature for another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignContext {
    SessionTranscript,
}

impl SignContext {
    pub const fn label(self) -> &'static [u8] {
        match self {
            SignContext::SessionTranscript => b"tp-remote/session-transcript/v1",
        }
    }
}

pub trait DeviceIdentity: Send + Sync {
    fn device_id(&self) -> DeviceId;

    fn public_key(&self) -> IdentityPublicKey;

    /// Signs `label(context) || message`.
    fn sign(&self, context: SignContext, message: &[u8]) -> Result<Signature, CoreError>;

    /// Human-comparable fingerprint for the consent prompt.
    fn display_fingerprint(&self) -> String;
}
