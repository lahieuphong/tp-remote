//! Security foundation (ARCH-00 §4.7, THREAT-MODEL §5–§6).
//!
//! IMPLEMENTED:
//! - [`AuthenticatedPeer`] and [`LocalConsent`]: proof types with private fields. Outside
//!   this crate they can be built only via the `test-support` feature.
//! - [`ConsentGrant`]: what a local `Allow` decision grants one authenticated peer for one
//!   session. It can be revoked and it expires; its scope can never be widened.
//! - [`authorize`]: pure, default-deny evaluation of a capability request.
//!
//! FUTURE (not in this crate yet):
//! - the PAKE handshake that produces `AuthenticatedPeer` (NET-03, U-03);
//! - the trusted local consent surface that produces `LocalConsent` (NET-03, U-06);
//! - OS permission checks (CAP-02) and the device-identity keystore (NET-03, U-16).
//!
//! A passing policy test proves the policy logic only. It does not prove that a future
//! WebRTC, IPC or OS adapter calls the policy; that must be tested in those phases.

mod authn;
mod consent;
mod policy;

pub use authn::AuthenticatedPeer;
pub use consent::{ConsentDecision, ConsentGrant, LocalConsent};
pub use policy::{AuthorizationContext, Decision, DenyReason, Permit, authorize};

/// Descriptor for this crate.
pub const MODULE: tp_contracts::module::ModuleDescriptor = tp_contracts::module::ModuleDescriptor {
    crate_name: "tp-security",
    role: "security-consent",
    state: tp_contracts::module::ModuleState::Partial,
    owner_phase: "NET-03",
};

#[cfg(test)]
mod tests;
