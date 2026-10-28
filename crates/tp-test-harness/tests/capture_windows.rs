//! Windows.Graphics.Capture on the real device (CAP-02A, T-CAP-WIN-01 … 10).
//!
//! Tests that only read OS state run with `cargo test`. Tests that open windows or
//! capture are `#[ignore]` so routine runs stay non-intrusive; real-device QA runs them:
//!   cargo test -p tp-test-harness --test capture_windows -- --ignored --test-threads=1 --nocapture
//!
//! Window tests capture only the synthetic test-pattern window. Monitor tests capture each
//! monitor for about a second: frames are never read back, stored or logged; only their
//! size and timing are checked (approved for CAP-02A).
#![cfg(windows)]

use std::time::{Duration, Instant};

use tp_capture::native::Provider;
use tp_capture::wgc::testing::{
    PATTERN_BOTTOM_RGB, PATTERN_TOP_RGB, TestPatternWindow, monitor_item_name, target_for_monitor,
    target_for_window,
};
use tp_capture::wgc::{TargetOrigin, is_supported};
use tp_contracts::capability::Availability;
use tp_contracts::capture::{
    CaptureConfig, CaptureEvent, CaptureProvider, CaptureStream, PixelFormat, PixelSize, TargetKind,
};
use tp_contracts::error::ErrorCategory;
use tp_test_harness::capture::{Recorder, close_to, intervals_ms, pixel, wait_until};

const CONFIG: CaptureConfig = CaptureConfig {
    max_fps: 0,
    format: PixelFormat::Bgra8,
    show_cursor: false,
};
const WAIT: Duration = Duration::from_secs(3);

// T-CAP-WIN-01: support detection is consistent with the advertised capabilities.
#[test]
fn support_detection_matches_capabilities() {
    let capabilities = Provider::new().capabilities();
    let expected = if is_supported() {
        Availability::Available
    } else {
        Availability::Unavailable(tp_contracts::capability::UnavailableReason::PlatformUnsupported)
    };
    assert_eq!(capabilities.availability, expected);
    assert!(capabilities.system_picker);
    assert!(capabilities.gpu_frames);
    println!(
        "wgc_supported={} max_fps={}",
        is_supported(),
        capabilities.max_fps
    );
}

// T-CAP-WIN-02/03/04 (generic part): invariants that hold for any monitor layout.
#[test]
fn display_enumeration_is_consistent() {
    let provider = Provider::new();
    let layout = provider.displays().expect("enumerate displays");
    assert!(!layout.displays.is_empty());
    assert_eq!(layout.displays.iter().filter(|d| d.is_primary).count(), 1);
    for (index, display) in layout.displays.iter().enumerate() {
        assert_eq!(display.id.0 as usize, index);
        assert!(display.size_px.width > 0 && display.size_px.height > 0);
        assert_eq!(display.size_px.width, display.bounds_os.width);
        assert_eq!(display.size_px.height, display.bounds_os.height);
        assert!(
            !display.name.is_empty() && display.name.chars().all(|c| c.is_ascii_alphanumeric())
        );
        if display.is_primary {
            assert_eq!((display.bounds_os.x, display.bounds_os.y), (0, 0));
        }
    }
    for (i, a) in layout.displays.iter().enumerate() {
        for b in &layout.displays[i + 1..] {
            let (ax, ay) = (i64::from(a.bounds_os.x), i64::from(a.bounds_os.y));
            let (bx, by) = (i64::from(b.bounds_os.x), i64::from(b.bounds_os.y));
            let overlap = ax < bx + i64::from(b.bounds_os.width)
                && bx < ax + i64::from(a.bounds_os.width)
                && ay < by + i64::from(b.bounds_os.height)
                && by < ay + i64::from(a.bounds_os.height);
            assert!(!overlap, "{} overlaps {}", a.name, b.name);
        }
    }
    assert_eq!(provider.displays().expect("again").epoch, layout.epoch);
    for d in &layout.displays {
        println!(
            "display index={} name={} x={} y={} w={} h={} scale={:?}% refresh={:?}Hz primary={}",
            d.id.0,
            d.name,
            d.bounds_os.x,
            d.bounds_os.y,
            d.size_px.width,
            d.size_px.height,
            d.scale_percent,
            d.refresh_hz,
            d.is_primary
        );
    }
}

// T-CAP-WIN-02/03/04 (DEV-W1): the workstation's three 1080p monitors, two of them at
// negative virtual-desktop coordinates.
#[test]
#[ignore = "real-device: DEV-W1 monitor layout"]
fn dev_w1_has_three_1080p_monitors_with_negative_origins() {
    let layout = Provider::new().displays().expect("enumerate displays");
    assert_eq!(layout.displays.len(), 3, "DEV-W1 has three monitors");
    for d in &layout.displays {
        assert_eq!(
            (d.size_px.width, d.size_px.height),
            (1920, 1080),
            "{}",
            d.name
        );
    }
    assert!(layout.displays.iter().filter(|d| d.bounds_os.x < 0).count() >= 2);
    assert!(is_supported(), "WGC must be supported on DEV-W1");
}

// T-CAP-WIN-04/05/06: every monitor delivers a frame of its own size, and the target is
// matched to the right display (target-to-monitor mapping validation).
#[test]
#[ignore = "real-device: captures each monitor for about a second (metadata only)"]
fn each_monitor_delivers_frames_of_its_own_size() {
    let provider = Provider::new();
    let layout = provider.displays().expect("enumerate displays");
    for display in &layout.displays {
        println!(
            "monitor {} item_name={:?}",
            display.name,
            monitor_item_name(display.id).expect("item name")
        );
        let target = target_for_monitor(display.id).expect("monitor target");
        assert_eq!(target.origin(), TargetOrigin::Programmatic);
        let recorder = Recorder::new();
        let started = Instant::now();
        let stream = provider
            .start(target, CONFIG, recorder.clone())
            .expect("start monitor capture");
        assert!(
            wait_until(WAIT, || recorder.frames() >= 1),
            "no frame from {}",
            display.name
        );
        let first = recorder.first_frame_at().expect("first frame") - started;
        let info = stream.target().clone();
        let stats = stream.stop().expect("stop");
        recorder.mark_stopped();
        let metas = recorder.metas();
        assert_eq!(metas[0].seq, 1);
        assert_eq!(metas[0].size, display.size_px, "{}", display.name);
        assert_eq!(info.size, display.size_px);
        assert_eq!(
            info.kind,
            TargetKind::Display,
            "{} not matched",
            display.name
        );
        assert_eq!(info.display, Some(display.id));
        assert_eq!(stats.frames_delivered, recorder.frames());
        println!(
            "monitor {} first_frame_ms={} frames={} kind={}",
            display.name,
            first.as_millis(),
            recorder.frames(),
            info.kind.as_str()
        );
        // Give any in-flight callback the chance to (wrongly) deliver after stop.
        let _ = wait_until(Duration::from_millis(200), || {
            recorder.deliveries_after_stop() > 0
        });
        assert_eq!(recorder.deliveries_after_stop(), 0);
    }
}

/// Topmost: WGC delivered no frames for this GDI window while another window covered it
/// (observed on DEV-W1, docs/qa/evidence/CAP-02A/perf/diag-*).
fn open_pattern(title: &str) -> TestPatternWindow {
    let window =
        TestPatternWindow::open(title, 120, 120, 640, 480).expect("open test-pattern window");
    window.set_topmost(true).expect("topmost");
    window
}

// T-CAP-WIN-05/06/07: real frames of the synthetic window, with correct metadata and the
// pattern's known colours (proof that the pixels came from the OS).
#[test]
#[ignore = "real-device: opens and captures the synthetic test-pattern window"]
fn window_capture_delivers_real_test_pattern_frames() {
    let window = open_pattern("TP Remote test pattern (T-CAP-WIN-06)");
    let provider = Provider::new();
    let recorder = Recorder::new();
    let started = Instant::now();
    let stream = provider
        .start(
            target_for_window(window.hwnd()).expect("target"),
            CONFIG,
            recorder.clone(),
        )
        .expect("start window capture");
    assert_eq!(stream.target().kind, TargetKind::Unknown);
    assert!(
        wait_until(WAIT, || recorder.frames() >= 30),
        "fewer than 30 frames in 3 s"
    );
    let first_ms = (recorder.first_frame_at().expect("first") - started).as_millis();

    recorder.request_readback(PixelSize {
        width: 640,
        height: 480,
    });
    assert!(
        wait_until(WAIT, || recorder.take_readback().is_some_and(
            |(size, rgba)| {
                let top = pixel(&rgba, size, size.width / 2, size.height * 35 / 100);
                let bottom = pixel(&rgba, size, size.width / 10, size.height * 90 / 100);
                println!(
                    "readback size={}x{} top={top:?} bottom={bottom:?}",
                    size.width, size.height
                );
                // The bottom half is grey, unless the moving white bar is at the sampled column.
                close_to(top, PATTERN_TOP_RGB, 8)
                    && (close_to(bottom, PATTERN_BOTTOM_RGB, 8)
                        || close_to(bottom, [255, 255, 255], 8))
            }
        )),
        "pattern colours not found in the captured frame"
    );
    assert_eq!(recorder.readback_errors(), 0);

    let generation = stream.generation();
    let stats = stream.stop().expect("stop");
    recorder.mark_stopped();
    let metas = recorder.metas();
    for (i, meta) in metas.iter().enumerate() {
        assert_eq!(meta.seq, i as u64 + 1, "sequence must be contiguous");
        assert_eq!(meta.generation, generation);
        assert_eq!(meta.format, PixelFormat::Bgra8);
        assert!(meta.size.width > 0 && meta.size.height > 0);
    }
    let intervals = intervals_ms(&metas);
    assert!(
        metas
            .windows(2)
            .all(|w| w[1].captured_at >= w[0].captured_at),
        "OS timestamps must not go backwards"
    );
    let span = metas
        .last()
        .unwrap()
        .captured_at
        .saturating_duration_since(metas[0].captured_at)
        .as_secs_f64();
    println!(
        "window first_frame_ms={first_ms} frames={} fps={:.1} max_interval_ms={:.1}",
        metas.len(),
        (metas.len() - 1) as f64 / span,
        intervals.iter().cloned().fold(0.0, f64::max)
    );
    assert_eq!(stats.frames_delivered, metas.len() as u64);
    let _ = wait_until(Duration::from_millis(300), || {
        recorder.deliveries_after_stop() > 0
    });
    assert_eq!(recorder.deliveries_after_stop(), 0, "delivery after stop");
    window.close();
}

// T-CAP-WIN-08: resizing the target resizes the frames and raises TargetResized.
#[test]
#[ignore = "real-device: opens, resizes and captures the synthetic window"]
fn resizing_the_target_resizes_frames() {
    let window = open_pattern("TP Remote test pattern (T-CAP-WIN-08)");
    let provider = Provider::new();
    let recorder = Recorder::new();
    let stream = provider
        .start(
            target_for_window(window.hwnd()).expect("target"),
            CONFIG,
            recorder.clone(),
        )
        .expect("start");
    assert!(wait_until(WAIT, || recorder.frames() >= 1));
    let before = recorder.metas()[0].size;
    window.resize(900, 600).expect("resize");
    assert!(
        wait_until(WAIT, || recorder
            .events()
            .iter()
            .any(|e| matches!(e, CaptureEvent::TargetResized(_)))),
        "no TargetResized event"
    );
    let resized_to = recorder.events().iter().find_map(|e| match e {
        CaptureEvent::TargetResized(size) => Some(*size),
        _ => None,
    });
    let target_size = resized_to.expect("size");
    assert!(
        wait_until(WAIT, || recorder
            .metas()
            .last()
            .is_some_and(|m| m.size == target_size)),
        "frames did not take the new size"
    );
    println!(
        "resized {}x{} -> {}x{}",
        before.width, before.height, target_size.width, target_size.height
    );
    assert_ne!(before, target_size);
    stream.stop().expect("stop");
    window.close();
}

// T-CAP-WIN-09: closing the target ends the run: TargetClosed, then no more frames.
#[test]
#[ignore = "real-device: opens, captures and closes the synthetic window"]
fn closing_the_target_ends_the_capture() {
    let window = open_pattern("TP Remote test pattern (T-CAP-WIN-09)");
    let provider = Provider::new();
    let recorder = Recorder::new();
    let stream = provider
        .start(
            target_for_window(window.hwnd()).expect("target"),
            CONFIG,
            recorder.clone(),
        )
        .expect("start");
    assert!(wait_until(WAIT, || recorder.frames() >= 1));
    window.close();
    assert!(
        wait_until(WAIT, || recorder
            .events()
            .contains(&CaptureEvent::TargetClosed)),
        "no TargetClosed event"
    );
    let frames_at_close = recorder.frames();
    assert!(!wait_until(Duration::from_millis(300), || recorder
        .frames()
        > frames_at_close));
    let stats = stream.stop().expect("stop after close is harmless");
    println!("closed after {} frames; stats={stats:?}", frames_at_close);
}

// T-CAP-WIN-10: resource failure handling — a target whose window is already gone fails
// with a typed error or ends at once; stop and drop are idempotent.
#[test]
#[ignore = "real-device: opens and closes the synthetic window"]
fn a_target_that_is_already_gone_fails_cleanly() {
    let window = open_pattern("TP Remote test pattern (T-CAP-WIN-10)");
    let target = target_for_window(window.hwnd()).expect("target");
    window.close();
    let recorder = Recorder::new();
    match Provider::new().start(target, CONFIG, recorder.clone()) {
        Err(error) => {
            println!("start on a closed target: {error}");
            assert_eq!(error.category(), ErrorCategory::PlatformUnavailable);
        }
        Ok(stream) => {
            assert!(
                wait_until(WAIT, || recorder
                    .events()
                    .contains(&CaptureEvent::TargetClosed)),
                "a closed target must end the run"
            );
            drop(stream);
            println!("start on a closed target: started, then TargetClosed");
        }
    }
    assert_eq!(recorder.frames(), 0, "no frames from a closed window");
}

// Stop releases capture promptly and repeatedly (CAP-PERF-10 smoke version).
#[test]
#[ignore = "real-device: ten capture start/stop cycles on the synthetic window"]
fn ten_start_stop_cycles_release_cleanly() {
    let window = open_pattern("TP Remote test pattern (cycles)");
    let provider = Provider::new();
    for cycle in 1..=10 {
        let recorder = Recorder::new();
        let started = Instant::now();
        let stream = provider
            .start(
                target_for_window(window.hwnd()).expect("target"),
                CONFIG,
                recorder.clone(),
            )
            .expect("start");
        assert!(
            wait_until(WAIT, || recorder.frames() >= 5),
            "cycle {cycle}: no frames"
        );
        let first_ms = (recorder.first_frame_at().unwrap() - started).as_millis();
        let stop_started = Instant::now();
        stream.stop().expect("stop");
        let stop_ms = stop_started.elapsed().as_millis();
        recorder.mark_stopped();
        assert!(!wait_until(Duration::from_millis(100), || recorder
            .deliveries_after_stop()
            > 0));
        println!(
            "cycle {cycle}: first_frame_ms={first_ms} stop_ms={stop_ms} frames={}",
            recorder.frames()
        );
        assert!(stop_ms < 1000);
    }
    window.close();
}
