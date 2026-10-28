//! Screen capture providers behind the `CaptureProvider` contract (ARCH-00 §4.2,
//! ADR-007).
//!
//! IMPLEMENTED (CAP-02A): Windows.Graphics.Capture, in [`wgc`] (Windows only): the
//! system capture picker, monitor enumeration, Direct3D 11 frames and a CPU readback for
//! the local preview. Tested on Windows 11 build 26100 only.
//!
//! NOT IMPLEMENTED: macOS ScreenCaptureKit (CAP-02B). On other systems
//! [`native::Provider`] reports `NotImplemented` and fails every call.
//!
//! Unsafe code is confined to the `wgc` module (Direct3D 11 / WinRT interop); every block
//! carries a `SAFETY:` comment. This crate never writes frames to disk and never logs
//! pixels.

pub mod scale;

#[cfg(windows)]
pub mod wgc;

#[cfg(not(windows))]
pub mod unsupported;

/// The capture provider for the operating system this build targets.
pub mod native {
    #[cfg(not(windows))]
    pub use crate::unsupported::{
        NoFrame as Frame, NoStream as Stream, NoTarget as Target, Provider,
    };
    #[cfg(windows)]
    pub use crate::wgc::{
        WgcFrame as Frame, WgcProvider as Provider, WgcStream as Stream, WgcTarget as Target,
    };
}

/// Descriptor for this crate.
pub const MODULE: tp_contracts::module::ModuleDescriptor = tp_contracts::module::ModuleDescriptor {
    crate_name: "tp-capture",
    role: "platform-capture",
    state: if cfg!(windows) {
        tp_contracts::module::ModuleState::Partial
    } else {
        tp_contracts::module::ModuleState::Foundation
    },
    owner_phase: "CAP-02B",
};
