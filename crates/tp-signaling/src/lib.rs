//! LAN signaling (and the WAN rendezvous client in WAN-07).
//!
//! Not implemented in BOOT-01: this build opens no listener and makes no network
//! connection.
#![forbid(unsafe_code)]

/// Descriptor for this crate.
pub const MODULE: tp_contracts::module::ModuleDescriptor = tp_contracts::module::ModuleDescriptor {
    crate_name: "tp-signaling",
    role: "signaling",
    state: tp_contracts::module::ModuleState::Foundation,
    owner_phase: "NET-03",
};
