//! Consent broker, authorization policy, device identity, PAKE handshake and OS
//! permission checks (ARCH-00 §4.7, THREAT-MODEL §6).
//!
//! Not implemented in BOOT-01: nothing in this build authenticates a peer, grants
//! consent or requests an OS permission. OS permission checks arrive in CAP-02; the
//! rest arrives in NET-03.

/// Descriptor for this crate.
pub const MODULE: tp_contracts::module::ModuleDescriptor = tp_contracts::module::ModuleDescriptor {
    crate_name: "tp-security",
    role: "security-consent",
    state: tp_contracts::module::ModuleState::Foundation,
    owner_phase: "NET-03",
};
