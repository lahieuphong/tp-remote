//! DTOs for the Tauri IPC boundary between the Rust shell and the React UI.
//!
//! Field names are camelCase on the wire. Unknown fields are rejected when these
//! types are deserialized, so fixtures and future clients cannot drift silently.

use serde::{Deserialize, Serialize};

use crate::module::{ModuleDescriptor, ModuleState};

/// Response of the `get_app_info` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppInfo {
    pub app_name: String,
    pub app_version: String,
    /// Roadmap phase this build belongs to, e.g. `BOOT-01`.
    pub phase: String,
    pub platform: PlatformInfo,
    pub build_profile: BuildProfile,
    pub protocol: ProtocolSupport,
    pub shell: ShellInfo,
    pub remote_features: RemoteFeatures,
    pub engine_modules: Vec<EngineModuleStatus>,
}

/// Operating system and CPU architecture the binary was compiled for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformInfo {
    /// `std::env::consts::OS`, e.g. `windows`.
    pub os: String,
    /// `std::env::consts::FAMILY`, e.g. `windows`.
    pub family: String,
    /// `std::env::consts::ARCH`, e.g. `x86_64`.
    pub arch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum BuildProfile {
    Debug,
    Release,
}

/// Remote protocol versions this build understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProtocolSupport {
    pub current: u16,
    pub min_supported: u16,
    pub max_supported: u16,
}

/// Desktop shell details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShellInfo {
    pub tauri_version: String,
    /// WebView runtime version, or `null` if the shell could not read it.
    pub webview_version: Option<String>,
}

/// Availability of remote desktop features in this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum RemoteFeatures {
    /// No capture, control, signaling or streaming exists in this build.
    NotImplemented,
}

/// Implementation state of one engine crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineModuleStatus {
    pub crate_name: String,
    pub role: String,
    pub state: ModuleState,
    pub owner_phase: String,
}

impl From<ModuleDescriptor> for EngineModuleStatus {
    fn from(d: ModuleDescriptor) -> Self {
        Self {
            crate_name: d.crate_name.to_owned(),
            role: d.role.to_owned(),
            state: d.state,
            owner_phase: d.owner_phase.to_owned(),
        }
    }
}

/// Error returned by IPC commands. Messages are safe to show to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IpcError {
    pub code: IpcErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum IpcErrorCode {
    Internal,
    /// Debug builds only: a failure deliberately injected for testing.
    InjectedFault,
}
