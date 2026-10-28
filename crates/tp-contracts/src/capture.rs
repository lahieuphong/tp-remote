//! Screen capture contract (ARCH-00 §4.2). FUTURE implementations: ScreenCaptureKit
//! (macOS) and Windows.Graphics.Capture (Windows) in `tp-capture`, CAP-02.
//!
//! Ownership and threading:
//! - The provider owns OS capture objects. [`CaptureProvider::start`] returns a stream
//!   handle owned by the caller; dropping or [`CaptureStream::stop`]ping it ends capture.
//! - Frames are pushed to the sink from a provider-owned thread. The sink must not block
//!   (latest-wins slot, ARCH-00 §5.2) and must not panic.
//! - `start` must only be called after authorization; `tp-core` enforces this, the
//!   provider does not.

use std::sync::Arc;

use crate::capability::Availability;
use crate::error::{CoreError, ErrorCategory};
use crate::time::MonoTime;

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
    pub is_primary: bool,
}

/// Display arrangement. `epoch` increases on every change so stale input coordinates can
/// be rejected.
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
    pub display: DisplayId,
    pub output_size: PixelSize,
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
}

/// Metadata of one captured frame. The pixels themselves are the provider's
/// [`CaptureProvider::Frame`] and are never logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameMeta {
    pub display: DisplayId,
    pub seq: u64,
    pub captured_at: MonoTime,
    pub size: PixelSize,
    pub format: PixelFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureEvent {
    LayoutChanged { epoch: u32 },
    PermissionRevoked,
    Stopped,
    Failed(ErrorCategory),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CaptureStats {
    pub frames_delivered: u64,
    pub frames_dropped: u64,
}

/// Receives frames from a capture thread.
pub trait FrameSink<F>: Send + Sync {
    fn on_frame(&self, meta: FrameMeta, frame: F);
    fn on_event(&self, event: CaptureEvent);
}

pub trait CaptureProvider {
    /// Platform frame payload (CPU buffer or GPU surface handle).
    type Frame: Send + 'static;
    type Stream: CaptureStream;

    /// Advertises what this provider can do; `NotImplemented` / `Unavailable` must be
    /// reported honestly.
    fn capabilities(&self) -> CaptureCapabilities;

    fn displays(&self) -> Result<DisplayLayout, CoreError>;

    /// Starts capturing one display. Fails with `PlatformUnavailable`,
    /// `PermissionDenied` or `UnsupportedCapability` instead of returning a dummy stream.
    fn start<S>(&self, config: CaptureConfig, sink: Arc<S>) -> Result<Self::Stream, CoreError>
    where
        S: FrameSink<Self::Frame> + 'static;
}

/// A running capture. Dropping it must stop capture as a last resort.
pub trait CaptureStream: Send {
    fn reconfigure(&mut self, config: CaptureConfig) -> Result<(), CoreError>;

    /// Stops capture and waits (bounded) for the capture thread to finish. Idempotent
    /// with respect to the OS: a second stop via `Drop` must be harmless.
    fn stop(self) -> Result<CaptureStats, CoreError>;
}
