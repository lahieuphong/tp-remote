//! The local preview host: drives the [`PreviewMachine`], the OS picker and the capture
//! provider, and keeps the latest preview frame for the UI (one slot, latest wins).
//!
//! Threads:
//! - `start`, `stop`, `status`, `take_frame` are called from IPC handlers (`stop` also
//!   at app exit). `start` must run on the owner window's UI thread (picker rule).
//! - The picker completion arrives on a WinRT thread; capture is then created on a
//!   short-lived `tp-preview-start` thread.
//! - Frames arrive on the provider's thread ([`PreviewSink`]). A provider-initiated end
//!   is handled on a short-lived `tp-preview-end` thread, because a sink must never stop
//!   its own stream.
//!
//! Lock order: `state` before `frames`. The sink only ever takes `frames`, and never
//! while reading pixels.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use tp_contracts::capability::Availability;
use tp_contracts::capture::{
    CaptureConfig, CaptureEvent, CapturePicker, CaptureStream, FrameMeta, FrameReadback, FrameSink,
    PickOutcome, PickerOwner, PixelFormat, PixelSize, TargetInfo, TargetKind,
};
use tp_contracts::error::{CoreError, ErrorCategory};
use tp_contracts::ipc::preview_frame::{HEADER_LEN, Header};
use tp_contracts::ipc::{
    CaptureAvailability, CaptureDisplay, CaptureDisplays, LocalPreviewState, LocalPreviewStatus,
    PreviewError, PreviewOutcome, PreviewStats, PreviewTarget, PreviewTargetKind,
};
use tp_contracts::time::{Clock, MonoTime};
use tp_diagnostics::{Event, FieldValue, Logger};

use super::LocalPreviewGrant;
use super::machine::{EndReason, PreviewMachine, PreviewState, Reject, StopAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewConfig {
    /// Preview frames are scaled down to fit within this size.
    pub max_size: PixelSize,
    /// Upper bound on CPU readbacks per second (capture itself is not throttled).
    pub max_preview_fps: u32,
    pub capture: CaptureConfig,
    pub stats_log_interval: Duration,
}

impl Default for PreviewConfig {
    fn default() -> PreviewConfig {
        PreviewConfig {
            max_size: PixelSize {
                width: 1280,
                height: 720,
            },
            max_preview_fps: 30,
            capture: CaptureConfig {
                max_fps: 0,
                format: PixelFormat::Bgra8,
                show_cursor: true,
            },
            stats_log_interval: Duration::from_secs(5),
        }
    }
}

/// Why `start` did not open the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartError {
    Rejected(Reject),
    Platform(CoreError),
}

const RATE_WINDOW: Duration = Duration::from_secs(1);
const MIN_RATE_SPAN: Duration = Duration::from_millis(250);
const MAX_TIMESTAMPS: usize = 512;

/// Preview frames and counters shared with the sink of the current run.
#[derive(Default)]
struct FrameState {
    generation: u32,
    accepting: bool,
    slot: Option<Vec<u8>>,
    seq: u32,
    captured: u64,
    previewed: u64,
    skipped: u64,
    served: u64,
    painted: u32,
    arrivals: VecDeque<MonoTime>,
    serves: VecDeque<MonoTime>,
    selected_at: Option<MonoTime>,
    first_frame_at: Option<MonoTime>,
    frame_size: Option<PixelSize>,
    preview_size: Option<PixelSize>,
    last_readback_at: Option<MonoTime>,
    last_log_at: Option<MonoTime>,
}

fn push_time(times: &mut VecDeque<MonoTime>, now: MonoTime) {
    while times
        .front()
        .is_some_and(|t| now.saturating_duration_since(*t) > RATE_WINDOW)
        || times.len() >= MAX_TIMESTAMPS
    {
        times.pop_front();
    }
    times.push_back(now);
}

/// Events per second observed in the last second (measured, never estimated).
fn rate(times: &VecDeque<MonoTime>, now: MonoTime, since: Option<MonoTime>) -> f64 {
    let Some(since) = since else { return 0.0 };
    let span = now.saturating_duration_since(since).min(RATE_WINDOW);
    if span < MIN_RATE_SPAN {
        return 0.0;
    }
    let count = times
        .iter()
        .filter(|t| now.saturating_duration_since(**t) <= RATE_WINDOW)
        .count();
    (count as f64 / span.as_secs_f64() * 10.0).round() / 10.0
}

impl FrameState {
    fn reset(&mut self, generation: u32, selected_at: MonoTime) {
        *self = FrameState {
            generation,
            accepting: true,
            selected_at: Some(selected_at),
            ..FrameState::default()
        };
    }

    fn revoke(&mut self) {
        self.accepting = false;
        self.slot = None;
    }
}

struct Frames(Mutex<FrameState>);

impl Frames {
    fn lock(&self) -> MutexGuard<'_, FrameState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct HostState<S> {
    machine: PreviewMachine,
    grant: Option<LocalPreviewGrant>,
    stream: Option<S>,
    target: Option<TargetInfo>,
    started_at: Option<MonoTime>,
    /// Run duration, frozen when the run ends.
    elapsed: Option<Duration>,
    dropped: u64,
}

struct Inner<P: CapturePicker> {
    platform: P,
    clock: Arc<dyn Clock>,
    logger: Logger,
    config: PreviewConfig,
    availability: Availability,
    state: Mutex<HostState<P::Stream>>,
    frames: Arc<Frames>,
}

/// The local self-preview service. Cheap to share: clones refer to the same host.
pub struct LocalPreview<P: CapturePicker> {
    inner: Arc<Inner<P>>,
}

impl<P: CapturePicker> Clone for LocalPreview<P> {
    fn clone(&self) -> Self {
        LocalPreview {
            inner: self.inner.clone(),
        }
    }
}

impl<P> LocalPreview<P>
where
    P: CapturePicker + Send + Sync + 'static,
    P::Frame: FrameReadback,
    P::Stream: 'static,
{
    pub fn new(platform: P, clock: Arc<dyn Clock>, logger: Logger, config: PreviewConfig) -> Self {
        let availability = platform.capabilities().availability;
        LocalPreview {
            inner: Arc::new(Inner {
                platform,
                clock,
                logger,
                config,
                availability,
                state: Mutex::new(HostState {
                    machine: PreviewMachine::new(availability == Availability::Available),
                    grant: None,
                    stream: None,
                    target: None,
                    started_at: None,
                    elapsed: None,
                    dropped: 0,
                }),
                frames: Arc::new(Frames(Mutex::new(FrameState::default()))),
            }),
        }
    }

    /// Monitors as the platform reports them now.
    pub fn displays(&self) -> Result<CaptureDisplays, CoreError> {
        let layout = self.inner.platform.displays()?;
        Ok(CaptureDisplays {
            epoch: layout.epoch,
            displays: layout
                .displays
                .iter()
                .map(|d| CaptureDisplay {
                    index: d.id.0,
                    name: d.name.clone(),
                    x: d.bounds_os.x,
                    y: d.bounds_os.y,
                    width: d.size_px.width,
                    height: d.size_px.height,
                    scale_percent: d.scale_percent,
                    refresh_hz: d.refresh_hz,
                    primary: d.is_primary,
                })
                .collect(),
        })
    }

    /// Opens the OS picker over `owner`. Call on the owner window's UI thread. Capture
    /// starts only if the local user selects a target in the picker.
    pub fn start(&self, owner: PickerOwner) -> Result<LocalPreviewStatus, StartError> {
        let generation = self
            .inner
            .state()
            .machine
            .request_start()
            .map_err(StartError::Rejected)?;
        self.inner.log(
            Event::info("preview.start_requested")
                .field("generation", FieldValue::U64(u64::from(generation))),
        );
        let weak = Arc::downgrade(&self.inner);
        let opened = self.inner.platform.pick(
            owner,
            Box::new(move |outcome| {
                if let Some(inner) = weak.upgrade() {
                    Inner::on_picked(&inner, generation, outcome);
                }
            }),
        );
        if let Err(error) = opened {
            let _ = self.inner.state().machine.picker_failed(generation, error);
            self.inner
                .log_error("preview.picker_failed", generation, error);
            return Err(StartError::Platform(error));
        }
        Ok(self.status())
    }

    /// Stops the preview (idempotent): revokes the grant, discards the preview buffer
    /// and releases the capture before returning.
    pub fn stop(&self) -> LocalPreviewStatus {
        Inner::stop_run(&self.inner);
        self.status()
    }

    /// The latest preview frame for `generation` (header + RGBA), or an empty header if
    /// there is none or the run is not authorized. `painted` is the UI's count of drawn
    /// frames, logged for preview-rate measurements.
    pub fn take_frame(&self, generation: u32, painted: u32) -> Vec<u8> {
        let now = self.inner.clock.now();
        let state = self.inner.state();
        let authorized = state.machine.state() == PreviewState::Capturing
            && state
                .grant
                .as_ref()
                .is_some_and(|g| g.permits_frame(generation));
        let mut frames = self.inner.frames.lock();
        frames.painted = painted;
        if !authorized || frames.generation != generation || !frames.accepting {
            return Header::EMPTY.encode().to_vec();
        }
        match frames.slot.take() {
            Some(frame) => {
                frames.served += 1;
                push_time(&mut frames.serves, now);
                frame
            }
            None => Header::EMPTY.encode().to_vec(),
        }
    }

    pub fn status(&self) -> LocalPreviewStatus {
        let now = self.inner.clock.now();
        let state = self.inner.state();
        let frames = self.inner.frames.lock();
        let machine_state = state.machine.state();
        let elapsed = match (machine_state, state.started_at) {
            (PreviewState::Capturing, Some(started)) => now.saturating_duration_since(started),
            _ => state.elapsed.unwrap_or_default(),
        };
        let live = machine_state == PreviewState::Capturing;
        let first_frame_ms = match (frames.selected_at, frames.first_frame_at) {
            (Some(selected), Some(first)) => {
                Some(millis(first.saturating_duration_since(selected)))
            }
            _ => None,
        };
        let size = |s: Option<PixelSize>| {
            s.unwrap_or(PixelSize {
                width: 0,
                height: 0,
            })
        };
        LocalPreviewStatus {
            availability: match self.inner.availability {
                Availability::Available => CaptureAvailability::Available,
                Availability::NotImplemented => CaptureAvailability::NotImplemented,
                Availability::Unavailable(_) => CaptureAvailability::Unsupported,
            },
            state: ipc_state(machine_state),
            generation: state.machine.generation(),
            target: state.target.as_ref().map(|t| PreviewTarget {
                kind: match t.kind {
                    TargetKind::Display => PreviewTargetKind::Display,
                    TargetKind::Unknown => PreviewTargetKind::Unknown,
                },
                label: t.label.clone(),
                width: t.size.width,
                height: t.size.height,
                display_index: t.display.map(|d| d.0),
            }),
            stats: PreviewStats {
                captured_frames: saturate(frames.captured),
                previewed_frames: saturate(frames.previewed),
                skipped_frames: saturate(frames.skipped),
                dropped_frames: saturate(state.dropped),
                capture_fps: if live {
                    rate(&frames.arrivals, now, frames.first_frame_at)
                } else {
                    0.0
                },
                preview_fps: if live {
                    rate(&frames.serves, now, frames.first_frame_at)
                } else {
                    0.0
                },
                elapsed_ms: millis(elapsed),
                first_frame_ms,
                frame_width: size(frames.frame_size).width,
                frame_height: size(frames.frame_size).height,
                preview_width: size(frames.preview_size).width,
                preview_height: size(frames.preview_size).height,
            },
            last_outcome: state.machine.last_end().map(|end| match end {
                EndReason::Cancelled => PreviewOutcome::Cancelled,
                EndReason::StoppedByUser => PreviewOutcome::StoppedByUser,
                EndReason::TargetClosed => PreviewOutcome::TargetClosed,
                EndReason::DeviceLost => PreviewOutcome::DeviceLost,
                EndReason::Failed(_) => PreviewOutcome::Failed,
            }),
            error: match (machine_state, state.machine.last_end()) {
                (PreviewState::Failed, Some(reason)) => Some(error_text(reason)),
                _ => None,
            },
        }
    }

    /// Current preview state (tests and diagnostics).
    pub fn state(&self) -> PreviewState {
        self.inner.state().machine.state()
    }

    /// Whether a local preview grant is currently held.
    pub fn has_grant(&self) -> bool {
        self.inner.state().grant.is_some()
    }

    /// Picker results and stream events rejected as stale or out of order.
    pub fn rejected_events(&self) -> u32 {
        self.inner.state().machine.rejected()
    }
}

fn saturate(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn millis(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis()).unwrap_or(u32::MAX)
}

fn ipc_state(state: PreviewState) -> LocalPreviewState {
    match state {
        PreviewState::NotAvailable => LocalPreviewState::NotAvailable,
        PreviewState::Ready => LocalPreviewState::Ready,
        PreviewState::SelectingTarget => LocalPreviewState::SelectingTarget,
        PreviewState::Initializing => LocalPreviewState::Initializing,
        PreviewState::Capturing => LocalPreviewState::Capturing,
        PreviewState::Stopping => LocalPreviewState::Stopping,
        PreviewState::Stopped => LocalPreviewState::Stopped,
        PreviewState::Failed => LocalPreviewState::Failed,
    }
}

/// Fixed user-facing text; never OS error strings, paths or titles.
fn error_text(reason: EndReason) -> PreviewError {
    let (message, recovery) = match reason {
        EndReason::DeviceLost => (
            "The graphics device was reset, so capture stopped.",
            "Start the preview again.",
        ),
        EndReason::Failed(error) if error.category() == ErrorCategory::PermissionDenied => (
            "Windows did not allow capture of the selected item.",
            "Choose a different display or window.",
        ),
        EndReason::Failed(_) => (
            "Capture could not start or stopped unexpectedly.",
            "Start the preview again and select the display or window once more.",
        ),
        EndReason::Cancelled | EndReason::StoppedByUser | EndReason::TargetClosed => {
            ("Capture ended.", "Start the preview again.")
        }
    };
    PreviewError {
        message: message.to_owned(),
        recovery: recovery.to_owned(),
    }
}

impl<P> Inner<P>
where
    P: CapturePicker + Send + Sync + 'static,
    P::Frame: FrameReadback,
    P::Stream: 'static,
{
    fn state(&self) -> MutexGuard<'_, HostState<P::Stream>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn log(&self, event: Event) {
        self.logger.log(&event);
    }

    fn log_error(&self, name: &'static str, generation: u32, error: CoreError) {
        self.log(
            Event::warn(name)
                .field("generation", FieldValue::U64(u64::from(generation)))
                .field("error_code", FieldValue::Static(error.category().as_str())),
        );
    }

    fn log_rejected(&self, name: &'static str, generation: u32, reject: Reject) {
        self.log(
            Event::warn(name)
                .field("generation", FieldValue::U64(u64::from(generation)))
                .field("reason", FieldValue::Static(reject.as_str())),
        );
    }

    /// Picker completion. Only a selection for the current request mints a grant.
    fn on_picked(inner: &Arc<Self>, generation: u32, outcome: PickOutcome<P::Target>) {
        match outcome {
            PickOutcome::Selected(target) => {
                let now = inner.clock.now();
                let accepted = {
                    let mut state = inner.state();
                    match state.machine.selected(generation) {
                        Ok(()) => {
                            state.grant =
                                Some(LocalPreviewGrant::on_picker_selection(generation, now));
                            state.target = None;
                            state.started_at = None;
                            state.elapsed = None;
                            state.dropped = 0;
                            inner.frames.lock().reset(generation, now);
                            Ok(())
                        }
                        Err(reject) => Err(reject),
                    }
                };
                match accepted {
                    Ok(()) => {
                        inner.log(
                            Event::info("preview.target_selected")
                                .field("generation", FieldValue::U64(u64::from(generation))),
                        );
                        let starter = inner.clone();
                        let spawned = std::thread::Builder::new()
                            .name("tp-preview-start".into())
                            .spawn(move || Inner::start_capture(&starter, generation, target));
                        if spawned.is_err() {
                            let error =
                                CoreError::new(ErrorCategory::InternalError, "thread spawn failed");
                            let mut state = inner.state();
                            let _ = state.machine.start_failed(generation, error);
                            state.grant = None;
                        }
                    }
                    Err(reject) => {
                        // Stale or stopped: the target is released without capturing.
                        drop(target);
                        inner.log_rejected("preview.selection_discarded", generation, reject);
                    }
                }
            }
            PickOutcome::Cancelled => match inner.state().machine.cancelled(generation) {
                Ok(()) => inner.log(
                    Event::info("preview.picker_cancelled")
                        .field("generation", FieldValue::U64(u64::from(generation))),
                ),
                Err(reject) => inner.log_rejected("preview.cancel_ignored", generation, reject),
            },
            PickOutcome::Failed(error) => {
                if inner
                    .state()
                    .machine
                    .picker_failed(generation, error)
                    .is_ok()
                {
                    inner.log_error("preview.picker_failed", generation, error);
                }
            }
        }
    }

    fn start_capture(inner: &Arc<Self>, generation: u32, target: P::Target) {
        let sink = Arc::new(PreviewSink {
            generation,
            frames: inner.frames.clone(),
            host: Arc::downgrade(inner),
            clock: inner.clock.clone(),
            max_size: inner.config.max_size,
            min_interval: if inner.config.max_preview_fps > 0 {
                Duration::from_secs(1) / inner.config.max_preview_fps
            } else {
                Duration::ZERO
            },
            stats_interval: inner.config.stats_log_interval,
            ending: AtomicBool::new(false),
        });
        let result = inner.platform.start(target, inner.config.capture, sink);
        let now = inner.clock.now();
        let mut state = inner.state();
        match result {
            Ok(stream) => match state.machine.started(generation) {
                Ok(()) => {
                    let info = stream.target().clone();
                    state.target = Some(info.clone());
                    state.started_at = Some(now);
                    state.stream = Some(stream);
                    drop(state);
                    inner.log(
                        Event::info("preview.capturing")
                            .field("generation", FieldValue::U64(u64::from(generation)))
                            .field("target_kind", FieldValue::Static(info.kind.as_str()))
                            .field("width", FieldValue::U64(u64::from(info.size.width)))
                            .field("height", FieldValue::U64(u64::from(info.size.height))),
                    );
                }
                Err(_) => {
                    // Stopped while initializing: release the new stream at once.
                    drop(state);
                    inner.frames.lock().revoke();
                    let stats = stream.stop().unwrap_or_default();
                    Inner::finish_stop(inner, generation, now, stats.frames_dropped);
                }
            },
            Err(error) => {
                let _ = state.machine.start_failed(generation, error);
                state.grant = None;
                drop(state);
                inner.frames.lock().revoke();
                inner.log_error("preview.start_failed", generation, error);
            }
        }
    }

    fn stop_run(inner: &Arc<Self>) {
        let (generation, stream) = {
            let mut state = inner.state();
            let was = state.machine.state();
            let action = state.machine.stop();
            state.grant = None;
            match action {
                StopAction::None => {
                    if was == PreviewState::SelectingTarget {
                        let generation = state.machine.generation();
                        drop(state);
                        inner.log(
                            Event::info("preview.stopped_while_selecting")
                                .field("generation", FieldValue::U64(u64::from(generation))),
                        );
                    }
                    return;
                }
                StopAction::StopStream { generation } => {
                    inner.frames.lock().revoke();
                    (generation, state.stream.take())
                }
            }
        };
        // Without a stream the run is still initializing; `start_capture` releases it.
        if let Some(stream) = stream {
            let stats = stream.stop().unwrap_or_default();
            Inner::finish_stop(inner, generation, inner.clock.now(), stats.frames_dropped);
        }
    }

    /// A provider-initiated end (target closed, device lost, failure).
    fn end_stream(inner: &Arc<Self>, generation: u32, reason: EndReason) {
        let stream = {
            let mut state = inner.state();
            if state.machine.stream_ended(generation, reason).is_err() {
                return; // already stopping, or stale
            }
            state.grant = None;
            inner.frames.lock().revoke();
            state.stream.take()
        };
        let dropped = stream
            .map(|s| s.stop().unwrap_or_default().frames_dropped)
            .unwrap_or(0);
        Inner::finish_stop(inner, generation, inner.clock.now(), dropped);
    }

    fn finish_stop(inner: &Arc<Self>, generation: u32, now: MonoTime, dropped: u64) {
        let (captured, previewed, skipped) = {
            let frames = inner.frames.lock();
            (frames.captured, frames.previewed, frames.skipped)
        };
        let mut state = inner.state();
        if state.machine.stopped(generation).is_err() {
            return;
        }
        state.elapsed = state.started_at.map(|s| now.saturating_duration_since(s));
        state.dropped = dropped;
        let reason = state
            .machine
            .last_end()
            .map_or("stoppedByUser", EndReason::as_str);
        let elapsed = state.elapsed.unwrap_or_default();
        drop(state);
        inner.log(
            Event::info("preview.ended")
                .field("generation", FieldValue::U64(u64::from(generation)))
                .field("reason", FieldValue::Static(reason))
                .field("frames", FieldValue::U64(captured))
                .field("previewed", FieldValue::U64(previewed))
                .field("skipped", FieldValue::U64(skipped))
                .field("dropped", FieldValue::U64(dropped))
                .field("elapsed_ms", FieldValue::U64(u64::from(millis(elapsed)))),
        );
    }
}

/// Receives frames of one run. Never blocks on the host state, never stops the stream.
struct PreviewSink<P: CapturePicker> {
    generation: u32,
    frames: Arc<Frames>,
    host: Weak<Inner<P>>,
    clock: Arc<dyn Clock>,
    max_size: PixelSize,
    min_interval: Duration,
    stats_interval: Duration,
    ending: AtomicBool,
}

impl<P> PreviewSink<P>
where
    P: CapturePicker + Send + Sync + 'static,
    P::Frame: FrameReadback,
    P::Stream: 'static,
{
    fn end(&self, reason: EndReason) {
        if self.ending.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(host) = self.host.upgrade() {
            let generation = self.generation;
            let _ = std::thread::Builder::new()
                .name("tp-preview-end".into())
                .spawn(move || Inner::end_stream(&host, generation, reason));
        }
    }

    fn log(&self, event: Event) {
        if let Some(host) = self.host.upgrade() {
            host.log(event);
        }
    }
}

impl<P> FrameSink<P::Frame> for PreviewSink<P>
where
    P: CapturePicker + Send + Sync + 'static,
    P::Frame: FrameReadback,
    P::Stream: 'static,
{
    fn on_frame(&self, meta: FrameMeta, frame: P::Frame) {
        let now = self.clock.now();
        let mut logs = Vec::new();
        {
            let mut frames = self.frames.lock();
            if !frames.accepting || frames.generation != self.generation {
                return;
            }
            frames.captured += 1;
            frames.frame_size = Some(meta.size);
            push_time(&mut frames.arrivals, now);
            if frames.first_frame_at.is_none() {
                frames.first_frame_at = Some(now);
                let ms = frames
                    .selected_at
                    .map_or(0, |s| millis(now.saturating_duration_since(s)));
                logs.push(
                    Event::info("preview.first_frame")
                        .field("generation", FieldValue::U64(u64::from(self.generation)))
                        .field("elapsed_ms", FieldValue::U64(u64::from(ms))),
                );
            }
            if frames
                .last_log_at
                .is_none_or(|t| now.saturating_duration_since(t) >= self.stats_interval)
            {
                frames.last_log_at = Some(now);
                logs.push(
                    Event::info("preview.stats")
                        .field("generation", FieldValue::U64(u64::from(self.generation)))
                        .field("frames", FieldValue::U64(frames.captured))
                        .field("previewed", FieldValue::U64(frames.previewed))
                        .field("skipped", FieldValue::U64(frames.skipped))
                        .field("painted", FieldValue::U64(u64::from(frames.painted)))
                        .field(
                            "fps",
                            FieldValue::text(&format!(
                                "{:.1}",
                                rate(&frames.arrivals, now, frames.first_frame_at)
                            )),
                        )
                        .field(
                            "preview_fps",
                            FieldValue::text(&format!(
                                "{:.1}",
                                rate(&frames.serves, now, frames.first_frame_at)
                            )),
                        ),
                );
            }
            let due = frames
                .last_readback_at
                .is_none_or(|t| now.saturating_duration_since(t) >= self.min_interval);
            if frames.slot.is_some() {
                // The UI has not taken the previous frame yet: keep it (latest-wins
                // would only waste a readback the UI cannot show).
                frames.skipped += 1;
            } else if due {
                frames.last_readback_at = Some(now);
                drop(frames);
                self.read_into_slot(&frame);
            }
        }
        drop(frame);
        for event in logs {
            self.log(event);
        }
    }

    fn on_event(&self, event: CaptureEvent) {
        if !self.frames.lock().accepting {
            return;
        }
        match event {
            CaptureEvent::TargetClosed => self.end(EndReason::TargetClosed),
            CaptureEvent::DeviceLost => self.end(EndReason::DeviceLost),
            CaptureEvent::PermissionRevoked => self.end(EndReason::Failed(CoreError::new(
                ErrorCategory::PermissionDenied,
                "capture permission revoked",
            ))),
            CaptureEvent::Failed(category) => {
                self.end(EndReason::Failed(CoreError::new(
                    category,
                    "capture failed",
                )));
            }
            CaptureEvent::TargetResized(size) => self.log(
                Event::info("preview.target_resized")
                    .field("generation", FieldValue::U64(u64::from(self.generation)))
                    .field("width", FieldValue::U64(u64::from(size.width)))
                    .field("height", FieldValue::U64(u64::from(size.height))),
            ),
            CaptureEvent::LayoutChanged { .. } | CaptureEvent::Stopped => {}
        }
    }
}

impl<P> PreviewSink<P>
where
    P: CapturePicker + Send + Sync + 'static,
    P::Frame: FrameReadback,
    P::Stream: 'static,
{
    /// Reads pixels without holding any lock, then publishes them if the run is still
    /// authorized.
    fn read_into_slot(&self, frame: &P::Frame) {
        let mut buffer = Vec::with_capacity(
            HEADER_LEN + (self.max_size.width as usize) * (self.max_size.height as usize) * 4,
        );
        buffer.extend_from_slice(&[0u8; HEADER_LEN]);
        match frame.read_rgba(self.max_size, &mut buffer) {
            Ok(size) => {
                let mut frames = self.frames.lock();
                if !frames.accepting || frames.generation != self.generation {
                    return;
                }
                frames.seq = frames.seq.wrapping_add(1);
                let header = Header {
                    generation: self.generation,
                    seq: frames.seq,
                    width: size.width,
                    height: size.height,
                };
                buffer[..HEADER_LEN].copy_from_slice(&header.encode());
                frames.slot = Some(buffer);
                frames.previewed += 1;
                frames.preview_size = Some(size);
            }
            Err(error) if error.category() == ErrorCategory::PlatformUnavailable => {
                self.end(EndReason::DeviceLost);
            }
            Err(error) => self.end(EndReason::Failed(error)),
        }
    }
}
