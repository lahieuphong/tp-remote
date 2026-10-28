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
    /// The request does not fit the current state (for example a second preview start).
    InvalidState,
    /// The feature does not exist on this system or in this build.
    Unavailable,
    /// The caller may not perform this action (for example a window other than `main`).
    Forbidden,
}

// ---------------------------------------------------------------------------------------
// Local self-preview (CAP-02A, ADR-007). Local only: nothing here is sent to a peer.
// ---------------------------------------------------------------------------------------

/// Response of `get_capture_displays`: the monitors Windows reports right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureDisplays {
    /// Increases whenever the observed layout changes. Indexes are valid only within
    /// one epoch; they are not stable hardware identities.
    pub epoch: u32,
    pub displays: Vec<CaptureDisplay>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureDisplay {
    pub index: u32,
    /// Sanitized OS device name, e.g. `DISPLAY1`.
    pub name: String,
    /// Virtual-desktop position in physical pixels; may be negative.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_percent: Option<u32>,
    pub refresh_hz: Option<u32>,
    pub primary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum CaptureAvailability {
    Available,
    /// No capture implementation exists for this operating system in this build.
    NotImplemented,
    /// An implementation exists, but this system does not support it.
    Unsupported,
}

/// Local preview lifecycle, separate from the remote session state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum LocalPreviewState {
    NotAvailable,
    Ready,
    /// The Windows capture picker is open; nothing is being captured.
    SelectingTarget,
    Initializing,
    Capturing,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum PreviewTargetKind {
    Display,
    /// Not matched to a display; usually an application window.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewTarget {
    pub kind: PreviewTargetKind,
    /// Sanitized OS name. For a window this is its title: shown locally, never logged.
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub display_index: Option<u32>,
}

/// Measured values only. Rates are computed from observed timestamps; zero means
/// "not enough samples yet", never an estimate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewStats {
    /// Frames delivered by Windows to TP Remote in this run.
    pub captured_frames: u32,
    /// Frames copied into the preview buffer.
    pub previewed_frames: u32,
    /// Frames not previewed because the UI had not yet taken the previous one.
    pub skipped_frames: u32,
    /// Frames the provider received but discarded.
    pub dropped_frames: u32,
    /// Frames received from Windows during the last second, measured on the monotonic
    /// clock (native capture rate; independent of what the UI draws).
    pub capture_fps: f64,
    /// Rate at which preview frames were handed to the UI over the last second.
    pub preview_fps: f64,
    pub elapsed_ms: u32,
    /// Time from the user's selection to the first captured frame.
    pub first_frame_ms: Option<u32>,
    pub frame_width: u32,
    pub frame_height: u32,
    pub preview_width: u32,
    pub preview_height: u32,
}

/// Why the last preview run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum PreviewOutcome {
    /// The picker was closed without a selection; nothing was captured.
    Cancelled,
    StoppedByUser,
    TargetClosed,
    DeviceLost,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewError {
    /// Fixed text; contains no OS error strings, paths or titles.
    pub message: String,
    pub recovery: String,
}

/// Response of `get_local_preview_status`, `start_local_preview` and
/// `stop_local_preview`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalPreviewStatus {
    pub availability: CaptureAvailability,
    pub state: LocalPreviewState,
    /// Number of the current or last preview request; frames carry it too.
    pub generation: u32,
    pub target: Option<PreviewTarget>,
    pub stats: PreviewStats,
    pub last_outcome: Option<PreviewOutcome>,
    pub error: Option<PreviewError>,
}

/// Binary layout of a `get_local_preview_frame` response: a 24-byte little-endian header
/// (`magic`, `generation: u32`, `seq: u32`, `width: u32`, `height: u32`, `reserved: u32`)
/// followed by `width × height × 4` RGBA bytes. `width == 0` means "no new frame".
pub mod preview_frame {
    pub const MAGIC: [u8; 4] = *b"TPF1";
    pub const HEADER_LEN: usize = 24;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Header {
        pub generation: u32,
        pub seq: u32,
        pub width: u32,
        pub height: u32,
    }

    impl Header {
        pub const EMPTY: Header = Header {
            generation: 0,
            seq: 0,
            width: 0,
            height: 0,
        };

        pub fn encode(&self) -> [u8; HEADER_LEN] {
            let mut out = [0u8; HEADER_LEN];
            out[0..4].copy_from_slice(&MAGIC);
            out[4..8].copy_from_slice(&self.generation.to_le_bytes());
            out[8..12].copy_from_slice(&self.seq.to_le_bytes());
            out[12..16].copy_from_slice(&self.width.to_le_bytes());
            out[16..20].copy_from_slice(&self.height.to_le_bytes());
            out
        }

        pub fn decode(bytes: &[u8]) -> Option<Header> {
            let field = |i: usize| -> Option<u32> {
                Some(u32::from_le_bytes(bytes.get(i..i + 4)?.try_into().ok()?))
            };
            if bytes.get(0..4)? != MAGIC {
                return None;
            }
            Some(Header {
                generation: field(4)?,
                seq: field(8)?,
                width: field(12)?,
                height: field(16)?,
            })
        }

        /// Expected total response length for this header.
        pub fn frame_len(&self) -> Option<usize> {
            let pixels = (self.width as usize)
                .checked_mul(self.height as usize)?
                .checked_mul(4)?;
            HEADER_LEN.checked_add(pixels)
        }
    }
}
