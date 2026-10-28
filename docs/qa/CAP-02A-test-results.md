# CAP-02A — Test results

| Field | Value |
|---|---|
| Phase | CAP-02A (Windows native capture and local preview) |
| Date | 2026-10-28 |
| Host | DEV-W1: Windows 11 Enterprise LTSC 10.0.26100, i9-12900K, RTX 3090 + UHD 770, 3 × 1920×1080 |
| Commit under test | Working tree on top of `7e1a661` (see report §2) |
| Canonical IDs | [TEST-MATRIX §14–15](TEST-MATRIX.md) |

Status words:
- **PASS**: observed.
- **PASS (host level)**: the production host was driven over real frames, with a test-only stand-in for the picker.
- **PASS (engine level)**: test-double platform.
- **NOT RUN**: not executed, or not corroborated.
- **N/A**: not applicable.

## 1. Gates

`npm run check` gives **9/9 PASS** (`evidence/CAP-02A/gates/npm-check-final.log`):

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets --locked` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS |
| Bindings drift (`npm run bindings:check`) | PASS (21 binding files) |
| `tsc --noEmit` | PASS |
| ESLint (`--max-warnings=0`) | PASS |
| Vitest | PASS |
| Frontend production build | PASS |

Native builds:
- `npm run build:app` (release `tp-desktop.exe`): PASS.
- `cargo build --release -p tp-test-harness --bins`: PASS.
- The baseline before any change also passed 9/9 (`evidence/CAP-02A/baseline/npm-check-baseline.log`).

## 2. Counts

| Suite | BOOT-02 | CAP-02A | New | Notes |
|---|---|---|---|---|
| Rust (routine `cargo test`) | 141 | **189 passed, 0 failed** | +48 | Includes doctests (3 new compile-fail) |
| Rust real-device (`#[ignore]`, run explicitly) | 0 | **11 passed** (final run) | +11 | `capture_windows` 7, `preview_windows` 4 |
| Vitest | 18 | **31 passed** | +13 | `LocalPreviewCard` 8, `localPreview.contract` 5 (R2 added an assertion, not a test) |

The test inventory is in `evidence/CAP-02A/gates/test-names.txt` (200 names, including 11 ignored).

**New Rust tests (+48):**

| Area | Tests |
|---|---|
| `tp-capture` unit | 10 |
| `tp-contracts/tests/preview_fixtures` | 6 |
| `tp-core/tests/preview` | 15 |
| `tp-core/tests/preview_machine` | 2 |
| `tp-core` compile-fail doctests | 3 |
| `tp-desktop/tests/ipc` | 4 |
| `tp-test-harness/tests/workspace_boundaries` | 6 |
| `capture_windows` (non-ignored) | 2 |

**BOOT-01/02 tests changed deliberately.** All still pass; none was removed.

| Test | Change | Reason |
|---|---|---|
| `tp-core` `no_capability_crate_claims_an_implementation` → `no_capability_is_advertised_and_only_capture_has_progressed` | Renamed; tp-capture may be `Partial` on Windows; NONE still asserted | Capture is now implemented locally; remote advertisement unchanged |
| `tp-core` `app_info_reports_this_build_honestly` | Phase `CAP-02A` | Phase constant |
| `workspace_boundaries` `no_production_platform_implementation_exists_yet` → `production_platform_implementations_exist_only_where_allowed` | Capture contracts allowed only in `tp-capture/src/wgc/` and `unsupported.rs` | Intended by the BOOT-02 handoff |
| `workspace_boundaries` `the_app_build_graph_has_no_test_support` | Also asserts a single `windows` version | New dependency |
| `tp-desktop/tests/config` `the_only_capability_grants_only_get_app_info_to_the_main_window` → `…_only_the_app_commands_…` | Exact list now has 6 permissions | Five preview commands |
| `tp-desktop/tests/ipc` `unknown_commands_are_rejected` | 7 more capture-shaped names | T-CAP-SEC-07 |
| `tp-contracts/tests/contracts_implementable` | Amended contract (`Target`, `CapturePicker`) | ADR-007 |
| Vitest `App.test.tsx` (6 of 7 tests) | Core-status queries scoped to the status card; preview calls routed to fixtures | The preview card adds a second "Ready" text and two IPC calls on mount |
| Vitest `appInfo.contract.test.ts` | Binding file count 11 → 21 | New DTOs |

## 3. Results by prompt ID

### Provider (T-CAP-WIN)

| ID | Result | Observed | Evidence |
|---|---|---|---|
| T-CAP-WIN-01 WGC support detection | **PASS** | `IsSupported` = true; capabilities `Available`, `system_picker`, `gpu_frames`, max 60 fps | `capture_windows::support_detection_matches_capabilities` |
| T-CAP-WIN-02 Enumerate three monitors | **PASS** | DISPLAY1, DISPLAY2, DISPLAY3 | `dev_w1_has_three_1080p_monitors_with_negative_origins`; `display_enumeration_is_consistent` |
| T-CAP-WIN-03 Negative-coordinate geometry | **PASS** | (−1920, 71), (−3840, 71), (0, 0) primary; no overlaps | same |
| T-CAP-WIN-04 Correct resolution | **PASS** | 1920×1080 each; frames from each monitor are 1920×1080 | `each_monitor_delivers_frames_of_its_own_size` |
| T-CAP-WIN-05 Provider initializes | **PASS** | Device on RTX 3090, free-threaded pool, session started | all real-device tests |
| T-CAP-WIN-06 First valid native frame | **PASS** | Window 126–139 ms; monitors 114–138 ms; pattern pixels `[255,0,255]` / `[32,32,32]` read back from the frame | `window_capture_delivers_real_test_pattern_frames` |
| T-CAP-WIN-07 Correct metadata | **PASS** | `seq` contiguous from 1; `generation` constant per run; OS timestamps non-decreasing; BGRA8; content size; target matched to the right display via WGC names (validated on all 3) | same; `each_monitor_…` |
| T-CAP-WIN-08 Target resized | **PASS** | 626×473 → 886×593 (provider); 786×593 → 986×693 (host); `TargetResized`, then new frame size | `resizing_the_target_resizes_frames`; `the_host_follows_a_resized_target` |
| T-CAP-WIN-09 Target closed | **PASS** | `TargetClosed` via watchdog (WGC `Closed` not raised, finding F-CAP-2); no frames afterwards; host → `Stopped`/`targetClosed` | `closing_the_target_ends_the_capture`; `the_host_ends_the_run_when_the_target_closes` |
| T-CAP-WIN-10 Device/resource failure | **PASS (resource)**; device loss **PASS (engine level)**, real **UNVERIFIED** | A closed target fails with `PlatformUnavailable` and no frames; device loss and readback failure → `Failed` + recovery text (test double). A real device removal cannot be induced without privileged tools or driver changes | `a_target_that_is_already_gone_fails_cleanly`; `tp-core preview::device_loss_and_readback_failures_end_in_failed` |

### Authorization and IPC (T-CAP-SEC)

| ID | Result | Observed | Evidence |
|---|---|---|---|
| T-CAP-SEC-01 No capture without local selection | **PASS** (engine + IPC) | `start` only opens the picker; 0 provider starts and no grant until `Selected`; at IPC level a fabricated owner handle → `forbidden`, no picker | `no_capture_starts_without_a_picker_selection`; `start_without_a_real_owner_window_opens_no_picker` |
| T-CAP-SEC-02 Picker cancellation → no capture | **PASS** (engine) and **PASS (app log)** | Engine test; on the device, generation 1 in session 1: `start_requested` → `picker_cancelled`, no capture thread ever existed | `cancelling_the_picker_captures_nothing`; `app/session1-functional.NOTES.md` |
| T-CAP-SEC-03 Grant cannot authorize remote viewing | **PASS** | `ViewScreen` denied with a preview active, even with a platform claiming everything; no conversion compiles | `a_local_preview_grant_authorizes_neither_remote_viewing_nor_input`; doctests |
| T-CAP-SEC-04 Grant cannot authorize remote input | **PASS** | `ControlInput` denied; `InputGate` injected nothing | same |
| T-CAP-SEC-05 Stop revokes | **PASS** (engine + host level) | Grant gone; slot empty; late frame ignored; on real frames: empty header after stop | `stop_revokes_preview_access`; `the_host_serves_real_pattern_frames_and_stop_revokes_them` |
| T-CAP-SEC-06 Stale requests cannot restart | **PASS** | Selection after stop and selection for an older request: no start, target released, rejection counted | `stale_selections_cannot_restart_capture`; exploration I2/I4 |
| T-CAP-SEC-07 Unknown IPC command rejected | **PASS** | 13 unknown or capture-shaped names rejected | `unknown_commands_are_rejected` |
| T-CAP-SEC-08 Unauthorized window/origin rejected | **PASS** (mock runtime, real ACL) | All 5 preview commands denied to a second window and to 3 remote origins. Live-WebView ACL: **UNVERIFIED** | `preview_commands_are_denied_to_other_windows_and_remote_origins` |
| T-CAP-SEC-09 No frame contents in logs | **PASS** | No label, title or pixel data in logs (synthetic private title; real test-pattern title); no non-allowlisted key | `logs_contain_no_target_labels_or_pixels`; host real-device test |
| T-CAP-SEC-10 No unauthorized network listener | **PASS** | 0 listening TCP sockets, 0 UDP endpoints (smoke runs; 4-minute idle session). Outbound HTTPS by WebView2 runtime processes recorded as F-CAP-8 | `windows/release-run1.summary.json`, `app/idle-network-check.*` |

### Runtime (T-CAP-RUN)

| ID | App level (person + picker) | Host level (real frames, stand-in picker) |
|---|---|---|
| T-CAP-RUN-01 Native picker appears | **PASS (corroborated once)**: generation 1 opened the picker (`start_requested`, `GraphicsCapture.dll` loaded) and the operator cancelled it | — |
| T-CAP-RUN-02 User selects a real display | **NOT RUN**: operator reports not corroborated (§5) | Monitor capture by the provider: PASS (3 displays) |
| T-CAP-RUN-03 Live preview updates | **NOT RUN** | **PASS**: frames with pattern pixels reach the slot; 23 fps served |
| T-CAP-RUN-04 Capture indicator active | **NOT RUN** (operator observation not corroborated); static check PASS (border never disabled) | — |
| T-CAP-RUN-05 Stop Capture works | **NOT RUN** at app level | **PASS** (stop 1–16 ms, revoked) |
| T-CAP-RUN-06 App closes during capture safely | **NOT RUN** while capturing (no capture ever ran in the app). Clean `WM_CLOSE` exit verified while idle (4 runs) | Stream drop/stop on teardown TESTED |
| T-CAP-RUN-07 Restart after shutdown | **PASS** (idle): second launch reaches IPC in 0.45 s, Ready | — |
| T-CAP-RUN-08 Ten start/stop cycles | **NOT RUN** | **PASS**: 10/10 host cycles; provider 10/10, 30/30, 100/100 |
| T-CAP-RUN-09 Thirty-minute stability | **NOT RUN** | **PASS (soak run 2)**: 1,800 s with frames throughout (96,837 frames, median 54 fps), 0 failures, memory flat, CPU 0.45%, GPU 3D 0.9%. Soak run 1: no crash, but frames only for the first 745 s (performance §5) |
| T-CAP-RUN-10 No leftover resources/processes | **PASS** for processes (0 survivors in every app run; nothing force-stopped). **PARTIAL** for resources: about 3 handles per capture run stay in the process until exit (F-CAP-4) | `perf/diagnostic-handles-per-step.log` |

### Performance (CAP-PERF)

See [CAP-02A-performance](CAP-02A-performance.md):
- **PASS (recorded):** 01, 02, 04, 05, 06, 07 (with the handle caveat), 08 (soak run 2) and 10.
- **03:** host level only; displayed preview FPS NOT RUN.
- **09:** NOT RUN.

## 4. Real-device runs

| Run | Result | Evidence |
|---|---|---|
| `capture_windows` run 1 | 5/7; 2 failures were real findings (monitor naming, Closed event), later fixed | `provider/real-device-run1.log` |
| Diagnostic: names and close behaviour | GDI ≠ WGC numbering; `Closed` not raised; minimized 146×28, closed 0×0 | `provider/diagnostic-names-and-close.log` |
| `capture_windows` runs 2, 3, final | 7/7 each | `provider/real-device-run2.log`, `…run3.log`, `final-capture_windows.log` |
| `preview_windows` host run 1, final | 4/4 each | `provider/host-real-device-run1.log`, `final-preview_windows.log` |
| Benches (see performance) | Valid runs used; invalid runs kept and listed | `perf/` |

## 5. App-level manual session (T-DEV-21)

Details: `evidence/CAP-02A/app/session1-functional.NOTES.md`.
- The app recorded one Start and one cancelled picker, and nothing else.
- The operator reported a live preview four times, but the app received no further start request.
- Independent checks agreed with the app: thread names, CPU/GPU, loaded modules and unchanged screenshots.
- These app-level items are therefore **NOT RUN**, not PASS.
- The session itself ended cleanly: `WM_CLOSE`, 0 survivors.

## 6. Negative controls

**11/11 caught**; every file was restored byte-for-byte (SHA-256); `Cargo.lock` unchanged; all guards pass after the run (`evidence/CAP-02A/negative-controls/negative-controls.log`).

| ID | Violation | Guard that failed |
|---|---|---|
| NC-CAP-1 | `SetIsBorderRequired(false)` | `the_capture_border_is_never_disabled` |
| NC-CAP-2 | Second `CreateCaptureSession` | `capture_sessions_are_created_in_one_place` |
| NC-CAP-3 | `WgcTarget::programmatic` outside testing.rs | `programmatic_capture_targets_are_test_only` |
| NC-CAP-4 | Second grant mint site | `only_the_picker_completion_mints_a_preview_grant` |
| NC-CAP-5 | `unsafe` in `scale.rs` | `unsafe_code_is_confined_and_justified` |
| NC-CAP-6 | `std::fs::write` in the preview host | `the_capture_and_preview_path_writes_no_files` |
| NC-CAP-7 | `FrameReadback` implemented in tp-core | `production_platform_implementations_exist_only_where_allowed` |
| NC-CAP-8 | `test-support` on tp-core's tp-capture dependency | `the_app_build_graph_has_no_test_support` |
| NC-CAP-9 | `LocalPreviewGrant` derives `Clone` | compile-fail doctest "copy" (line 18) |
| NC-CAP-10 | `From<LocalPreviewGrant> for ConsentGrant` | compile-fail doctest "widen" (line 13) |
| NC-CAP-11 / 11b | `selected()` ignores generation and state | `stale_selections_cannot_restart_capture`; exploration invariant I2 (independently) |

## 7. Windows runtime regression (BOOT-01 WIN-BOOT checks, CAP-02A build)

| Check | Result | Evidence |
|---|---|---|
| Window visible, IPC `get_app_info` | PASS (0.66 s; restart 0.45 s) | `windows/release-run1.summary.json`, `release-restart.summary.json` |
| Token | PASS: not elevated, **Medium** integrity | same |
| DPI | PASS: PerMonitorV2, 96 DPI | same |
| Listeners | PASS: 0 TCP listening, 0 UDP | same |
| Resize at 4 sizes incl. the negative-coordinate monitor | PASS | `release-run1.resize-*.png` |
| Clean exit via WM_CLOSE | PASS: exit 0, 0 survivors | same |
| UI shows the local preview card in Ready, CAP-02A badge | PASS | `release-run1.initial.png` |
| Debug fault injection (WIN-BOOT-06/07) | NOT RE-RUN in CAP-02A (code path unchanged; covered by `tp-desktop` tests in debug and release) | — |

## 8. Defects found during the phase

| ID | Defect | Found by | Fix |
|---|---|---|---|
| D-1 | Monitor classification matched GDI names, but WGC numbers monitors differently | Real-device test | Match against each monitor's own WGC item name; validated on all 3 |
| D-2 | `GraphicsCaptureItem.Closed` not raised for a destroyed window | Real-device test | Liveness watchdog (250 ms) |
| D-3 | Two `unsafe extern` callbacks without a SAFETY contract | New boundary test | Contracts added (F-CAP-1) |
| D-4 | Every start created 3 extra monitor items (one leaked handle each) | Handle diagnostic | Names cached per layout |
| D-5 | Preview-stats doc claimed OS timestamps for the UI's capture FPS | Review | Doc corrected (host monotonic clock) |
| D-6 | First bench results invalid (source stalled) | Repaint counter | Topmost source; stalled runs excluded and documented |
| D-7 | Bench wrapper recorded no exit code | Review | Process handle read at start |
| T-1 … T-4 | Test defects: exploration helper, card-test backend, App tests' ambiguous "Ready", two no-op waits | Test runs, review | Corrected |

## 9. Not run or blocked

- App-level picker flow with a person: T-CAP-RUN-02 … 06 (while capturing), 08 and 09, and T-CAP-RUN-04 observation.
- Displayed preview FPS (CAP-PERF-03 in the WebView).
- Display resolution and scale changes (CAP-PERF-09, T-DEV-02).
- UAC and lock screen (T-DEV-03).
- Real device loss.
- Live-WebView ACL.
- DXGI comparison (U-11).
- All macOS items (CAP-02B).

## 10. CAP-02A-R2 reconciliation

Results of the R2 acceptance attempt (2026-10-28). Evidence: `docs/qa/evidence/CAP-02A-R2/`.

### 10.1 Gates and regression (R2 build)

| Check | Result | Evidence |
|---|---|---|
| `npm run check` (before the sessions; final) | 9/9 PASS both times; Rust 189 passed / 0 failed / 11 ignored; Vitest 31 passed | `CAP-02A-R2/gates/npm-check-pre-session.log`, `npm-check-final.log` |
| Native release build | PASS: `tp-desktop.exe` 8,960,512 bytes, SHA-256 `FFE0F665DCE284586B06B54C858299367785974D217D9DF6A32EFE279CD3C646` | `CAP-02A-R2/build/` |
| Negative controls (all 11, final code) | 11/11 caught; all restored; `Cargo.lock` unchanged; guards pass after | `CAP-02A-R2/negative-controls/negative-controls.log` |
| Windows runtime smoke (R2 build) | PASS: IPC 0.44 s, Medium integrity, PerMonitorV2, 0 TCP listening / 0 UDP / 0 TCP connections, 4 resizes, exit 0, 0 survivors | `CAP-02A-R2/windows/r2-release-run1.summary.json` |
| Bindings drift | PASS (part of `npm run check`) | same |

**Not repeated:** the 30-minute host soak and the 100-cycle provider run (CAP-02A evidence is sufficient; nothing in R2 touched the capture or host code).

### 10.2 App-level results (T-DEV-21)

| Step | Result | Evidence |
|---|---|---|
| Only the intended instance runs; window identifiable | PASS: title `TP Remote - QA <label> - PID <pid>`; one `tp-desktop` per session | `r2-session*.summary.json` |
| App visible, not minimized | **Not maintained:** covered by the operator's editor windows, and minimized repeatedly from outside the app | `r2-session1.samples.csv`, `r2-session3.window-events.log` |
| Click Start → IPC → Rust → picker | **PASS** (diagnostic click, with permission): received 271 ms after the click; picker "Capture with TP Remote - QA r2-session3 - PID 16388" shown over TP Remote | `r2-session3.events.log`, `.window-events.log` |
| A person selects the test pattern | **NOT RUN / blocked**: the picker closed without a selection (`picker_cancelled`) | same |
| Capturing state, moving pixels in the canvas, Preview FPS > 0, counters increasing, capture indicator, Stop, release | **NOT RUN** (no selection) | — |
| Cancellation produces no capture | **PASS** (app level, twice) | `session1-functional`, `r2-session3` |
| Clean exit; restart after exit | **PASS** while idle (5 app launches across CAP-02A and R2, all `WM_CLOSE` → exit, 0 survivors). Exit while capturing: **NOT RUN** | summaries |
| Ten app-level cycles | **NOT RUN** | — |
| No delivery after stop | Provider/host level only (CAP-02A) | — |

**Root cause of the CAP-02A mismatch:** identified at the level of "operator input did not reach TP Remote". Details are in the [report §21.2](../reports/CAP-02A-report.md#212-root-cause-of-the-operatorapp-mismatch-identified-operator-side-interaction-app-path-verified). Seven operator reports of success were not corroborated by the app.

### 10.3 Status changes from R2

- T-CAP-RUN-01 (picker appears): now also proven for the correct native window (the picker title names the QA window).
- T-CAP-SEC-02 (cancel): app-level PASS confirmed a second time.
- T-CAP-RUN-07 (restart): PASS (idle), 4 more relaunches.
- All other T-CAP-RUN items stay as in §3.
- New UI evidence surface: the card shows **Frames drawn** (frames painted in this window). Vitest asserts it becomes non-zero once frames are drawn.
