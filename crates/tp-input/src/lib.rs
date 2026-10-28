//! Input injection: CGEvent (macOS) and SendInput (Windows), behind the InputController
//! contract (ARCH-00 §4.3).
//!
//! Not implemented in BOOT-01: this build cannot inject mouse or keyboard input.

/// Descriptor for this crate.
pub const MODULE: tp_contracts::module::ModuleDescriptor = tp_contracts::module::ModuleDescriptor {
    crate_name: "tp-input",
    role: "platform-input",
    state: tp_contracts::module::ModuleState::Foundation,
    owner_phase: "CTRL-05",
};
