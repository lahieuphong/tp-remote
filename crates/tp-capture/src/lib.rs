//! Screen capture providers: ScreenCaptureKit (macOS) and Windows.Graphics.Capture
//! (Windows), behind the CaptureProvider contract (ARCH-00 §4.2).
//!
//! Not implemented in BOOT-01: this build cannot capture the screen.

/// Descriptor for this crate.
pub const MODULE: tp_contracts::module::ModuleDescriptor = tp_contracts::module::ModuleDescriptor {
    crate_name: "tp-capture",
    role: "platform-capture",
    state: tp_contracts::module::ModuleState::Foundation,
    owner_phase: "CAP-02",
};
