//! Engine module descriptors.

use serde::{Deserialize, Serialize};

/// Implementation state of an engine module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum ModuleState {
    /// The crate exists and compiles, but provides no runtime capability yet.
    Foundation,
    /// Some real functionality exists; the module is not feature-complete.
    Partial,
}

/// Static description of an engine crate, declared by the crate itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleDescriptor {
    /// Cargo package name, e.g. `tp-capture`.
    pub crate_name: &'static str,
    /// Conceptual module from ARCH-00 §3.3, e.g. `platform-capture`.
    pub role: &'static str,
    pub state: ModuleState,
    /// Roadmap phase that owns the module's next real capability.
    pub owner_phase: &'static str,
}
