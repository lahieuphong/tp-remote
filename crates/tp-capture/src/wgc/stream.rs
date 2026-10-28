//! A running Windows.Graphics.Capture session.
//!
//! Delivery rule: every frame and event reaches the sink while holding `Shared::delivery`.
//! `stop` takes the same lock and clears the sink, so it waits for an in-flight callback
//! (bounded by one sink call, i.e. one preview readback) and nothing is delivered after
//! it returns. Callbacks never call back into the stream, so the lock cannot deadlock.
//!
//! Target loss: `GraphicsCaptureItem.Closed` is subscribed, but on DEV-W1 it was not
//! raised when a captured window was destroyed (docs/qa/evidence/CAP-02A/provider/
//! diagnostic-names-and-close.log). A watchdog thread therefore checks every 250 ms
//! whether the item still has a size; a destroyed window reports 0×0 (a minimized one
//! does not). Whichever notices first ends the run with `TargetClosed`.
//!
//! Latest wins at the source: the frame callback drains the pool, closing older queued
//! frames (counted as dropped and `superseded`), so the 2-buffer pool never stays full.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use tp_contracts::capture::{
    CaptureConfig, CaptureEvent, CaptureStats, CaptureStream, FrameMeta, FrameReadback, FrameSink,
    PixelFormat, PixelSize, TargetInfo,
};
use tp_contracts::error::{CoreError, ErrorCategory};
use tp_contracts::time::MonoTime;
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::core::{IInspectable, Ref};

use super::d3d::{DEVICE_LOST, Gpu};
use super::monitors::MonitorRecord;
use super::os_error;
use super::target::{self, WgcTarget};

/// Frame-pool buffers. Two keep the latest frame available while the sink reads one.
const BUFFERS: i32 = 2;
const WATCHDOG_INTERVAL: Duration = Duration::from_millis(250);
const FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;

type DynSink = Arc<dyn FrameSink<WgcFrame>>;

struct Delivery {
    sink: Option<DynSink>,
    /// Set once the run has ended (stopped, target closed, device lost, failure).
    ended: bool,
    seq: u64,
    pool_size: SizeInt32,
    delivered: u64,
    dropped: u64,
    /// Older queued frames discarded in favour of a newer one (part of `dropped`).
    superseded: u64,
}

/// Delivery counters of a run (diagnostics and QA evidence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamDiagnostics {
    pub delivered: u64,
    pub dropped: u64,
    pub superseded: u64,
}

struct Shared {
    delivery: Mutex<Delivery>,
    gpu: Arc<Gpu>,
    generation: u64,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Delivery> {
        self.delivery.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn on_frame_arrived(&self, pool: &Direct3D11CaptureFramePool) {
        let mut delivery = self.lock();
        self.take_and_deliver(&mut delivery, pool);
    }

    fn diagnostics(&self) -> StreamDiagnostics {
        let delivery = self.lock();
        StreamDiagnostics {
            delivered: delivery.delivered,
            dropped: delivery.dropped,
            superseded: delivery.superseded,
        }
    }

    /// Takes the newest queued frame (closing older ones) and hands it to the sink.
    fn take_and_deliver(
        &self,
        delivery: &mut MutexGuard<'_, Delivery>,
        pool: &Direct3D11CaptureFramePool,
    ) {
        let mut frame = match pool.TryGetNextFrame() {
            Ok(frame) => frame,
            Err(_) => {
                if self.gpu.is_lost() {
                    end_with(delivery, CaptureEvent::DeviceLost);
                }
                return;
            }
        };
        // Latest wins at the source: never leave frames queued in the 2-buffer pool.
        while let Ok(newer) = pool.TryGetNextFrame() {
            let _ = frame.Close();
            delivery.dropped += 1;
            delivery.superseded += 1;
            frame = newer;
        }
        let sink = match (&delivery.sink, delivery.ended) {
            (Some(sink), false) => sink.clone(),
            _ => {
                delivery.dropped += 1;
                let _ = frame.Close();
                return;
            }
        };
        let content = frame.ContentSize().unwrap_or(delivery.pool_size);
        let pool_size = delivery.pool_size;
        let size = PixelSize {
            width: u32::try_from(content.Width.min(pool_size.Width)).unwrap_or(0),
            height: u32::try_from(content.Height.min(pool_size.Height)).unwrap_or(0),
        };
        // SystemRelativeTime is the frame's QPC time in 100 ns units.
        let captured_at = frame
            .SystemRelativeTime()
            .map(|t| MonoTime::from_micros(u64::try_from(t.Duration / 10).unwrap_or(0)))
            .unwrap_or(MonoTime::ZERO);
        delivery.seq += 1;
        delivery.delivered += 1;
        let meta = FrameMeta {
            generation: self.generation,
            seq: delivery.seq,
            captured_at,
            size,
            format: PixelFormat::Bgra8,
        };
        sink.on_frame(
            meta,
            WgcFrame {
                frame: Some(frame),
                size,
                gpu: self.gpu.clone(),
            },
        );

        let resized = content.Width > 0
            && content.Height > 0
            && (content.Width != pool_size.Width || content.Height != pool_size.Height);
        if resized && !delivery.ended {
            let recreated = self.gpu.winrt_device().and_then(|device| {
                pool.Recreate(&device, FORMAT, BUFFERS, content)
                    .map_err(os_error(
                        ErrorCategory::PlatformUnavailable,
                        "Recreate failed",
                    ))
            });
            match recreated {
                Ok(()) => {
                    delivery.pool_size = content;
                    sink.on_event(CaptureEvent::TargetResized(PixelSize {
                        width: content.Width as u32,
                        height: content.Height as u32,
                    }));
                }
                Err(_) if self.gpu.is_lost() => end_with(delivery, CaptureEvent::DeviceLost),
                Err(error) => end_with(delivery, CaptureEvent::Failed(error.category())),
            }
        }
    }

    fn on_closed(&self) {
        end_with(&mut self.lock(), CaptureEvent::TargetClosed);
    }

    fn on_panic(&self) {
        end_with(
            &mut self.lock(),
            CaptureEvent::Failed(ErrorCategory::InternalError),
        );
    }
}

/// Ends the run once: the sink hears about it, later frames are dropped.
fn end_with(delivery: &mut Delivery, event: CaptureEvent) {
    if delivery.ended {
        return;
    }
    delivery.ended = true;
    if let Some(sink) = &delivery.sink {
        sink.on_event(event);
    }
}

/// One captured frame: a texture of the frame pool, returned to the pool on drop.
/// Sinks must not keep it (WGC requires frames to be released promptly).
pub struct WgcFrame {
    frame: Option<Direct3D11CaptureFrame>,
    size: PixelSize,
    gpu: Arc<Gpu>,
}

impl WgcFrame {
    pub fn size(&self) -> PixelSize {
        self.size
    }
}

impl FrameReadback for WgcFrame {
    fn read_rgba(&self, max: PixelSize, out: &mut Vec<u8>) -> Result<PixelSize, CoreError> {
        let frame = self
            .frame
            .as_ref()
            .ok_or_else(|| CoreError::new(ErrorCategory::InvalidState, "frame already released"))?;
        let surface = frame.Surface().map_err(|e| {
            if self.gpu.is_lost() {
                DEVICE_LOST
            } else {
                os_error(ErrorCategory::InternalError, "frame has no surface")(e)
            }
        })?;
        self.gpu.read_rgba(&surface, self.size, max, out)
    }
}

impl Drop for WgcFrame {
    fn drop(&mut self) {
        if let Some(frame) = self.frame.take() {
            let _ = frame.Close();
        }
    }
}

/// A running capture. `stop` (or drop) releases the session, the frame pool and the
/// capture item; the item cannot be reused afterwards.
pub struct WgcStream {
    shared: Arc<Shared>,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    item: GraphicsCaptureItem,
    frame_token: i64,
    closed_token: i64,
    info: TargetInfo,
    watchdog: Option<(mpsc::Sender<()>, JoinHandle<()>)>,
    stopped: bool,
}

/// Ends the run with `TargetClosed` once the item reports no size. Holds only a weak
/// reference; exits when the stream stops (sender dropped) or after reporting.
fn spawn_watchdog(
    shared: Weak<Shared>,
    item: GraphicsCaptureItem,
) -> Option<(mpsc::Sender<()>, JoinHandle<()>)> {
    let (stop, stopped) = mpsc::channel::<()>();
    let thread = std::thread::Builder::new()
        .name("tp-wgc-watchdog".into())
        .spawn(move || {
            while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(WATCHDOG_INTERVAL) {
                let Some(shared) = shared.upgrade() else {
                    break;
                };
                if target::item_size(&item).width == 0 {
                    let _ = catch_unwind(AssertUnwindSafe(|| shared.on_closed()));
                    break;
                }
            }
        })
        .ok()?;
    Some((stop, thread))
}

impl WgcStream {
    /// Description of the graphics adapter used for this run.
    pub fn adapter(&self) -> &str {
        self.shared.gpu.adapter()
    }

    /// Delivery counters so far (diagnostics; never includes pixel data).
    pub fn diagnostics(&self) -> StreamDiagnostics {
        self.shared.diagnostics()
    }

    /// The target's size as Windows reports it now (zero once the target is gone).
    pub fn current_target_size(&self) -> PixelSize {
        target::item_size(&self.item)
    }

    fn shutdown(&mut self) -> CaptureStats {
        let stats = {
            let mut delivery = self.shared.lock();
            delivery.sink = None;
            delivery.ended = true;
            CaptureStats {
                frames_delivered: delivery.delivered,
                frames_dropped: delivery.dropped,
            }
        };
        // The delivery lock is released before joining, so a watchdog that is reporting
        // right now can finish (it finds the run ended and does nothing).
        if let Some((stop, thread)) = self.watchdog.take() {
            drop(stop);
            let _ = thread.join();
        }
        if !self.stopped {
            self.stopped = true;
            let _ = self.pool.RemoveFrameArrived(self.frame_token);
            let _ = self.item.RemoveClosed(self.closed_token);
            let _ = self.session.Close();
            let _ = self.pool.Close();
        }
        stats
    }
}

impl CaptureStream for WgcStream {
    fn target(&self) -> &TargetInfo {
        &self.info
    }

    fn generation(&self) -> u64 {
        self.shared.generation
    }

    fn reconfigure(&mut self, config: CaptureConfig) -> Result<(), CoreError> {
        apply_config(&self.session, config);
        Ok(())
    }

    fn stop(mut self) -> Result<CaptureStats, CoreError> {
        Ok(self.shutdown())
    }
}

impl Drop for WgcStream {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn apply_config(session: &GraphicsCaptureSession, config: CaptureConfig) {
    // Optional session properties; older builds may not support them.
    let _ = session.SetIsCursorCaptureEnabled(config.show_cursor);
    if config.max_fps > 0 {
        let interval = TimeSpan {
            Duration: 10_000_000 / i64::from(config.max_fps),
        };
        let _ = session.SetMinUpdateInterval(interval);
    }
}

pub(crate) fn start<S>(
    target: WgcTarget,
    config: CaptureConfig,
    sink: Arc<S>,
    generation: u64,
    monitors: &[MonitorRecord],
    monitor_names: &[Option<String>],
) -> Result<WgcStream, CoreError>
where
    S: FrameSink<WgcFrame> + 'static,
{
    if config.format != PixelFormat::Bgra8 {
        return Err(CoreError::new(
            ErrorCategory::UnsupportedCapability,
            "only BGRA8 capture is implemented",
        ));
    }
    let item = target.item;
    let info = target::describe(&item, monitors, monitor_names);
    if info.size.width == 0 || info.size.height == 0 {
        return Err(CoreError::new(
            ErrorCategory::PlatformUnavailable,
            "the capture target has no size (it may have been closed)",
        ));
    }
    let size = item.Size().map_err(os_error(
        ErrorCategory::PlatformUnavailable,
        "target size unavailable",
    ))?;

    let gpu = Arc::new(Gpu::create()?);
    let device = gpu.winrt_device()?;
    let failed = || {
        os_error(
            ErrorCategory::PlatformUnavailable,
            "starting the capture session failed",
        )
    };
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&device, FORMAT, BUFFERS, size)
        .map_err(failed())?;
    let session = pool.CreateCaptureSession(&item).map_err(failed())?;
    apply_config(&session, config);

    let sink: DynSink = sink;
    let shared = Arc::new(Shared {
        delivery: Mutex::new(Delivery {
            sink: Some(sink),
            ended: false,
            seq: 0,
            pool_size: size,
            delivered: 0,
            dropped: 0,
            superseded: 0,
        }),
        gpu,
        generation,
    });

    let on_frame = Arc::downgrade(&shared);
    let frame_token = pool
        .FrameArrived(&TypedEventHandler::new(
            move |pool: Ref<Direct3D11CaptureFramePool>, _: Ref<IInspectable>| {
                if let (Some(shared), Some(pool)) = (on_frame.upgrade(), pool.as_ref())
                    && catch_unwind(AssertUnwindSafe(|| shared.on_frame_arrived(pool))).is_err()
                {
                    let _ = catch_unwind(AssertUnwindSafe(|| shared.on_panic()));
                }
                Ok(())
            },
        ))
        .map_err(failed())?;
    let on_closed = Arc::downgrade(&shared);
    let closed_token = item
        .Closed(&TypedEventHandler::new(
            move |_: Ref<GraphicsCaptureItem>, _: Ref<IInspectable>| {
                if let Some(shared) = on_closed.upgrade() {
                    let _ = catch_unwind(AssertUnwindSafe(|| shared.on_closed()));
                }
                Ok(())
            },
        ))
        .map_err(failed())?;

    let mut stream = WgcStream {
        shared,
        pool,
        session,
        item,
        frame_token,
        closed_token,
        info,
        watchdog: None,
        stopped: false,
    };
    if let Err(error) = stream.session.StartCapture() {
        stream.shutdown();
        return Err(failed()(error));
    }
    stream.watchdog = spawn_watchdog(Arc::downgrade(&stream.shared), stream.item.clone());
    Ok(stream)
}
