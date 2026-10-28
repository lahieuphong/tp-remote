//! Screen capture contract (ARCH-00 §4.2, amended in CAP-02A by ADR-007).
//!
//! IMPLEMENTED in CAP-02A: Windows.Graphics.Capture in `tp-capture` (Windows only).
//! FUTURE: ScreenCaptureKit (macOS), CAP-02B.
//!
//! Ownership and threading:
//! - The provider owns OS capture objects. [`CaptureProvider::start`] returns a stream
//!   handle owned by the caller; dropping or [`CaptureStream::stop`]ping it ends capture.
//! - Frames are pushed to the sink from a provider-owned thread. The sink must not block
//!   (latest-wins slot, ARCH-00 §5.2), must not panic and must not call `stop` itself.
//! - After `stop` returns, the sink receives nothing more from that stream.
//! - `start` must only be called after authorization; `tp-core` enforces this, the
//!   provider does not.
//!
//! Targets (ADR-007): what to capture is an opaque, provider-specific
//! [`CaptureProvider::Target`]. In CAP-02A the only production source of a target is the
//! operating system's own picker ([`CapturePicker`]), operated by the local user. The UI
//! cannot name or forge a target.

use std::sync::Arc;

use crate::capability::Availability;
use crate::error::{CoreError, ErrorCategory};
use crate::time::MonoTime;

/// Index of a display within one [`DisplayLayout`]. Valid only together with that
/// layout's `epoch`; it is not a stable hardware identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DisplayId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PixelSize {
    pub width: u32,
    pub height: u32,
}

/// Rectangle in the OS coordinate space: physical pixels of the virtual desktop on
/// Windows, global points on macOS (ARCH-00 §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RectI32 {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayInfo {
    pub id: DisplayId,
    /// OS display name, sanitized before it leaves the provider.
    pub name: String,
    pub bounds_os: RectI32,
    pub size_px: PixelSize,
    /// Pixels per OS unit × 1000 (1000 on Windows Per-Monitor-V2, 2000 on Retina).
    pub scale_milli: u32,
    /// Effective DPI scaling in percent (100 = 96 DPI on Windows), or `None` if the OS
    /// did not report it.
    pub scale_percent: Option<u32>,
    /// Current refresh rate in hertz, or `None` if the OS did not report it.
    pub refresh_hz: Option<u32>,
    pub is_primary: bool,
}

/// Display arrangement. `epoch` increases on every observed change so stale input
/// coordinates and display ids can be rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayLayout {
    pub epoch: u32,
    pub displays: Vec<DisplayInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    Bgra8,
    Nv12,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CaptureConfig {
    /// Upper bound on delivered frames per second; 0 means no limit requested. The OS
    /// may deliver fewer frames (WGC delivers frames only when content changes).
    pub max_fps: u32,
    pub format: PixelFormat,
    pub show_cursor: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureCapabilities {
    pub availability: Availability,
    pub max_fps: u32,
    /// Frames are GPU surfaces (IOSurface / D3D11 texture) rather than CPU buffers.
    pub gpu_frames: bool,
    /// The OS provides its own target picker (the trusted selection boundary).
    pub system_picker: bool,
}

/// What a capture target is, as far as the provider can establish it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetKind {
    /// A whole display, matched to an entry of the current [`DisplayLayout`].
    Display,
    /// A target the provider could not classify (on Windows: usually an application
    /// window). Treated with the same privacy and lifecycle rules as a display.
    Unknown,
}

impl TargetKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            TargetKind::Display => "display",
            TargetKind::Unknown => "unknown",
        }
    }
}

/// Description of a running capture's target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetInfo {
    pub kind: TargetKind,
    /// Sanitized OS name of the target. For windows this is the window title, which can
    /// be private: it may be shown to the local user, but must never be logged or sent
    /// to a peer.
    pub label: String,
    /// Size of the target when capture started.
    pub size: PixelSize,
    /// The matching display, when `kind` is `Display`.
    pub display: Option<DisplayId>,
}

/// Metadata of one captured frame. The pixels themselves are the provider's
/// [`CaptureProvider::Frame`] and are never logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameMeta {
    /// Provider-assigned number of the capture run (`start` call) this frame belongs to.
    pub generation: u64,
    /// 1 for the first frame of a run, then increasing by one per delivered frame.
    pub seq: u64,
    /// When the OS produced the frame, in the provider's monotonic time base (on Windows
    /// the frame's QPC timestamp). Comparable between frames of one provider only.
    pub captured_at: MonoTime,
    /// Size of the valid content of the frame.
    pub size: PixelSize,
    pub format: PixelFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureEvent {
    LayoutChanged {
        epoch: u32,
    },
    /// The target's content size changed; later frames have the new size.
    TargetResized(PixelSize),
    /// The target went away (window closed, display disconnected). No more frames.
    TargetClosed,
    /// The graphics device was removed or reset. No more frames from this run.
    DeviceLost,
    PermissionRevoked,
    Stopped,
    Failed(ErrorCategory),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CaptureStats {
    pub frames_delivered: u64,
    /// Frames the provider received from the OS but did not deliver (for example after
    /// stop began). Frames the OS itself skipped are not observable and not counted.
    pub frames_dropped: u64,
}

/// Receives frames from a capture thread.
pub trait FrameSink<F>: Send + Sync {
    fn on_frame(&self, meta: FrameMeta, frame: F);
    fn on_event(&self, event: CaptureEvent);
}

/// Frames whose pixels can be copied into a CPU buffer (local preview).
pub trait FrameReadback {
    /// Appends the frame's pixels to `out` as tightly packed RGBA8 rows, scaled down
    /// (aspect ratio kept) to fit within `max`. Never scales up. Returns the size written.
    fn read_rgba(&self, max: PixelSize, out: &mut Vec<u8>) -> Result<PixelSize, CoreError>;
}

pub trait CaptureProvider {
    /// Platform frame payload (CPU buffer or GPU surface handle).
    type Frame: Send + 'static;
    /// What to capture: an opaque platform object. See the module docs.
    type Target: Send + 'static;
    type Stream: CaptureStream;

    /// Advertises what this provider can do; `NotImplemented` / `Unavailable` must be
    /// reported honestly.
    fn capabilities(&self) -> CaptureCapabilities;

    fn displays(&self) -> Result<DisplayLayout, CoreError>;

    /// Starts capturing one target. Fails with `PlatformUnavailable`,
    /// `PermissionDenied` or `UnsupportedCapability` instead of returning a dummy stream.
    fn start<S>(
        &self,
        target: Self::Target,
        config: CaptureConfig,
        sink: Arc<S>,
    ) -> Result<Self::Stream, CoreError>
    where
        S: FrameSink<Self::Frame> + 'static;
}

/// Native window that owns a system picker: an `HWND` on Windows, an `NSWindow`
/// pointer on macOS. Providers must verify it before use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickerOwner(pub isize);

/// Result of one system-picker invocation.
#[derive(Debug)]
pub enum PickOutcome<T> {
    /// The local user chose a target in the OS picker.
    Selected(T),
    /// The user closed the picker without choosing anything.
    Cancelled,
    Failed(CoreError),
}

/// Completion callback of [`CapturePicker::pick`]. Called exactly once, from any thread.
pub type PickDone<T> = Box<dyn FnOnce(PickOutcome<T>) + Send + 'static>;

/// The operating system's own capture-target picker: the trusted local selection
/// boundary (ADR-007). Only the OS draws it and only the local user can operate it.
pub trait CapturePicker: CaptureProvider {
    /// Shows the picker over `owner` without blocking. Must be called on the owner
    /// window's UI thread. If this returns `Ok`, `done` is called exactly once later;
    /// if it returns `Err`, `done` is never called.
    fn pick(&self, owner: PickerOwner, done: PickDone<Self::Target>) -> Result<(), CoreError>;
}

/// A running capture. Dropping it must stop capture as a last resort.
pub trait CaptureStream: Send {
    fn target(&self) -> &TargetInfo;

    /// The `FrameMeta::generation` of this run.
    fn generation(&self) -> u64;

    fn reconfigure(&mut self, config: CaptureConfig) -> Result<(), CoreError>;

    /// Stops capture and waits (bounded) for in-flight frame callbacks to finish. After
    /// it returns the sink receives nothing more. A second stop via `Drop` is harmless.
    fn stop(self) -> Result<CaptureStats, CoreError>;
}
