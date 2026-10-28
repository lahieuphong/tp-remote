//! Shared contracts for the TP Remote engine.
//!
//! This crate has no internal dependencies (ARCH-00 §3.3). It holds:
//! - [`ipc`]: DTOs exchanged between the Rust shell and the React UI. TypeScript
//!   bindings for these types are generated with `ts-rs` and must not be edited by hand.
//! - [`module`]: descriptors each engine crate uses to report its implementation state.
//!
//! UI IPC contracts are deliberately separate from the remote network protocol, which
//! lives in `tp-protocol`.
//!
//! The capture, input, transport, codec, session and security traits from ARCH-00 §4
//! are not defined yet; they arrive with the phases that implement them.
#![forbid(unsafe_code)]

pub mod ipc;
pub mod module;

/// Descriptor for this crate.
pub const MODULE: module::ModuleDescriptor = module::ModuleDescriptor {
    crate_name: "tp-contracts",
    role: "contracts",
    state: module::ModuleState::Partial,
    owner_phase: "CAP-02",
};
