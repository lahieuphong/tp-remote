//! TP Remote network protocol (wire format v1: versioned JSON, ADR-002 amendment A1).
//!
//! BOOT-01 scope: the envelope, version support, size limits and strict decoding.
//! The only defined message is [`Hello`], and only its shape — there is no handshake,
//! no transport and no remote-control command. Message types that ARCH-00 reserves for
//! later phases are recognised and rejected as not authorised.
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

mod envelope;
mod error;
mod limits;
mod message;
mod version;

pub use envelope::{decode, encode};
pub use error::{ProtocolError, ProtocolErrorKind};
pub use limits::{Channel, MAX_APP_VERSION_LEN};
pub use message::{Body, Hello, Message, MessageKind, RESERVED_MESSAGE_TYPES};
pub use version::{
    MAX_SUPPORTED_PROTOCOL_VERSION, MIN_SUPPORTED_PROTOCOL_VERSION, PROTOCOL_VERSION, is_supported,
    support,
};

/// Descriptor for this crate.
pub const MODULE: tp_contracts::module::ModuleDescriptor = tp_contracts::module::ModuleDescriptor {
    crate_name: "tp-protocol",
    role: "protocol",
    state: tp_contracts::module::ModuleState::Partial,
    owner_phase: "NET-03",
};
