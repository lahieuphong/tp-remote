//! Session lifecycle (ARCH-00 §6, clarified by ADR-003).
//!
//! IMPLEMENTED: the deterministic 8-state [`SessionMachine`], its timers, ordered cleanup
//! effects and typed rejections; the [`SessionRegistry`] that enforces one active session
//! per device and rejects events of other sessions; a thread-safe [`SharedRegistry`].
//!
//! NOT IMPLEMENTED: anything that performs I/O. The machine only *describes* what must
//! happen next ([`Effect`]); no network, capture, prompt or input exists in this crate.
//! Production session identifiers need a CSPRNG source (FUTURE: NET-03).
#![forbid(unsafe_code)]

mod machine;
mod registry;
mod shared;

pub use machine::{
    Cause, ConsentOrigin, Effect, Event, RejectReason, SessionMachine, Timeouts, TimerKind,
    Transition, TransitionError,
};
#[cfg(any(test, feature = "test-support"))]
pub use registry::SequentialIds;
pub use registry::{
    AUDIT_CAPACITY, AuditRecord, AuditVerdict, RegistryError, SessionIdSource, SessionRegistry,
    StopOutcome,
};
pub use shared::SharedRegistry;

/// Descriptor for this crate.
pub const MODULE: tp_contracts::module::ModuleDescriptor = tp_contracts::module::ModuleDescriptor {
    crate_name: "tp-session",
    role: "session-manager",
    state: tp_contracts::module::ModuleState::Partial,
    owner_phase: "NET-03",
};
