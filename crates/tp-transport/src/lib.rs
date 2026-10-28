//! Media and control transport (WebRTC, ADR-002), behind the Transport contract
//! (ARCH-00 §4.5).
//!
//! Not implemented in BOOT-01: this build opens no sockets and sends no media.

/// Descriptor for this crate.
pub const MODULE: tp_contracts::module::ModuleDescriptor = tp_contracts::module::ModuleDescriptor {
    crate_name: "tp-transport",
    role: "media-transport",
    state: tp_contracts::module::ModuleState::Foundation,
    owner_phase: "NET-03",
};
