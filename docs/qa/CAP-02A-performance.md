# CAP-02A — Capture and preview performance (DEV-W1)

| Field | Value |
|---|---|
| Phase | CAP-02A |
| Date | 2026-10-28 |
| Scope | Windows.Graphics.Capture provider and local-preview host, measured on real frames. Not end-to-end remote latency: no network path exists |
| Evidence | `docs/qa/evidence/CAP-02A/perf/` (JSONL from the tools, CSV from external sampling), `provider/`, `app/` |

All numbers below were measured. Targets from the prompt (30 fps, 720p, 30 min, 10 cycles) are hypotheses; this report says whether each was met.

## 1. Environment

| Item | Value |
|---|---|
| CPU | 12th Gen Intel Core i9-12900K. Windows reports **16 logical processors** (`[Environment]::ProcessorCount`); CPU % below is of the whole machine (×16 = % of one logical processor) |
| GPUs | NVIDIA GeForce RTX 3090 (driver 32.0.16.1088); Intel UHD Graphics 770 (32.0.101.7088) |
| Adapter used for capture | **NVIDIA GeForce RTX 3090**, the default adapter of `D3D11CreateDevice(nullptr, HARDWARE)`, reported by every bench run |
| OS | Windows 11 Enterprise LTSC 10.0.26100 (build 26100), x64 |
| Displays | DISPLAY1 (−1920, 71), DISPLAY2 (−3840, 71), DISPLAY3 (0, 0, primary); each 1920×1080, 60 Hz, 100% |
| Build | Release (`cargo build --release`), rustc 1.99.1 |
| Frame pool | `CreateFreeThreaded`, BGRA8, 2 buffers; capture not throttled (`max_fps = 0`) |
| Preview path | CPU readback (staging texture) ≤ 30/s, area-averaged to ≤ 1280×720 RGBA, 1-slot latest-wins buffer |
| Source | Synthetic test-pattern window (GDI, repaints ≈ 64/s), kept topmost; its own repaint count is logged (`pattern_paints`) |
| Sampling | `scripts/capture-bench-windows.ps1`: `Get-Process` every 2–10 s from outside; GPU via `\GPU Engine(pid_*)\Utilization Percentage` (soak run 2) |

## 2. Results by measurement

| ID | Measurement | Result | Evidence |
|---|---|---|---|
| CAP-PERF-01 | Time to first valid frame | **Window:** 143.5–287.8 ms over 140 starts (100-cycle run on DISPLAY3 mean 167.6 ms; 30-cycle mean 163.1; 10-cycle mean 164.9). **Monitor:** 114–138 ms (all three displays). **Through the preview host** (stand-in picker, starter thread, first preview frame pulled): 182–382 ms | `perf/perf-*-cycles*.jsonl`, `provider/real-device-run2.log`, `provider/host-real-device-run1.log` |
| CAP-PERF-02 | Native capture FPS | **720p window, 60 s:** 3,338 frames, mean **55.6 fps**; 1-s windows min 51 / p10 53 / median 56 / max 60; 0 windows below 29. **1080p window (1906×1073), live part:** median 50.2, min 46. **Soak run 1, live 12.4 min:** median 52, p5 45, min 31. Bounded by the 60 Hz refresh | `perf/perf-720p-window.jsonl`, `perf/perf-1080p-window-to-720p.jsonl`, `perf/soak-30min-host.jsonl` |
| CAP-PERF-03 | Displayed preview FPS | **In the WebView: NOT RUN.** The app-level picker flow was not completed (test results T-DEV-21). **Preview frames served by the host to a UI-like puller:** median 22.5 fps (20.2–23.7) with the 30/s readback cap (soak run 1) | `perf/soak-30min-host.jsonl` (`pulled_fps`) |
| CAP-PERF-04 | Frame drops | Provider `dropped` = 0 and `superseded` = 0 in every run; nothing delivered after stop (0 in 160+ stops). Preview `skipped` (UI had not yet taken the previous frame): 1,210 of 38,412 captured frames (3.1%) in soak run 1. Frames the OS itself skipped are not observable through WGC | `perf/*.jsonl` |
| CAP-PERF-05 | Process CPU | 720p window + 30/s readback: **0.84% mean** (max 1.12%) of the machine ≈ 13% of one logical processor. 1080p → 720p: 0.83% mean. Soak run 1 (800×450 window), live period: 0.44% mean | `perf/*.process.csv` |
| CAP-PERF-06 | Working set | 720p bench 35–50 MB; soak run 1: 48.4–53.1 MB, private 54.3–57.8 MB, quartile means 54.5 / 56.0 / 54.3 / 54.3 MB: **no growth trend over 30 min** | `perf/soak-30min-host.process.csv` |
| CAP-PERF-07 | Cleanup after stop | `stop` returns in 1–17 ms; no frame or event after stop; threads drop from 29 to 7 after the run; capture-session objects closed. **Not fully returned:** about 3 process handles per run (1 per `GraphicsCaptureItem`, 2 per free-threaded frame pool), measured in isolation (`perf/diagnostic-handles-per-step.log`) | `perf/perf-100-cycles-primary.process.csv` |
| CAP-PERF-08 | GPU usage | Soak run 2 (800×450 window, 30/s readback, 30 min): GPU 3D engine of the process **0.90% mean, 1.23% max**; copy engine 0. Per-process `\GPU Engine` counters; samples from t = 185 s (the wrapper's sampler failed on its first GPU sample and was replaced by `perf/attach-sampler.ps1`) | `perf/soak-30min-host-run2.process.csv` |
| CAP-PERF-09 | During display resolution changes | **NOT RUN**: changing display resolution or scaling would modify user display settings, which the phase may not do without approval. Window resizes were measured instead (626×473 → 886×593 and 786×593 → 986×693, followed correctly) | `provider/*` |
| CAP-PERF-10 | Repeated start/stop | **10/10, 30/30 and 100/100** cycles on DISPLAY3 with frames each time. On DISPLAY1 one 100-cycle run had only 6/100, because the source stalled from cycle 6 on (§5) | `perf/perf-10-cycles.jsonl`, `perf/perf-30-cycles-trend.jsonl`, `perf/perf-100-cycles-primary.jsonl`, `perf/perf-100-cycles-trend.jsonl` |

## 3. Readback cost (preview path)

| Content | Preview output | Readbacks | Mean | Max |
|---|---|---|---|---|
| 1266×713 (720p window) | 1266×713 (no scaling) | 1,787 in 60 s | 3.25 ms | 5.55 ms |
| 1906×1073 (1080p window) | 1280×720 (area average) | 591 in 20 live s | 9.40 ms | 16.65 ms |
| 786×443 (soak window) | 786×443 | 16,670 in 30 min (run 1) | — | — |

- **Cost:** the readback runs inside the frame callback, holding one of the pool's 2 buffers. At 1080p → 720p it takes about 9 ms, well inside a 16.7 ms frame interval.
- **Tuning:** a GPU-side scaler (VID-04's video processor) would remove most of this CPU work.
- **Preview rate:** the served preview rate stays below the 30/s cap (≈ 23/s). The throttle is measured from the previous readback, and frames arrive on a 60 Hz grid, so each interval overshoots by up to one frame. A deadline-based throttle would reach ≈ 30/s. It was not changed in this phase.

## 4. Stability

| Run | Duration | Result |
|---|---|---|
| Soak run 1 (host level, real WGC frames, UI-like puller) | 1,800 s | No crash, no failure, 0 invalid frames, stop 5.2 ms, no frame and no grant after stop. Frames flowed for the first **745 s**; from 746 s the source delivered no frames until the end (§5). The run continued in `Capturing` without errors |
| Soak run 2 (same, with minimized-state logging, auto-restore and GPU) | 1,800 s | **No crash, no failure, frames for the whole 30 min**: 96,837 frames; 5-s capture rate min 38 / p5 47 / median 54 / max 60 fps; served preview median 22.8 fps; 0 invalid frames; stop 3.2 ms; no frame or grant after stop. The window was never minimized (0 restores). CPU 0.45% mean; working set 44.9–48.0 MB; private memory quartile means 55.6 / 56.5 / 56.1 / 56.3 MB (flat); handles 440 → 430; threads 25 → 22 |
| Provider and host real-device suites | ≈ 10 s each | 11/11 tests pass (final run) |

## 5. Observed OS behaviour that limits measurements

1. **No frames for a covered or minimized window.**
   - The source kept repainting, but WGC stopped delivering frames. New sessions on the same window then got no frame at all.
   - Pool starvation was ruled out: no frame was found queued in the pool during stalls, `superseded` = 0.
   - Keeping the window topmost removed most stalls. Displays are not affected.
   - Details: [capture pipeline §9](../architecture/CAP-02A-capture-pipeline.md).
2. **Secondary displays:**
   - Windows placed on DISPLAY1/DISPLAY2 stalled intermittently even when topmost.
   - DISPLAY3 ran 100/100 cycles.
   - All three monitors reported `Active` in WMI; the display timeout is "never"; no screen saver is configured.
   - **Root cause UNVERIFIED.**
   - In session 1, the test-pattern window and TP Remote were both found *minimized* by something outside the tools, which would explain stalls.
   - Soak run 2 logged the window's minimized state: it was never minimized, and frames flowed for the whole 30 minutes. That neither confirms nor refutes the cause of soak run 1's stall.
3. **Bench runs affected by these stalls:**
   - `bench-1080p-window-to-720p`, `bench-10-cycles` (first attempts, not topmost);
   - `perf-1080p-window-to-720p-run2` and `perf-100-cycles-trend` (DISPLAY1);
   - `diag-A/B/E` (diagnostics).
   - They are kept as evidence, are **not** used as performance results, and are counted in the test results.

## 6. Not measured in this phase

- Displayed FPS in the WebView, and app-process CPU/GPU while previewing: app-level runs NOT RUN.
- Behaviour during display resolution or scale changes, monitor unplug, UAC and the lock screen.
- DXGI Desktop Duplication comparison (U-11).
- HDR content.
- The Intel adapter as the capture device.
- End-to-end latency. The 200 ms remote target remains a VID-04/XPLAT-06 item; nothing here measures it.

## 7. CAP-02A-R2 note

- R2 did not repeat any performance run. The capture, host and readback code are unchanged, so the CAP-02A results stand.
- App-level measurements stay **NOT RUN**: displayed FPS in the WebView, app-process CPU/GPU while previewing, and app-level start/stop cycles. No target selection was completed in the picker ([test results §10](CAP-02A-test-results.md#10-cap-02a-r2-reconciliation)).
- **Related finding:** windows on this desktop were repeatedly minimized from outside the tools, and covered by other windows. A minimized window delivers no frames (§5). This likely explains the earlier stalls on secondary displays and soak run 1's stall at 746 s, but it was not observed for those runs. Soak run 2, which logged the minimized state and could restore the window, had none.
