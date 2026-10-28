//! Video encoders and decoders (H.264), behind the VideoEncoder / VideoDecoder
//! contracts (ARCH-00 §4.6).
//!
//! Not implemented in BOOT-01: this build cannot encode or decode video.

/// Descriptor for this crate.
pub const MODULE: tp_contracts::module::ModuleDescriptor = tp_contracts::module::ModuleDescriptor {
    crate_name: "tp-media",
    role: "media-codec",
    state: tp_contracts::module::ModuleState::Foundation,
    owner_phase: "VID-04",
};
