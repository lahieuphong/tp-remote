//! Video codec contracts (ARCH-00 §4.6). FUTURE implementations in `tp-media`, VID-04.
//!
//! Encoders and decoders are synchronous and owned by one worker thread. The input and
//! output frame types are associated types, so a capture provider and an encoder are
//! paired at compile time without trait objects.

use std::fmt;

use crate::capability::Availability;
use crate::capture::{FrameMeta, PixelSize};
use crate::error::CoreError;
use crate::time::MonoTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codec {
    /// H.264 Constrained Baseline, Annex B, packetization-mode 1 (D-08).
    H264ConstrainedBaseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderConfig {
    pub codec: Codec,
    pub size: PixelSize,
    pub max_fps: u32,
    pub target_bitrate_bps: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecInfo {
    pub name: &'static str,
    pub hardware: bool,
    pub availability: Availability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EncodeOptions {
    pub force_keyframe: bool,
}

/// One encoded access unit. Its `Debug` shows the length, never the bitstream.
#[derive(Clone, PartialEq, Eq)]
pub struct EncodedFrame {
    pub seq: u64,
    pub captured_at: MonoTime,
    pub keyframe: bool,
    pub codec: Codec,
    pub bitstream: Vec<u8>,
}

impl fmt::Debug for EncodedFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedFrame")
            .field("seq", &self.seq)
            .field("captured_at", &self.captured_at)
            .field("keyframe", &self.keyframe)
            .field("codec", &self.codec)
            .field("bitstream_len", &self.bitstream.len())
            .finish()
    }
}

pub trait VideoEncoder: Send {
    /// Frame type accepted (the capture provider's frame type, or a converted form).
    type Input;

    fn info(&self) -> CodecInfo;

    fn configure(&mut self, config: &EncoderConfig) -> Result<(), CoreError>;

    /// Encodes one frame and appends 0..n access units to `out` (hardware encoders may
    /// emit asynchronously). `out` is caller-owned so buffers can be reused.
    fn encode(
        &mut self,
        meta: &FrameMeta,
        frame: &Self::Input,
        options: EncodeOptions,
        out: &mut Vec<EncodedFrame>,
    ) -> Result<(), CoreError>;

    fn set_rate(&mut self, bitrate_bps: u32, max_fps: u32) -> Result<(), CoreError>;

    /// Emits pending access units; called before teardown.
    fn flush(&mut self, out: &mut Vec<EncodedFrame>) -> Result<(), CoreError>;
}

pub trait VideoDecoder: Send {
    type Output;

    fn info(&self) -> CodecInfo;

    /// Decodes one access unit. Returns `InvalidPayload` when a keyframe is needed.
    fn decode(&mut self, frame: &EncodedFrame) -> Result<Option<Self::Output>, CoreError>;

    fn reset(&mut self);
}
