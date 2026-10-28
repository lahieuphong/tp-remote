//! Shared helpers for real-device capture tests and the capture bench (Windows only).
//!
//! Real-device code waits for OS events with bounded polling; it never sleeps for a
//! fixed time and calls it a result.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tp_capture::native::Frame;
use tp_contracts::capture::{CaptureEvent, FrameMeta, FrameReadback, FrameSink, PixelSize};

/// A sink that records metadata and events, optionally reads back one frame at a time,
/// and counts anything that arrives after it was told the stream stopped.
#[derive(Default)]
pub struct Recorder {
    metas: Mutex<Vec<FrameMeta>>,
    events: Mutex<Vec<CaptureEvent>>,
    readback: Mutex<Option<(PixelSize, Vec<u8>)>>,
    want_readback: AtomicBool,
    readback_max: Mutex<Option<PixelSize>>,
    readback_errors: AtomicU64,
    stopped: AtomicBool,
    after_stop: AtomicU64,
    first_frame_at: Mutex<Option<Instant>>,
    keep_metas: AtomicBool,
    count: AtomicU64,
    readbacks: AtomicU64,
    readback_us_total: AtomicU64,
    readback_us_max: AtomicU64,
    last_captured_at: Mutex<Option<tp_contracts::time::MonoTime>>,
}

/// Readback cost observed by a [`Recorder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadbackStats {
    pub count: u64,
    pub total_us: u64,
    pub max_us: u64,
}

impl Recorder {
    pub fn new() -> Arc<Recorder> {
        let recorder = Recorder::default();
        recorder.keep_metas.store(true, Ordering::Relaxed);
        Arc::new(recorder)
    }

    /// Records counts only (long runs keep memory flat).
    pub fn counting() -> Arc<Recorder> {
        Arc::new(Recorder::default())
    }

    /// Ask for the next frame to be copied to the CPU, scaled to fit within `max`.
    pub fn request_readback(&self, max: PixelSize) {
        *lock(&self.readback_max) = Some(max);
        self.want_readback.store(true, Ordering::SeqCst);
    }

    pub fn take_readback(&self) -> Option<(PixelSize, Vec<u8>)> {
        lock(&self.readback).take()
    }

    pub fn metas(&self) -> Vec<FrameMeta> {
        lock(&self.metas).clone()
    }

    pub fn events(&self) -> Vec<CaptureEvent> {
        lock(&self.events).clone()
    }

    pub fn frames(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }

    pub fn readback_errors(&self) -> u64 {
        self.readback_errors.load(Ordering::SeqCst)
    }

    pub fn first_frame_at(&self) -> Option<Instant> {
        *lock(&self.first_frame_at)
    }

    /// Marks the stream as stopped; later deliveries are counted as violations.
    pub fn mark_stopped(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    pub fn deliveries_after_stop(&self) -> u64 {
        self.after_stop.load(Ordering::SeqCst)
    }

    pub fn readback_stats(&self) -> ReadbackStats {
        ReadbackStats {
            count: self.readbacks.load(Ordering::SeqCst),
            total_us: self.readback_us_total.load(Ordering::SeqCst),
            max_us: self.readback_us_max.load(Ordering::SeqCst),
        }
    }

    /// OS timestamp of the most recent frame.
    pub fn last_captured_at(&self) -> Option<tp_contracts::time::MonoTime> {
        *lock(&self.last_captured_at)
    }
}

impl FrameSink<Frame> for Recorder {
    fn on_frame(&self, meta: FrameMeta, frame: Frame) {
        if self.stopped.load(Ordering::SeqCst) {
            self.after_stop.fetch_add(1, Ordering::SeqCst);
        }
        self.count.fetch_add(1, Ordering::SeqCst);
        lock(&self.first_frame_at).get_or_insert_with(Instant::now);
        *lock(&self.last_captured_at) = Some(meta.captured_at);
        if self.keep_metas.load(Ordering::Relaxed) {
            lock(&self.metas).push(meta);
        }
        if self.want_readback.swap(false, Ordering::SeqCst) {
            let max = lock(&self.readback_max).unwrap_or(meta.size);
            let mut pixels = Vec::new();
            let started = Instant::now();
            let result = frame.read_rgba(max, &mut pixels);
            let us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
            match result {
                Ok(size) => {
                    self.readbacks.fetch_add(1, Ordering::SeqCst);
                    self.readback_us_total.fetch_add(us, Ordering::SeqCst);
                    self.readback_us_max.fetch_max(us, Ordering::SeqCst);
                    *lock(&self.readback) = Some((size, pixels));
                }
                Err(_) => {
                    self.readback_errors.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
    }

    fn on_event(&self, event: CaptureEvent) {
        if self.stopped.load(Ordering::SeqCst) {
            self.after_stop.fetch_add(1, Ordering::SeqCst);
        }
        lock(&self.events).push(event);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Polls `condition` every 5 ms until it holds or `timeout` passes.
pub fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// RGBA pixel at `(x, y)` of a tightly packed buffer of `size`.
pub fn pixel(rgba: &[u8], size: PixelSize, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * size.width + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2]]
}

/// Whether two colours are within `tolerance` per channel.
pub fn close_to(actual: [u8; 3], expected: [u8; 3], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= tolerance)
}

/// Frame intervals from OS timestamps, in milliseconds.
pub fn intervals_ms(metas: &[FrameMeta]) -> Vec<f64> {
    metas
        .windows(2)
        .map(|w| {
            w[1].captured_at
                .saturating_duration_since(w[0].captured_at)
                .as_secs_f64()
                * 1000.0
        })
        .collect()
}
