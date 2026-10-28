//! Test-only tooling for TP Remote. This crate is never a dependency of the desktop app
//! (enforced by `tests/workspace_boundaries.rs`).
//!
//! - [`bindings`]: generates and drift-checks the TypeScript bindings of the IPC DTOs.
#![forbid(unsafe_code)]

pub mod bindings;
