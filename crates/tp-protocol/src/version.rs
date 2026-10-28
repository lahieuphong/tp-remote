//! Protocol version constants.
//!
//! v1 is strict: decoders reject unknown fields, so any change to a message shape
//! requires a new protocol version (ADR-002 amendment A1).

use tp_contracts::ipc::ProtocolSupport;

/// Version this build writes.
pub const PROTOCOL_VERSION: u16 = 1;
/// Oldest version this build accepts.
pub const MIN_SUPPORTED_PROTOCOL_VERSION: u16 = 1;
/// Newest version this build accepts.
pub const MAX_SUPPORTED_PROTOCOL_VERSION: u16 = 1;

const _: () = assert!(
    MIN_SUPPORTED_PROTOCOL_VERSION >= 1
        && MIN_SUPPORTED_PROTOCOL_VERSION <= PROTOCOL_VERSION
        && PROTOCOL_VERSION <= MAX_SUPPORTED_PROTOCOL_VERSION
);

/// Whether a version number read from the wire is accepted.
pub fn is_supported(version: u64) -> bool {
    (u64::from(MIN_SUPPORTED_PROTOCOL_VERSION)..=u64::from(MAX_SUPPORTED_PROTOCOL_VERSION))
        .contains(&version)
}

/// Version support, as reported to the UI.
pub fn support() -> ProtocolSupport {
    ProtocolSupport {
        current: PROTOCOL_VERSION,
        min_supported: MIN_SUPPORTED_PROTOCOL_VERSION,
        max_supported: MAX_SUPPORTED_PROTOCOL_VERSION,
    }
}
