//! Systems without a capture implementation in this build (macOS until CAP-02B).
//! Everything reports `NotImplemented` or fails; no stream can ever be created, because
//! the target type has no values.
#![forbid(unsafe_code)]

use std::sync::Arc;

use tp_contracts::capability::Availability;
use tp_contracts::capture::{
    CaptureCapabilities, CaptureConfig, CapturePicker, CaptureProvider, CaptureStats,
    CaptureStream, DisplayLayout, FrameReadback, FrameSink, PickDone, PickerOwner, PixelSize,
    TargetInfo,
};
use tp_contracts::error::{CoreError, ErrorCategory};

const NOT_IMPLEMENTED: CoreError = CoreError::new(
    ErrorCategory::PlatformUnavailable,
    "screen capture is not implemented for this operating system",
);

/// Uninhabited: no frame can exist.
pub enum NoFrame {}
/// Uninhabited: no target can exist.
pub enum NoTarget {}
/// Uninhabited: no stream can exist.
pub enum NoStream {}

#[derive(Debug, Default)]
pub struct Provider;

impl Provider {
    pub fn new() -> Provider {
        Provider
    }
}

impl FrameReadback for NoFrame {
    fn read_rgba(&self, _max: PixelSize, _out: &mut Vec<u8>) -> Result<PixelSize, CoreError> {
        match *self {}
    }
}

impl CaptureStream for NoStream {
    fn target(&self) -> &TargetInfo {
        match *self {}
    }
    fn generation(&self) -> u64 {
        match *self {}
    }
    fn reconfigure(&mut self, _config: CaptureConfig) -> Result<(), CoreError> {
        match *self {}
    }
    fn stop(self) -> Result<CaptureStats, CoreError> {
        match self {}
    }
}

impl CaptureProvider for Provider {
    type Frame = NoFrame;
    type Target = NoTarget;
    type Stream = NoStream;

    fn capabilities(&self) -> CaptureCapabilities {
        CaptureCapabilities {
            availability: Availability::NotImplemented,
            max_fps: 0,
            gpu_frames: false,
            system_picker: false,
        }
    }

    fn displays(&self) -> Result<DisplayLayout, CoreError> {
        Err(NOT_IMPLEMENTED)
    }

    fn start<S>(
        &self,
        target: NoTarget,
        _config: CaptureConfig,
        _sink: Arc<S>,
    ) -> Result<NoStream, CoreError>
    where
        S: FrameSink<NoFrame> + 'static,
    {
        match target {}
    }
}

impl CapturePicker for Provider {
    fn pick(&self, _owner: PickerOwner, _done: PickDone<NoTarget>) -> Result<(), CoreError> {
        Err(NOT_IMPLEMENTED)
    }
}
