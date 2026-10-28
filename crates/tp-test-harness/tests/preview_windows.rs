//! The local-preview host over real Windows.Graphics.Capture (CAP-02A, host level). The
//! picker is replaced by `WindowPickerForTests`, which "selects" the synthetic test-pattern
//! window; everything after the selection is the production path (grant, starter thread,
//! WGC stream, sink, readback, frame slot, stop). Real-device and window-opening, so
//! `#[ignore]`:
//!   cargo test -p tp-test-harness --test preview_windows -- --ignored --test-threads=1 --nocapture
#![cfg(windows)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use tp_capture::wgc::testing::{PATTERN_TOP_RGB, TestPatternWindow, WindowPickerForTests};
use tp_contracts::capture::{PickerOwner, PixelSize};
use tp_contracts::ipc::preview_frame::{HEADER_LEN, Header};
use tp_contracts::ipc::{LocalPreviewState, PreviewOutcome};
use tp_core::clock::MonotonicClock;
use tp_core::preview::{LocalPreview, PreviewConfig};
use tp_diagnostics::{Logger, MemorySink, REDACTED};
use tp_test_harness::capture::{close_to, pixel, wait_until};

const TITLE: &str = "TP Remote Test Pattern (preview host)";
const WAIT: Duration = Duration::from_secs(5);

struct Fixture {
    window: TestPatternWindow,
    preview: LocalPreview<WindowPickerForTests>,
    log: Arc<MemorySink>,
}

fn fixture() -> Fixture {
    let window = TestPatternWindow::open(TITLE, 120, 120, 800, 600).expect("open window");
    window.set_topmost(true).expect("topmost");
    let log = Arc::new(MemorySink::new());
    let preview = LocalPreview::new(
        WindowPickerForTests::new(window.hwnd()),
        Arc::new(MonotonicClock::new()),
        Logger::new(log.clone()),
        PreviewConfig::default(),
    );
    Fixture {
        window,
        preview,
        log,
    }
}

impl Fixture {
    fn start(&self) -> u32 {
        let generation = self
            .preview
            .start(PickerOwner(0))
            .expect("start")
            .generation;
        assert!(
            wait_until(WAIT, || self.preview.status().state
                == LocalPreviewState::Capturing),
            "not capturing: {:?}",
            self.preview.status().state
        );
        generation
    }

    /// Pulls frames like the UI until one arrives.
    fn next_frame(&self, generation: u32) -> Option<(Header, Vec<u8>)> {
        let mut found = None;
        wait_until(WAIT, || {
            let bytes = self.preview.take_frame(generation, 0);
            match Header::decode(&bytes) {
                Some(h) if h.width > 0 => {
                    found = Some((h, bytes));
                    true
                }
                _ => false,
            }
        });
        found
    }
}

// T-CAP-RUN-03 (host level): real frames reach the preview slot with a valid header and
// the pattern's pixels; stop revokes; ordinary logs never contain the window title.
#[test]
#[ignore = "real-device: opens and captures the synthetic window"]
fn the_host_serves_real_pattern_frames_and_stop_revokes_them() {
    let f = fixture();
    let generation = f.start();
    let (header, bytes) = f.next_frame(generation).expect("a preview frame");
    assert_eq!(header.generation, generation);
    assert_eq!(header.frame_len(), Some(bytes.len()));
    let size = PixelSize {
        width: header.width,
        height: header.height,
    };
    let rgba = &bytes[HEADER_LEN..];
    let top = pixel(rgba, size, size.width / 2, size.height * 35 / 100);
    println!(
        "host frame {}x{} seq={} top={top:?}",
        header.width, header.height, header.seq
    );
    assert!(
        close_to(top, PATTERN_TOP_RGB, 8),
        "pattern colour not found: {top:?}"
    );

    assert!(wait_until(WAIT, || f.preview.status().stats.capture_fps > 0.0));
    let status = f.preview.status();
    assert_eq!(
        status.target.as_ref().map(|t| t.label.as_str()),
        Some(TITLE)
    );
    assert!(status.stats.first_frame_ms.is_some());
    println!(
        "host stats: captured={} previewed={} capture_fps={} first_frame_ms={:?}",
        status.stats.captured_frames,
        status.stats.previewed_frames,
        status.stats.capture_fps,
        status.stats.first_frame_ms
    );

    let stopped = f.preview.stop();
    assert_eq!(stopped.state, LocalPreviewState::Stopped);
    assert_eq!(stopped.last_outcome, Some(PreviewOutcome::StoppedByUser));
    assert!(!f.preview.has_grant());
    let _ = wait_until(Duration::from_millis(300), || false);
    assert_eq!(
        Header::decode(&f.preview.take_frame(generation, 0)),
        Some(Header::EMPTY)
    );

    let log = f.log.lines().join("\n");
    assert!(log.contains("event=preview.capturing"), "{log}");
    assert!(
        !log.contains("Test Pattern"),
        "window title in the log:\n{log}"
    );
    assert!(!log.contains(REDACTED), "non-allowlisted key:\n{log}");
    f.window.close();
}

// T-CAP-WIN-08 (host level): a resized target is reported with its new frame size.
#[test]
#[ignore = "real-device: opens, resizes and captures the synthetic window"]
fn the_host_follows_a_resized_target() {
    let f = fixture();
    f.start();
    assert!(wait_until(WAIT, || f.preview.status().stats.frame_width > 0));
    let before = f.preview.status().stats;
    f.window.resize(1000, 700).expect("resize");
    assert!(
        wait_until(WAIT, || f.preview.status().stats.frame_width
            != before.frame_width),
        "frame size did not change"
    );
    let after = f.preview.status().stats;
    println!(
        "host resize {}x{} -> {}x{} (preview {}x{})",
        before.frame_width,
        before.frame_height,
        after.frame_width,
        after.frame_height,
        after.preview_width,
        after.preview_height
    );
    assert!(
        f.log
            .lines()
            .iter()
            .any(|l| l.contains("event=preview.target_resized"))
    );
    f.preview.stop();
    f.window.close();
}

// T-CAP-WIN-09 (host level): closing the captured window ends the run as targetClosed.
#[test]
#[ignore = "real-device: opens, captures and closes the synthetic window"]
fn the_host_ends_the_run_when_the_target_closes() {
    let f = fixture();
    f.start();
    let Fixture {
        window, preview, ..
    } = f;
    window.close();
    assert!(
        wait_until(WAIT, || preview.status().state
            == LocalPreviewState::Stopped),
        "state after close: {:?}",
        preview.status().state
    );
    let status = preview.status();
    assert_eq!(status.last_outcome, Some(PreviewOutcome::TargetClosed));
    assert!(status.error.is_none());
    assert!(!preview.has_grant());
}

// T-CAP-RUN-08 (host level): ten start/stop cycles through the production host.
#[test]
#[ignore = "real-device: ten preview cycles on the synthetic window"]
fn ten_host_cycles_start_and_stop_cleanly() {
    let f = fixture();
    for cycle in 1..=10 {
        let started = Instant::now();
        let generation = f.start();
        assert!(
            f.next_frame(generation).is_some(),
            "cycle {cycle}: no frame"
        );
        let stop_started = Instant::now();
        let status = f.preview.stop();
        println!(
            "host cycle {cycle}: generation={generation} to_first_preview_ms={} stop_ms={}",
            started.elapsed().as_millis(),
            stop_started.elapsed().as_millis()
        );
        assert_eq!(status.state, LocalPreviewState::Stopped);
        assert!(!f.preview.has_grant());
    }
    assert_eq!(f.preview.status().generation, 10);
    f.window.close();
}
