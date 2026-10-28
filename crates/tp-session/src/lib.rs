//! Session manager and the 8-state session machine (ARCH-00 §6).
//!
//! Not implemented in BOOT-01: this build cannot create, accept or run a session.
#![forbid(unsafe_code)]

/// Descriptor for this crate.
pub const MODULE: tp_contracts::module::ModuleDescriptor = tp_contracts::module::ModuleDescriptor {
    crate_name: "tp-session",
    role: "session-manager",
    state: tp_contracts::module::ModuleState::Foundation,
    owner_phase: "NET-03",
};
