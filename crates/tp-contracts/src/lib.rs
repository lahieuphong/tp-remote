//! Shared contracts for the TP Remote engine.
//!
//! This crate has no internal dependencies (ARCH-00 §3.3). It holds:
//! - [`ipc`]: DTOs exchanged between the Rust shell and the React UI. TypeScript
//!   bindings for these types are generated with `ts-rs` and must not be edited by hand.
//! - [`module`]: descriptors each engine crate uses to report its implementation state.
//! - Shared primitives: [`id`], [`time`], [`secret`], [`error`], [`capability`], [`cancel`].
//! - The engine contracts of ARCH-00 §4: [`session`], [`capture`], [`input`], [`codec`],
//!   [`transport`], [`permission`], [`identity`]. See docs/architecture/BOOT-02-contract-map.md.
//!
//! Interface availability is not implementation availability. These traits exist, but no
//! production implementation of capture, input, codec, transport, OS permissions or
//! device identity exists yet; the app reports [`capability::PlatformCapabilities::NONE`].
//!
//! UI IPC contracts are deliberately separate from the remote network protocol, which
//! lives in `tp-protocol`.
#![forbid(unsafe_code)]

pub mod cancel;
pub mod capability;
pub mod capture;
pub mod codec;
pub mod error;
pub mod id;
pub mod identity;
pub mod input;
pub mod ipc;
pub mod module;
pub mod permission;
pub mod secret;
pub mod session;
pub mod time;
pub mod transport;

/// Descriptor for this crate.
pub const MODULE: module::ModuleDescriptor = module::ModuleDescriptor {
    crate_name: "tp-contracts",
    role: "contracts",
    state: module::ModuleState::Partial,
    owner_phase: "CAP-02",
};
