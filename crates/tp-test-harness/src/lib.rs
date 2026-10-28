//! Test-only tooling for TP Remote. This crate is never a dependency of the desktop app
//! (enforced by `tests/workspace_boundaries.rs`).
//!
//! - [`bindings`]: generates and drift-checks the TypeScript bindings of the IPC DTOs.
//! - [`capture`] (Windows): helpers for real-device capture tests and `capture-bench`.
#![forbid(unsafe_code)]

pub mod bindings;
#[cfg(windows)]
pub mod capture;
