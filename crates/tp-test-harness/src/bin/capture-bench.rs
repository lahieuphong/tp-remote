//! Capture performance bench (CAP-02A, CAP-PERF-01/02/04/07/10). Captures the synthetic
//! test-pattern window through the real Windows.Graphics.Capture provider, optionally with
//! a preview-style CPU readback, and prints measurements as JSON lines. Process CPU and
//! memory are sampled from outside (scripts/capture-perf-windows.ps1).
//!
//!   capture-bench [--seconds N] [--cycles N] [--x X --y Y --width W --height H]
//!                 [--preview WxH] [--preview-fps N]
//!
//! Only the synthetic window is ever captured; pixels are never stored.

#[cfg(windows)]
fn main() {
    use std::time::{Duration, Instant};

    use serde_json::json;
    use tp_capture::native::Provider;
    use tp_capture::wgc::testing::{TestPatternWindow, pattern_paints, target_for_window};
    use tp_contracts::capture::{
        CaptureConfig, CaptureProvider, CaptureStream, PixelFormat, PixelSize,
    };
    use tp_test_harness::capture::{Recorder, wait_until};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let number =
        |name: &str, default: i64| arg(name).and_then(|v| v.parse().ok()).unwrap_or(default);
    let seconds = number("--seconds", 10).max(1) as u64;
    let cycles = number("--cycles", 1).max(1) as u32;
    let preview_fps = number("--preview-fps", 30).max(0) as u32;
    let preview = arg("--preview").and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some(PixelSize {
            width: w.parse().ok()?,
            height: h.parse().ok()?,
        })
    });
    let window = TestPatternWindow::open(
        "TP Remote Test Pattern (capture-bench)",
        number("--x", 100) as i32,
        number("--y", 100) as i32,
        number("--width", 1280) as i32,
        number("--height", 720) as i32,
    )
    .expect("open test-pattern window");
    let topmost = args.iter().any(|a| a == "--topmost");
    if topmost {
        window.set_topmost(true).expect("topmost");
    }
    // Let the window paint at least once before capture starts.
    let _ = wait_until(Duration::from_millis(300), || false);
    println!(
        "{}",
        json!({"event": "bench.start", "pid": std::process::id(), "seconds": seconds,
               "cycles": cycles, "topmost": topmost, "preview": preview.map(|p| format!("{}x{}", p.width, p.height)),
               "preview_fps": preview_fps})
    );

    let provider = Provider::new();
    let config = CaptureConfig {
        max_fps: 0,
        format: PixelFormat::Bgra8,
        show_cursor: false,
    };
    let mut failures = 0u32;
    for cycle in 1..=cycles {
        let recorder = Recorder::counting();
        let started = Instant::now();
        let stream = match provider.start(
            target_for_window(window.hwnd()).expect("target"),
            config,
            recorder.clone(),
        ) {
            Ok(stream) => stream,
            Err(error) => {
                println!(
                    "{}",
                    json!({"event": "bench.start_failed", "cycle": cycle, "error": error.to_string()})
                );
                failures += 1;
                continue;
            }
        };
        let adapter = stream.adapter().to_owned();
        let content = stream.target().size;
        if !wait_until(Duration::from_secs(3), || recorder.frames() >= 1) {
            println!("{}", json!({"event": "bench.no_frames", "cycle": cycle}));
            failures += 1;
            continue;
        }
        let first_frame_ms =
            (recorder.first_frame_at().expect("first") - started).as_secs_f64() * 1000.0;

        let run = Duration::from_secs(seconds);
        let mut next_readback = Instant::now();
        let mut last_report = Instant::now();
        let mut last = (recorder.frames(), recorder.last_captured_at());
        while started.elapsed() < run {
            if let Some(max) = preview
                && preview_fps > 0
                && Instant::now() >= next_readback
            {
                recorder.request_readback(max);
                let _ = recorder.take_readback();
                next_readback += Duration::from_secs_f64(1.0 / f64::from(preview_fps));
            }
            if last_report.elapsed() >= Duration::from_secs(1) {
                let now = (recorder.frames(), recorder.last_captured_at());
                // Rate from OS frame timestamps, not from wall-clock guesses.
                let fps = match (last.1, now.1) {
                    (Some(a), Some(b)) if now.0 > last.0 && b > a => {
                        (now.0 - last.0) as f64 / b.saturating_duration_since(a).as_secs_f64()
                    }
                    _ => 0.0,
                };
                let rb = recorder.readback_stats();
                println!(
                    "{}",
                    json!({"event": "bench.tick", "cycle": cycle,
                           "t_s": started.elapsed().as_secs_f64(), "frames": now.0,
                           "capture_fps": (fps * 10.0).round() / 10.0, "readbacks": rb.count,
                           "pattern_paints": pattern_paints(),
                           "pattern_minimized": window.is_minimized(),
                           "superseded": stream.diagnostics().superseded,
                           "readback_ms_max": rb.max_us as f64 / 1000.0})
                );
                last = now;
                last_report = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        let diagnostics = stream.diagnostics();
        let stop_started = Instant::now();
        let stats = stream.stop().expect("stop");
        let stop_ms = stop_started.elapsed().as_secs_f64() * 1000.0;
        recorder.mark_stopped();
        let _ = wait_until(Duration::from_millis(200), || {
            recorder.deliveries_after_stop() > 0
        });
        let rb = recorder.readback_stats();
        println!(
            "{}",
            json!({"event": "bench.cycle", "cycle": cycle, "adapter": adapter,
                   "content": format!("{}x{}", content.width, content.height),
                   "first_frame_ms": (first_frame_ms * 10.0).round() / 10.0,
                   "frames": stats.frames_delivered, "frames_dropped": stats.frames_dropped,
                   "mean_capture_fps": (stats.frames_delivered as f64 / seconds as f64 * 10.0).round() / 10.0,
                   "readbacks": rb.count, "readback_errors": recorder.readback_errors(),
                   "readback_ms_avg": if rb.count > 0 { (rb.total_us as f64 / rb.count as f64 / 10.0).round() / 100.0 } else { 0.0 },
                   "readback_ms_max": rb.max_us as f64 / 1000.0,
                   "stop_ms": (stop_ms * 10.0).round() / 10.0,
                   "superseded": diagnostics.superseded,
                   "deliveries_after_stop": recorder.deliveries_after_stop()})
        );
        if recorder.deliveries_after_stop() > 0 {
            failures += 1;
        }
    }
    window.close();
    println!("{}", json!({"event": "bench.end", "failures": failures}));
    std::process::exit(i32::from(failures > 0));
}

#[cfg(not(windows))]
fn main() {
    eprintln!("capture-bench is Windows-only");
    std::process::exit(2);
}
