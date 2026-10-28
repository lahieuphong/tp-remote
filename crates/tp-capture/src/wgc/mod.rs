//! Windows.Graphics.Capture provider (CAP-02A, ADR-007).
//!
//! Pipeline: the system picker (`GraphicsCapturePicker`, the trusted local selection
//! boundary) → `GraphicsCaptureItem` → `Direct3D11CaptureFramePool::CreateFreeThreaded`
//! (BGRA8, 2 buffers) → `GraphicsCaptureSession`. Frames arrive on a WGC thread-pool
//! thread and go to the sink as [`WgcFrame`]s: GPU textures, not copied unless the sink
//! asks for a CPU readback.
//!
//! The yellow capture border is the operating system's own capture indicator. This
//! module never changes the session's border setting and never requests borderless
//! capture access.
//!
//! Threading:
//! - [`CapturePicker::pick`] runs on the owner window's UI thread (the Tauri main
//!   thread) and returns at once; the picker's completion arrives on a WinRT thread.
//! - `start` and `stop` may run on any thread. `start` creates its own Direct3D 11
//!   device; the free-threaded frame pool needs no dispatcher queue.
//! - Frame and target-closed callbacks run on WGC threads, inside `catch_unwind`.
#![allow(unsafe_code)]

mod d3d;
mod monitors;
mod picker;
mod stream;
mod target;
#[cfg(feature = "test-support")]
pub mod testing;

use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tp_contracts::capability::{Availability, UnavailableReason};
use tp_contracts::capture::{
    CaptureCapabilities, CaptureConfig, CapturePicker, CaptureProvider, DisplayInfo, DisplayLayout,
    FrameSink, PickDone, PickerOwner,
};
use tp_contracts::error::{CoreError, ErrorCategory};
use windows::Graphics::Capture::GraphicsCaptureSession;

pub use stream::{StreamDiagnostics, WgcFrame, WgcStream};
pub use target::{TargetOrigin, WgcTarget};

/// HRESULT of the most recent Windows API failure (diagnostics only, never shown to users).
static LAST_HRESULT: AtomicI32 = AtomicI32::new(0);

/// Maps a Windows error to a [`CoreError`] with fixed context, remembering its HRESULT.
pub(crate) fn os_error(
    category: ErrorCategory,
    context: &'static str,
) -> impl FnOnce(windows::core::Error) -> CoreError {
    move |error| {
        LAST_HRESULT.store(error.code().0, Ordering::Relaxed);
        CoreError::new(category, context)
    }
}

/// HRESULT of the most recent Windows API failure seen by this crate, for diagnostics.
pub fn last_os_error_code() -> Option<i32> {
    match LAST_HRESULT.load(Ordering::Relaxed) {
        0 => None,
        code => Some(code),
    }
}

/// Whether this system supports Windows.Graphics.Capture.
pub fn is_supported() -> bool {
    GraphicsCaptureSession::IsSupported().unwrap_or(false)
}

/// The Windows.Graphics.Capture provider.
#[derive(Debug, Default)]
pub struct WgcProvider {
    layout: Mutex<monitors::LayoutTracker>,
    next_generation: AtomicU64,
    /// WGC monitor names for the last seen layout (creating the items to read them costs
    /// one process handle each, so they are not re-read per start).
    monitor_names: Mutex<Option<NameCache>>,
}

/// A layout and the WGC names of its monitors, in the same order.
type NameCache = (Vec<DisplayInfo>, Vec<Option<String>>);

impl WgcProvider {
    pub fn new() -> WgcProvider {
        WgcProvider::default()
    }

    fn monitor_names(&self, monitors: &[monitors::MonitorRecord]) -> Vec<Option<String>> {
        let key: Vec<DisplayInfo> = monitors.iter().map(|m| m.info.clone()).collect();
        let mut cache = self
            .monitor_names
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some((cached_key, names)) = cache.as_ref()
            && *cached_key == key
        {
            return names.clone();
        }
        let names = target::wgc_monitor_names(monitors);
        *cache = Some((key, names.clone()));
        names
    }
}

impl CaptureProvider for WgcProvider {
    type Frame = WgcFrame;
    type Target = WgcTarget;
    type Stream = WgcStream;

    fn capabilities(&self) -> CaptureCapabilities {
        let availability = if is_supported() {
            Availability::Available
        } else {
            Availability::Unavailable(UnavailableReason::PlatformUnsupported)
        };
        // WGC delivers at most one frame per display refresh.
        let max_fps = monitors::enumerate()
            .ok()
            .and_then(|m| m.iter().filter_map(|r| r.info.refresh_hz).max())
            .unwrap_or(0);
        CaptureCapabilities {
            availability,
            max_fps,
            gpu_frames: true,
            system_picker: true,
        }
    }

    fn displays(&self) -> Result<DisplayLayout, CoreError> {
        let displays: Vec<_> = monitors::enumerate()?
            .into_iter()
            .map(|record| record.info)
            .collect();
        let epoch = self
            .layout
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .observe(&displays);
        Ok(DisplayLayout { epoch, displays })
    }

    fn start<S>(
        &self,
        target: WgcTarget,
        config: CaptureConfig,
        sink: Arc<S>,
    ) -> Result<WgcStream, CoreError>
    where
        S: FrameSink<WgcFrame> + 'static,
    {
        if !is_supported() {
            return Err(CoreError::new(
                ErrorCategory::PlatformUnavailable,
                "Windows.Graphics.Capture is not supported on this system",
            ));
        }
        let monitors = monitors::enumerate().unwrap_or_default();
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let names = self.monitor_names(&monitors);
        stream::start(target, config, sink, generation, &monitors, &names)
    }
}

impl CapturePicker for WgcProvider {
    fn pick(&self, owner: PickerOwner, done: PickDone<WgcTarget>) -> Result<(), CoreError> {
        picker::pick(owner, done)
    }
}
