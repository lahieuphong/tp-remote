//! Local self-preview authorization and lifecycle (CAP-02A, T-CAP-SEC-01 … 06 and 09,
//! engine level). The platform below is a test double: the test plays the role of the
//! Windows picker by completing it explicitly, and frames are synthetic byte patterns.
//! Real Windows capture is tested in tp-test-harness/tests/capture_windows.rs.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use tp_contracts::capability::{Availability, Capability, PlatformCapabilities, UnavailableReason};
use tp_contracts::capture::{
    CaptureCapabilities, CaptureConfig, CaptureEvent, CapturePicker, CaptureProvider, CaptureStats,
    CaptureStream, DisplayId, DisplayInfo, DisplayLayout, FrameMeta, FrameReadback, FrameSink,
    PickDone, PickOutcome, PickerOwner, PixelFormat, PixelSize, RectI32, TargetInfo, TargetKind,
};
use tp_contracts::error::{CoreError, ErrorCategory};
use tp_contracts::id::{Generation, SessionId, SessionRef};
use tp_contracts::input::{InjectableEvent, InputCapabilities, InputController};
use tp_contracts::ipc::preview_frame::{HEADER_LEN, Header};
use tp_contracts::ipc::{CaptureAvailability, LocalPreviewState, PreviewOutcome};
use tp_contracts::time::{Clock, MonoTime};
use tp_core::gate::InputGate;
use tp_core::preview::{LocalPreview, PreviewConfig, PreviewState, Reject, StartError};
use tp_core::service::SessionService;
use tp_diagnostics::{Logger, MemorySink, REDACTED};
use tp_security::Decision;
use tp_session::{SequentialIds, Timeouts};

/// A window title that must never reach a log line.
const PRIVATE_TITLE: &str = "Q3 salaries.xlsx - Excel";
const OWNER: PickerOwner = PickerOwner(0x1234);

struct ManualClock(AtomicU64);

impl ManualClock {
    fn new() -> Arc<ManualClock> {
        Arc::new(ManualClock(AtomicU64::new(1_000_000)))
    }
    fn advance(&self, duration: Duration) {
        self.0
            .fetch_add(duration.as_micros() as u64, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> MonoTime {
        MonoTime::from_micros(self.0.load(Ordering::SeqCst))
    }
}

/// Counts how many targets were released (dropped), captured or not.
struct FakeTarget {
    released: Arc<AtomicU32>,
}

impl Drop for FakeTarget {
    fn drop(&mut self) {
        self.released.fetch_add(1, Ordering::SeqCst);
    }
}

struct FakeFrame {
    size: PixelSize,
    fail: Option<CoreError>,
}

impl FrameReadback for FakeFrame {
    fn read_rgba(&self, max: PixelSize, out: &mut Vec<u8>) -> Result<PixelSize, CoreError> {
        if let Some(error) = self.fail {
            return Err(error);
        }
        let size = PixelSize {
            width: self.size.width.min(max.width),
            height: self.size.height.min(max.height),
        };
        out.extend(
            std::iter::repeat_n([7u8, 8, 9, 255], (size.width * size.height) as usize).flatten(),
        );
        Ok(size)
    }
}

type Sink = Arc<dyn FrameSink<FakeFrame>>;

#[derive(Default)]
struct Shared {
    pending: Mutex<Vec<PickDone<FakeTarget>>>,
    picks: AtomicU32,
    starts: AtomicU32,
    stops: AtomicU32,
    active_sink: Mutex<Option<Sink>>,
    last_sink: Mutex<Option<Sink>>,
    fail_pick: AtomicBool,
    fail_start: AtomicBool,
    /// When set, `start` blocks until `release_start` is called.
    hold_start: Mutex<bool>,
    start_released: Condvar,
}

struct FakePlatform {
    available: bool,
    shared: Arc<Shared>,
}

struct FakeStream {
    info: TargetInfo,
    shared: Arc<Shared>,
    _target: FakeTarget,
}

impl CaptureStream for FakeStream {
    fn target(&self) -> &TargetInfo {
        &self.info
    }
    fn generation(&self) -> u64 {
        1
    }
    fn reconfigure(&mut self, _config: CaptureConfig) -> Result<(), CoreError> {
        Ok(())
    }
    fn stop(self) -> Result<CaptureStats, CoreError> {
        // Contract: after stop returns, the sink receives nothing more.
        *self.shared.active_sink.lock().unwrap() = None;
        self.shared.stops.fetch_add(1, Ordering::SeqCst);
        Ok(CaptureStats::default())
    }
}

impl CaptureProvider for FakePlatform {
    type Frame = FakeFrame;
    type Target = FakeTarget;
    type Stream = FakeStream;

    fn capabilities(&self) -> CaptureCapabilities {
        CaptureCapabilities {
            availability: if self.available {
                Availability::Available
            } else {
                Availability::Unavailable(UnavailableReason::PlatformUnsupported)
            },
            max_fps: 60,
            gpu_frames: false,
            system_picker: true,
        }
    }

    fn displays(&self) -> Result<DisplayLayout, CoreError> {
        Ok(DisplayLayout {
            epoch: 1,
            displays: vec![DisplayInfo {
                id: DisplayId(0),
                name: "DISPLAY1".into(),
                bounds_os: RectI32 {
                    x: -1920,
                    y: 71,
                    width: 1920,
                    height: 1080,
                },
                size_px: PixelSize {
                    width: 1920,
                    height: 1080,
                },
                scale_milli: 1000,
                scale_percent: Some(100),
                refresh_hz: Some(60),
                is_primary: false,
            }],
        })
    }

    fn start<S>(
        &self,
        target: FakeTarget,
        _config: CaptureConfig,
        sink: Arc<S>,
    ) -> Result<FakeStream, CoreError>
    where
        S: FrameSink<FakeFrame> + 'static,
    {
        self.shared.starts.fetch_add(1, Ordering::SeqCst);
        let mut held = self.shared.hold_start.lock().unwrap();
        while *held {
            held = self.shared.start_released.wait(held).unwrap();
        }
        drop(held);
        if self.shared.fail_start.load(Ordering::SeqCst) {
            return Err(CoreError::new(
                ErrorCategory::PlatformUnavailable,
                "fake start failure",
            ));
        }
        let sink: Sink = sink;
        *self.shared.active_sink.lock().unwrap() = Some(sink.clone());
        *self.shared.last_sink.lock().unwrap() = Some(sink);
        Ok(FakeStream {
            info: TargetInfo {
                kind: TargetKind::Unknown,
                label: PRIVATE_TITLE.into(),
                size: PixelSize {
                    width: 1920,
                    height: 1080,
                },
                display: None,
            },
            shared: self.shared.clone(),
            _target: target,
        })
    }
}

impl CapturePicker for FakePlatform {
    fn pick(&self, owner: PickerOwner, done: PickDone<FakeTarget>) -> Result<(), CoreError> {
        assert_eq!(owner, OWNER);
        if self.shared.fail_pick.load(Ordering::SeqCst) {
            return Err(CoreError::new(
                ErrorCategory::PlatformUnavailable,
                "fake picker failure",
            ));
        }
        self.shared.picks.fetch_add(1, Ordering::SeqCst);
        self.shared.pending.lock().unwrap().push(done);
        Ok(())
    }
}

struct Fixture {
    preview: LocalPreview<FakePlatform>,
    shared: Arc<Shared>,
    clock: Arc<ManualClock>,
    log: Arc<MemorySink>,
    released: Arc<AtomicU32>,
}

fn fixture(available: bool) -> Fixture {
    let shared = Arc::new(Shared::default());
    let clock = ManualClock::new();
    let log = Arc::new(MemorySink::new());
    let preview = LocalPreview::new(
        FakePlatform {
            available,
            shared: shared.clone(),
        },
        clock.clone(),
        Logger::new(log.clone()),
        PreviewConfig {
            max_preview_fps: 0,
            ..PreviewConfig::default()
        },
    );
    Fixture {
        preview,
        shared,
        clock,
        log,
        released: Arc::new(AtomicU32::new(0)),
    }
}

impl Fixture {
    fn target(&self) -> FakeTarget {
        FakeTarget {
            released: self.released.clone(),
        }
    }

    /// Completes the oldest pending picker invocation, as the OS picker would.
    fn complete_pick(&self, outcome: PickOutcome<FakeTarget>) {
        let done = self.shared.pending.lock().unwrap().remove(0);
        done(outcome);
    }

    fn wait_for(&self, state: PreviewState) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.preview.state() != state {
            assert!(
                Instant::now() < deadline,
                "stuck in {:?}, wanted {state:?}",
                self.preview.state()
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn start_capturing(&self) -> u32 {
        let status = self.preview.start(OWNER).expect("start");
        self.complete_pick(PickOutcome::Selected(self.target()));
        self.wait_for(PreviewState::Capturing);
        status.generation
    }

    fn deliver(&self, seq: u64) {
        let sink = self.shared.active_sink.lock().unwrap().clone();
        if let Some(sink) = sink {
            sink.on_frame(
                meta(seq),
                FakeFrame {
                    size: size(4, 2),
                    fail: None,
                },
            );
        }
    }

    fn log_text(&self) -> String {
        self.log.lines().join("\n")
    }
}

const fn size(width: u32, height: u32) -> PixelSize {
    PixelSize { width, height }
}

fn meta(seq: u64) -> FrameMeta {
    FrameMeta {
        generation: 1,
        seq,
        captured_at: MonoTime::from_micros(seq * 16_667),
        size: size(4, 2),
        format: PixelFormat::Bgra8,
    }
}

fn header(frame: &[u8]) -> Header {
    Header::decode(frame).expect("frame header")
}

// T-CAP-SEC-01: the IPC request only opens the picker. Nothing is captured, granted or
// served until the picker reports a selection.
#[test]
fn no_capture_starts_without_a_picker_selection() {
    let f = fixture(true);
    let status = f.preview.start(OWNER).expect("start");
    assert_eq!(status.state, LocalPreviewState::SelectingTarget);
    assert_eq!(f.shared.picks.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.shared.starts.load(Ordering::SeqCst),
        0,
        "capture started without a selection"
    );
    assert!(!f.preview.has_grant());
    assert_eq!(
        header(&f.preview.take_frame(status.generation, 0)),
        Header::EMPTY
    );
}

// T-CAP-SEC-02: cancelling the picker produces no capture and no grant.
#[test]
fn cancelling_the_picker_captures_nothing() {
    let f = fixture(true);
    f.preview.start(OWNER).expect("start");
    f.complete_pick(PickOutcome::Cancelled);
    assert_eq!(f.preview.state(), PreviewState::Ready);
    let status = f.preview.status();
    assert_eq!(status.last_outcome, Some(PreviewOutcome::Cancelled));
    assert!(status.error.is_none());
    assert_eq!(f.shared.starts.load(Ordering::SeqCst), 0);
    assert!(!f.preview.has_grant());
    // A new request is possible afterwards.
    assert!(f.preview.start(OWNER).is_ok());
}

struct NeverCalled(Arc<AtomicU32>);
impl InputController for NeverCalled {
    fn capabilities(&self) -> InputCapabilities {
        InputCapabilities {
            availability: Availability::Available,
            absolute_pointer: true,
        }
    }
    fn inject(&mut self, _event: &InjectableEvent) -> Result<(), CoreError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), CoreError> {
        Ok(())
    }
}

// T-CAP-SEC-03 / 04: an active local preview grant does not authorize remote viewing or
// input. (Type-level separation is proven by the compile_fail doctests in
// tp_core::preview; this checks that the running system behaves the same way, even with
// a platform that claims every capability.)
#[test]
fn a_local_preview_grant_authorizes_neither_remote_viewing_nor_input() {
    let f = fixture(true);
    f.start_capturing();
    assert!(f.preview.has_grant());

    let service = SessionService::new(
        SequentialIds::default(),
        Timeouts::default(),
        PlatformCapabilities {
            view_screen: Availability::Available,
            control_input: Availability::Available,
        },
        Logger::new(f.log.clone()),
    );
    let session = SessionRef {
        id: SessionId::from_bytes([9; 16]).expect("id"),
        generation: Generation::FIRST,
    };
    let now = f.clock.now();
    for capability in Capability::ALL {
        assert!(
            matches!(
                service.authorize(session, capability, now),
                Decision::Deny(_)
            ),
            "{capability:?} must be denied"
        );
    }
    let calls = Arc::new(AtomicU32::new(0));
    let mut gate = InputGate::new(NeverCalled(calls.clone()));
    assert!(
        gate.inject(
            &service,
            session,
            &InjectableEvent::PointerMove { x: 1, y: 1 },
            now
        )
        .is_err()
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "input reached the controller"
    );
    assert_eq!(tp_core::platform_capabilities(), PlatformCapabilities::NONE);
}

// T-CAP-SEC-05: stopping revokes the grant, discards the buffered frame and ignores late
// frames from a sink that is still referenced.
#[test]
fn stop_revokes_preview_access() {
    let f = fixture(true);
    let generation = f.start_capturing();
    f.deliver(1);
    let sink = f.shared.last_sink.lock().unwrap().clone().expect("sink");
    let before = f.preview.status().stats.captured_frames;

    let status = f.preview.stop();
    assert_eq!(status.state, LocalPreviewState::Stopped);
    assert_eq!(status.last_outcome, Some(PreviewOutcome::StoppedByUser));
    assert!(!f.preview.has_grant());
    assert_eq!(header(&f.preview.take_frame(generation, 0)), Header::EMPTY);

    sink.on_frame(
        meta(2),
        FakeFrame {
            size: size(4, 2),
            fail: None,
        },
    );
    assert_eq!(
        f.preview.status().stats.captured_frames,
        before,
        "late frame accepted"
    );
    assert_eq!(header(&f.preview.take_frame(generation, 0)), Header::EMPTY);
    assert_eq!(f.shared.stops.load(Ordering::SeqCst), 1);
}

// T-CAP-SEC-06: a selection that arrives after a stop, or for an older request, never
// starts capture; its target is released.
#[test]
fn stale_selections_cannot_restart_capture() {
    let f = fixture(true);
    // Stop while the picker is open, then the user picks something.
    f.preview.start(OWNER).expect("start");
    f.preview.stop();
    assert_eq!(f.preview.state(), PreviewState::Stopped);
    f.complete_pick(PickOutcome::Selected(f.target()));
    assert_eq!(f.preview.state(), PreviewState::Stopped);
    assert_eq!(f.shared.starts.load(Ordering::SeqCst), 0);
    assert_eq!(
        f.released.load(Ordering::SeqCst),
        1,
        "stale target not released"
    );
    assert!(!f.preview.has_grant());

    // Request 2 is cancelled; request 3 is open; request 2's picker reports late.
    f.preview.start(OWNER).expect("start 2");
    let late = f.shared.pending.lock().unwrap().remove(0);
    f.preview.stop();
    let third = f.preview.start(OWNER).expect("start 3").generation;
    let rejected_before = f.preview.rejected_events();
    late(PickOutcome::Selected(f.target()));
    assert_eq!(f.preview.state(), PreviewState::SelectingTarget);
    assert_eq!(f.shared.starts.load(Ordering::SeqCst), 0);
    assert_eq!(f.released.load(Ordering::SeqCst), 2);
    assert_eq!(f.preview.rejected_events(), rejected_before + 1);

    // The current request still works.
    f.complete_pick(PickOutcome::Selected(f.target()));
    f.wait_for(PreviewState::Capturing);
    assert_eq!(f.preview.status().generation, third);
    assert_eq!(f.shared.starts.load(Ordering::SeqCst), 1);
}

#[test]
fn a_second_start_is_rejected_while_one_is_in_progress() {
    let f = fixture(true);
    f.preview.start(OWNER).expect("start");
    assert_eq!(
        f.preview.start(OWNER).err(),
        Some(StartError::Rejected(Reject::Busy))
    );
    f.complete_pick(PickOutcome::Selected(f.target()));
    f.wait_for(PreviewState::Capturing);
    assert_eq!(
        f.preview.start(OWNER).err(),
        Some(StartError::Rejected(Reject::Busy))
    );
    assert_eq!(
        f.shared.picks.load(Ordering::SeqCst),
        1,
        "a second picker was opened"
    );
}

#[test]
fn an_unavailable_platform_never_opens_the_picker() {
    let f = fixture(false);
    let status = f.preview.status();
    assert_eq!(status.state, LocalPreviewState::NotAvailable);
    assert_eq!(status.availability, CaptureAvailability::Unsupported);
    assert_eq!(
        f.preview.start(OWNER).err(),
        Some(StartError::Rejected(Reject::NotAvailable))
    );
    assert_eq!(f.shared.picks.load(Ordering::SeqCst), 0);
}

#[test]
fn picker_and_start_failures_end_in_failed_with_fixed_text() {
    let f = fixture(true);
    f.shared.fail_pick.store(true, Ordering::SeqCst);
    assert!(matches!(
        f.preview.start(OWNER),
        Err(StartError::Platform(_))
    ));
    let status = f.preview.status();
    assert_eq!(status.state, LocalPreviewState::Failed);
    assert!(status.error.is_some());

    f.shared.fail_pick.store(false, Ordering::SeqCst);
    f.shared.fail_start.store(true, Ordering::SeqCst);
    f.preview.start(OWNER).expect("retry after failure");
    f.complete_pick(PickOutcome::Selected(f.target()));
    f.wait_for(PreviewState::Failed);
    let status = f.preview.status();
    assert_eq!(status.last_outcome, Some(PreviewOutcome::Failed));
    assert!(!f.preview.has_grant());
    assert_eq!(
        f.released.load(Ordering::SeqCst),
        1,
        "failed target not released"
    );
}

#[test]
fn frames_flow_through_a_single_latest_slot() {
    let f = fixture(true);
    let generation = f.start_capturing();
    assert_eq!(header(&f.preview.take_frame(generation, 0)), Header::EMPTY);
    f.deliver(1);
    f.deliver(2); // slot still full: skipped
    let frame = f.preview.take_frame(generation, 1);
    let h = header(&frame);
    assert_eq!(
        (h.generation, h.seq, h.width, h.height),
        (generation, 1, 4, 2)
    );
    assert_eq!(frame.len(), HEADER_LEN + 4 * 2 * 4);
    assert_eq!(&frame[HEADER_LEN..HEADER_LEN + 4], &[7, 8, 9, 255]);
    assert_eq!(header(&f.preview.take_frame(generation, 1)), Header::EMPTY);
    assert_eq!(
        header(&f.preview.take_frame(generation + 1, 1)),
        Header::EMPTY
    );

    f.clock.advance(Duration::from_millis(500));
    f.deliver(3);
    let stats = f.preview.status().stats;
    assert_eq!(stats.captured_frames, 3);
    assert_eq!(stats.previewed_frames, 2);
    assert_eq!(stats.skipped_frames, 1);
    assert_eq!((stats.frame_width, stats.frame_height), (4, 2));
    assert!(
        stats.capture_fps > 0.0,
        "rate over a 0.5 s span with 3 frames"
    );
    assert_eq!(stats.elapsed_ms, 500);
}

#[test]
fn a_closed_target_ends_the_run_and_releases_the_stream_once() {
    let f = fixture(true);
    f.start_capturing();
    let sink = f.shared.active_sink.lock().unwrap().clone().expect("sink");
    sink.on_event(CaptureEvent::TargetClosed);
    sink.on_event(CaptureEvent::TargetClosed);
    f.wait_for(PreviewState::Stopped);
    let status = f.preview.status();
    assert_eq!(status.last_outcome, Some(PreviewOutcome::TargetClosed));
    assert!(status.error.is_none());
    assert!(!f.preview.has_grant());
    let deadline = Instant::now() + Duration::from_secs(2);
    while f.shared.stops.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(f.shared.stops.load(Ordering::SeqCst), 1);
}

// T-CAP-WIN-10 (engine part): a lost device or a failing readback ends the run as Failed
// with recovery text; capture can be started again.
#[test]
fn device_loss_and_readback_failures_end_in_failed() {
    let f = fixture(true);
    f.start_capturing();
    let sink = f.shared.active_sink.lock().unwrap().clone().expect("sink");
    sink.on_event(CaptureEvent::DeviceLost);
    f.wait_for(PreviewState::Failed);
    let status = f.preview.status();
    assert_eq!(status.last_outcome, Some(PreviewOutcome::DeviceLost));
    assert!(status.error.is_some_and(|e| e.recovery.contains("again")));

    f.start_capturing();
    let sink = f.shared.active_sink.lock().unwrap().clone().expect("sink");
    sink.on_frame(
        meta(1),
        FakeFrame {
            size: size(4, 2),
            fail: Some(CoreError::new(
                ErrorCategory::InternalError,
                "fake readback failure",
            )),
        },
    );
    f.wait_for(PreviewState::Failed);
    assert_eq!(
        f.preview.status().last_outcome,
        Some(PreviewOutcome::Failed)
    );
}

#[test]
fn stop_during_initialization_releases_the_new_stream() {
    let f = fixture(true);
    *f.shared.hold_start.lock().unwrap() = true;
    f.preview.start(OWNER).expect("start");
    f.complete_pick(PickOutcome::Selected(f.target()));
    f.wait_for(PreviewState::Initializing);
    let deadline = Instant::now() + Duration::from_secs(2);
    while f.shared.starts.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(f.preview.stop().state, LocalPreviewState::Stopping);
    *f.shared.hold_start.lock().unwrap() = false;
    f.shared.start_released.notify_all();
    f.wait_for(PreviewState::Stopped);
    assert_eq!(f.shared.stops.load(Ordering::SeqCst), 1);
    assert!(!f.preview.has_grant());
    assert!(f.shared.active_sink.lock().unwrap().is_none());
}

#[test]
fn stop_is_idempotent() {
    let f = fixture(true);
    assert_eq!(f.preview.stop().state, LocalPreviewState::Ready);
    f.start_capturing();
    for _ in 0..3 {
        assert_eq!(f.preview.stop().state, LocalPreviewState::Stopped);
    }
    assert_eq!(f.shared.stops.load(Ordering::SeqCst), 1);
}

#[test]
fn displays_are_reported_with_negative_coordinates() {
    let f = fixture(true);
    let displays = f.preview.displays().expect("displays");
    assert_eq!(displays.epoch, 1);
    assert_eq!(
        (displays.displays[0].x, displays.displays[0].y),
        (-1920, 71)
    );
}

// T-CAP-SEC-09: ordinary logs carry counters and fixed identifiers only: never the target
// label (a window title), never pixels, and every key is allowlisted.
#[test]
fn logs_contain_no_target_labels_or_pixels() {
    let f = fixture(true);
    let generation = f.start_capturing();
    f.deliver(1);
    let _ = f.preview.take_frame(generation, 1);
    assert_eq!(
        f.preview.status().target.map(|t| t.label),
        Some(PRIVATE_TITLE.to_owned())
    );
    f.preview.stop();
    f.preview.start(OWNER).expect("start");
    f.complete_pick(PickOutcome::Cancelled);

    let text = f.log_text();
    for expected in [
        "preview.start_requested",
        "preview.capturing",
        "preview.first_frame",
        "preview.ended",
        "preview.picker_cancelled",
    ] {
        assert!(text.contains(expected), "missing {expected}:\n{text}");
    }
    assert!(
        !text.contains("Q3 salaries"),
        "target label leaked:\n{text}"
    );
    assert!(!text.contains("xlsx"), "target label leaked:\n{text}");
    assert!(
        !text.contains(REDACTED),
        "a non-allowlisted key was logged:\n{text}"
    );
    for line in f.log.lines() {
        assert!(line.len() < 400, "unexpectedly long log line: {line}");
    }
}
