//! Drives the real local-preview host (`tp_core::preview::LocalPreview`) over real
//! Windows.Graphics.Capture frames of the synthetic test-pattern window, pulling preview
//! frames like the UI does (up to 60 requests per second, next request after the previous
//! one). Prints the host status as JSON lines. CAP-02A host-level substitute for the
//! app-level runs; the picker is replaced by `WindowPickerForTests` (test-only).
//!
//!   preview-soak [--seconds N] [--cycles N] [--x X --y Y --width W --height H]
//!
//! Process CPU and memory are sampled from outside (scripts/capture-bench-windows.ps1).

#[cfg(windows)]
fn main() {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use serde_json::json;
    use tp_capture::wgc::testing::{TestPatternWindow, WindowPickerForTests};
    use tp_contracts::capture::PickerOwner;
    use tp_contracts::ipc::LocalPreviewState;
    use tp_contracts::ipc::preview_frame::Header;
    use tp_core::clock::MonotonicClock;
    use tp_core::preview::{LocalPreview, PreviewConfig};
    use tp_diagnostics::Logger;
    use tp_test_harness::capture::wait_until;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let number = |name: &str, default: i64| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let seconds = number("--seconds", 30).max(1) as u64;
    let cycles = number("--cycles", 1).max(1) as u32;
    let window = TestPatternWindow::open(
        "TP Remote Test Pattern (preview-soak)",
        number("--x", -1700) as i32,
        number("--y", 200) as i32,
        number("--width", 1280) as i32,
        number("--height", 720) as i32,
    )
    .expect("open test-pattern window");
    window.set_topmost(true).expect("topmost");
    let _ = wait_until(Duration::from_millis(300), || false);

    let preview = LocalPreview::new(
        WindowPickerForTests::new(window.hwnd()),
        Arc::new(MonotonicClock::new()),
        Logger::stderr(),
        PreviewConfig::default(),
    );
    println!(
        "{}",
        json!({"event": "soak.start", "pid": std::process::id(), "seconds": seconds, "cycles": cycles})
    );
    let mut failures = 0u32;
    for cycle in 1..=cycles {
        let started = Instant::now();
        let generation = match preview.start(PickerOwner(0)) {
            Ok(status) => status.generation,
            Err(error) => {
                println!(
                    "{}",
                    json!({"event": "soak.start_failed", "cycle": cycle, "error": format!("{error:?}")})
                );
                failures += 1;
                continue;
            }
        };
        if !wait_until(Duration::from_secs(5), || {
            preview.status().state == LocalPreviewState::Capturing
        }) {
            println!(
                "{}",
                json!({"event": "soak.not_capturing", "cycle": cycle, "state": format!("{:?}", preview.status().state)})
            );
            failures += 1;
            preview.stop();
            continue;
        }
        let mut drawn = 0u64;
        let mut invalid = 0u64;
        let mut last_report = Instant::now();
        let mut drawn_at_report = 0u64;
        while started.elapsed() < Duration::from_secs(seconds) {
            let frame = preview.take_frame(generation, drawn as u32);
            match Header::decode(&frame) {
                Some(h) if h.width > 0 => {
                    if h.generation == generation && h.frame_len() == Some(frame.len()) {
                        drawn += 1;
                    } else {
                        invalid += 1;
                    }
                }
                Some(_) => {}
                None => invalid += 1,
            }
            if last_report.elapsed() >= Duration::from_secs(5) {
                let s = preview.status();
                let served = (drawn - drawn_at_report) as f64 / last_report.elapsed().as_secs_f64();
                println!(
                    "{}",
                    json!({"event": "soak.tick", "cycle": cycle, "t_s": started.elapsed().as_secs(),
                           "state": format!("{:?}", s.state), "captured": s.stats.captured_frames,
                           "previewed": s.stats.previewed_frames, "skipped": s.stats.skipped_frames,
                           "capture_fps": s.stats.capture_fps, "preview_fps_host": s.stats.preview_fps,
                           "pulled_fps": (served * 10.0).round() / 10.0,
                           "frame": format!("{}x{}", s.stats.frame_width, s.stats.frame_height),
                           "preview": format!("{}x{}", s.stats.preview_width, s.stats.preview_height)})
                );
                drawn_at_report = drawn;
                last_report = Instant::now();
            }
            // The UI requests the next frame on the next animation frame (~16 ms).
            std::thread::sleep(Duration::from_millis(16));
        }
        let before_stop = preview.status();
        let stop_started = Instant::now();
        let after = preview.stop();
        let stop_ms = stop_started.elapsed().as_secs_f64() * 1000.0;
        let late = Header::decode(&preview.take_frame(generation, 0)).is_some_and(|h| h.width > 0);
        println!(
            "{}",
            json!({"event": "soak.cycle", "cycle": cycle, "first_frame_ms": before_stop.stats.first_frame_ms,
                   "captured": before_stop.stats.captured_frames, "previewed": before_stop.stats.previewed_frames,
                   "skipped": before_stop.stats.skipped_frames, "pulled": drawn, "invalid": invalid,
                   "elapsed_ms": before_stop.stats.elapsed_ms, "stop_ms": (stop_ms * 10.0).round() / 10.0,
                   "state_after_stop": format!("{:?}", after.state), "grant_after_stop": preview.has_grant(),
                   "frame_after_stop": late, "dropped": after.stats.dropped_frames})
        );
        if late || preview.has_grant() || invalid > 0 || after.state != LocalPreviewState::Stopped {
            failures += 1;
        }
    }
    window.close();
    println!("{}", json!({"event": "soak.end", "failures": failures}));
    std::process::exit(i32::from(failures > 0));
}

#[cfg(not(windows))]
fn main() {
    eprintln!("preview-soak is Windows-only");
    std::process::exit(2);
}
