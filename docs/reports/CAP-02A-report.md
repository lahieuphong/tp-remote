# CAP-02A report — Windows native screen capture and local preview

| Field | Value |
|---|---|
| Phase | CAP-02A |
| Date | 2026-10-28 |
| Host | DEV-W1, Windows 11 Enterprise LTSC 10.0.26100 |
| Verdict | **PARTIAL** (§19); unchanged after CAP-02A-R2 (§21) |
| Next phases | CAP-02B (macOS) and NET-03: **not started**; waiting for reviewer approval |

Status words used throughout:
- **IMPLEMENTED** · **TESTED** (automated) · **VERIFIED** (observed on DEV-W1)
- **host level**: the production preview host driven over real frames with a test-only picker stand-in
- **NOT RUN** · **UNVERIFIED** · **FUTURE**

## 1. Executive summary

**Achieved:**
- TP Remote has a real Windows.Graphics.Capture provider, verified on the device:
  - real frames of a synthetic test pattern, with exact colours read back;
  - all three monitors, including negative origins, each mapped to the right display;
  - resize, closed-target handling, 100/100 start/stop cycles, and nothing delivered after stop.
- A local self-preview is in the app:
  - the IPC request only opens the Windows picker, and a separate `LocalPreviewGrant` authorizes local frames only;
  - remote authorization is unchanged (`platform_capabilities()` = NONE).
- All gates pass (9/9; Rust 189 + 11 real-device; Vitest 31), every new security test passes, and 11/11 negative controls catch their violation.
- A 30-minute soak of the production preview host over real frames ran without failure, with flat memory (CPU 0.45%, GPU 0.9%).

**Not verified:** the complete app-level flow with a person selecting a target in the picker and seeing the live preview in the WebView.
- The operator reported success four times.
- The app received exactly one picker request, which was cancelled, and never captured. Its log, thread list, CPU/GPU and screenshots all show this.
- The same pipeline below the picker was therefore verified at host level instead, on real frames.

**Findings:**
- WGC delivered no frames for windows that were covered or (probably) minimized, and intermittently for windows on the secondary displays.
- `GraphicsCaptureItem.Closed` was not raised. Mitigated with a watchdog.
- Each capture run leaves about 3 handles.
- WebView2's own processes make outbound HTTPS connections. There is no listener.

## 2. Baseline state and Git status

**Baseline** (before any CAP-02A change, `evidence/CAP-02A/baseline/`):
- `git status --short` empty; branch `main`; HEAD `f42b230` "BOOT-02: core contracts, session lifecycle and security primitives"; `main` level with `origin/main`; no stash.
- BOOT-02 had been committed and pushed outside the assistant's actions.
- `npm run check`:
  - The first attempt failed at the first gate because this shell's PATH lacked `%USERPROFILE%\.cargo\bin` (environment only; log kept as `npm-check-baseline-attempt1-no-cargo-on-path.log`).
  - With cargo on the PATH: **9/9 PASS, 141 Rust and 18 Vitest tests**.

**During the phase:**
- At 16:22:14 a commit `7e1a661` "CAP-02A: Windows capture pipeline and local preview" was created by `lahieuphong` and pushed (`main` level with `origin/main`).
- **The assistant made no commit, branch, stash, reset or push.** No git write command was run, and the BOOT-02 size baseline used `git archive`.
- `7e1a661` captured the work in progress at that moment. Everything after it is uncommitted on top of it: 33 paths at the end of the phase, including the QA documents, this report, the soak and negative-control evidence, and later fixes (name cache, minimized-state instrumentation, script fixes).

## 3. Capture API and dependency choice

| Option | Assessment | Decision |
|---|---|---|
| A/B: Microsoft `windows` (windows-rs) with direct WGC + Direct3D 11 | Official bindings; **0.62.2 already in `Cargo.lock` through Tauri**, so no new package; MIT OR Apache-2.0; MSRV 1.82 (toolchain 1.99.1); `unsafe` stays in our code, where we can confine and audit it | **Chosen** (approved before installation) |
| C: `wgc` 3.1.0 | MIT OR Apache-2.0, ≈ 2.3k downloads, released 2026-10-25; adds a wrapper layer | Rejected |
| `windows-capture` 2.0.1 | MIT, popular; includes encoding and file-saving features the app must not ship | Rejected |

- **Lockfile impact:** dependency-list lines only.
- **Binary impact:** +244,736 bytes (+2.8%). The BOOT-02 commit, rebuilt here, gives 8,697,344 bytes; CAP-02A gives 8,942,080 (`evidence/CAP-02A/build/size-baseline.txt`).
- **OS API requirements:**
  - `GraphicsCaptureSession` / `GraphicsCapturePicker`: Windows 10 1803+;
  - `CreateForMonitor`/`CreateForWindow` interop: 1903+;
  - `MinUpdateInterval`: 24H2;
  - free-threaded pool: needs no dispatcher queue.
  - Verified only on build 26100.
- **Async:** `windows-future` types are not re-exported by `windows` 0.62. Instead of adding a direct dependency, `IAsyncOperation::when` (reachable through the type) is used.

## 4. Security model for Local Self-Preview (ADR-007)

**The contradiction:**
- BOOT-02's only approval proof is `LocalConsent` for an *authenticated remote session*. It has no production constructor by design.
- A local preview cannot use it without a fake constructor or a fake session, both of which are forbidden.

**Resolution:** a separate, minimal local-only model, recorded in [ADR-007](../architecture/ADR-007-local-preview-authorization.md).
- **Picker boundary:** the Windows capture picker is the trusted selection boundary.
  - `start_local_preview` only opens the picker, and only over a live window of this process (`IsWindow` + process id).
  - No IPC argument can name a target. Targets are opaque OS objects produced, in production, only by the picker.
- **Grant:** `LocalPreviewGrant`.
  - Minted in exactly one place, on picker selection for the current request generation.
  - It authorizes local preview frames of that generation only.
  - It is not `Clone`, not serializable, not forgeable, and not convertible into `ConsentGrant` (compile-fail doctests).
  - It is revoked on stop, and the buffer is cleared.
- **Lifecycle:**
  - Cancel captures nothing; a stale or late selection is released uncaptured; one request at a time.
  - It has its own 8-state machine, separate from the remote session machine.
- **Unchanged:** remote `ViewScreen` and `ControlInput` still need an authenticated peer plus host consent, and stay denied while a preview runs (tested).
- **Test-only bypasses:** programmatic targets and the picker stand-in exist only behind `tp-capture/test-support`. Boundary tests and the `cargo tree` guard keep them out of the app.

## 5. Files created and modified

95 files relative to BOOT-02, excluding evidence. Full list: `git diff --name-status f42b230`, plus the untracked QA docs and this report.

| Area | Created | Modified |
|---|---|---|
| `tp-capture` | `scale.rs`, `unsupported.rs`, `wgc/{mod,d3d,monitors,picker,stream,target,testing}.rs` | `Cargo.toml`, `lib.rs` |
| `tp-contracts` | `tests/preview_fixtures.rs` | `capture.rs` (amended contract), `ipc.rs` (preview DTOs, frame header, 3 error codes), `tests/contracts_implementable.rs` |
| `tp-core` | `preview/{mod,machine,host}.rs`, `tests/preview.rs`, `tests/preview_machine.rs` | `lib.rs` (phase, `NativePreview`) |
| `tp-diagnostics` | — | `lib.rs` (12 allowlisted keys) |
| `tp-desktop` | 5 generated permission files | `src/lib.rs` (5 commands, exit handling), `build.rs`, `capabilities/main-window.json`, `tests/{ipc,config}.rs` |
| `tp-test-harness` | `src/capture.rs`, `src/bin/{capture-bench,preview-soak,tp-test-pattern}.rs`, `tests/{capture_windows,preview_windows}.rs` | `Cargo.toml`, `lib.rs`, `bindings.rs`, `tests/workspace_boundaries.rs` |
| UI | `core-api/{localPreview,parseLocalPreview,validate}.ts`, `app/useLocalPreview.ts`, `components/LocalPreviewCard{,.test}.tsx`, `contract/localPreview.contract.test.ts`, `test/previewFixtures.ts`, 10 generated bindings | `App.tsx`, `App.test.tsx`, `RemoteFeaturesCard.tsx`, `core-api/{errors,parseAppInfo}.ts`, `contract/appInfo.contract.test.ts`, `bindings/{index,IpcErrorCode}.ts` |
| Fixtures | `tests/fixtures/ipc/preview/**` (4 valid, 6 invalid) | — |
| Scripts | `capture-qa-windows.ps1`, `capture-bench-windows.ps1` | `smoke-windows.ps1` (all-state TCP listing) |
| Workspace | — | `Cargo.toml` (`windows` workspace dependency), `Cargo.lock` |
| Docs | ADR-007, CAP-02A capture pipeline, security review, test results, performance, this report | ARCH-00, TEST-MATRIX, TEST-STRATEGY, THREAT-MODEL, PROMPT-PLAN, BOOT-01 dependency map, BOOT-02 contract map, README |

## 6. CaptureProvider implementation

`tp_capture::wgc::WgcProvider` implements `CaptureProvider` + `CapturePicker`:
- **Pipeline:** picker → `GraphicsCaptureItem` → per-run D3D11 device (BGRA, multithread-protected) → `Direct3D11CaptureFramePool::CreateFreeThreaded` (BGRA8, 2 buffers) → `GraphicsCaptureSession`.
- **Frame metadata:** OS QPC timestamp (`SystemRelativeTime`), contiguous `seq`, run `generation`, content size, format.
- **Latest wins:** queued frames are drained (counted as dropped/superseded).
- **Resize:** `Recreate` and `TargetResized`.
- **Target closed:** the `Closed` event, plus a 250 ms liveness watchdog because `Closed` was not raised on DEV-W1.
- **Device loss:** detection → `DeviceLost`.
- **Panics:** `catch_unwind` on every callback.
- **Delivery and stop:** delivery holds one lock, so `stop` (bounded, idempotent, also run on drop) guarantees nothing arrives afterwards.
- **Readback:** a CPU readback through a reused staging texture, area-averaged to ≤ 1280×720 RGBA.
- **Never:** frames are not written to disk; the border property is never touched.

Status: IMPLEMENTED, TESTED, VERIFIED. Real device loss is UNVERIFIED.

## 7. Windows monitor enumeration

`EnumDisplayMonitors` + `GetMonitorInfoW` + `GetDpiForMonitor` + `EnumDisplaySettingsW` (the process is PerMonitorV2):

| Index | Device | Origin | Size | Scale | Hz | Primary | WGC name |
|---|---|---|---|---|---|---|---|
| 0 | DISPLAY1 | (−1920, 71) | 1920×1080 | 100% | 60 | no | Display 2 |
| 1 | DISPLAY2 | (−3840, 71) | 1920×1080 | 100% | 60 | no | Display 1 |
| 2 | DISPLAY3 | (0, 0) | 1920×1080 | 100% | 60 | yes | Display 3 |

- **Ordering:** sorted by device name. Ids are valid within a layout epoch only; the epoch increments on change.
- **Numbering:** GDI order, WGC numbering and physical order all differ. A picked display is classified only when its WGC name and size match exactly one monitor; this was validated on all three.
- **Targeting:** automatic monitor targeting is **not** offered in the app.

## 8. Live preview implementation

- **Commands:** five Tauri commands (all synchronous, on the main thread as the picker requires):
  - `get_capture_displays`
  - `get_local_preview_status`
  - `start_local_preview`
  - `stop_local_preview`
  - `get_local_preview_frame`
- **Frame transport:** the frame command returns a binary IPC response: a 24-byte header plus RGBA bytes, with a shared Rust/TS fixture. It goes through Tauri's protocol, with no socket.
- **UI:**
  - The WebView pulls the next frame only after drawing the previous one, so at most one frame is in flight, and draws it to a canvas.
  - It shows every required state, Start and Stop, a picker hint, target information, resolution, **capture FPS (frames from Windows)** and **preview FPS (frames drawn here)** as separate figures, skipped/dropped counts, elapsed time, first-frame time, errors with recovery steps, and the display list.
  - When capture ends, the canvas is removed and "No capture is active." is shown.
  - The mirror effect is explained for display targets.
- **Status:** IMPLEMENTED and TESTED (Vitest with mocked IPC; IPC through the real ACL with the mock runtime). In the real app, the release UI renders the card (screenshot). **A live preview in the real WebView was not observed (NOT RUN).**

## 9. Real-device validation

| Check | Result |
|---|---|
| Provider suite (`capture_windows`, 7 tests) | 7/7 (final run); run 1 found D-1 and D-2 |
| Host suite (`preview_windows`, 4 tests) | 4/4: real pattern pixels in the preview slot, stop revokes, resize, close, 10 cycles |
| Monitor capture (3 displays, metadata only) | Each delivers 1920×1080 frames in 114–138 ms; mapped correctly |
| Picker over the app window (person) | Opened once and cancelled, captured nothing (corroborated). Selection and live preview: **NOT RUN** (§1) |
| Capture border | Static guarantee TESTED; app-level observation NOT RUN |
| Runtime regression (release, smoke) | IPC, Medium integrity, PerMonitorV2, resize at 4 sizes, clean exit, restart: PASS |

## 10. FPS, CPU, RAM and frame-drop measurements

Full report: [CAP-02A-performance](../qa/CAP-02A-performance.md).

| Metric | Result |
|---|---|
| First frame | Window 143.5–287.8 ms (140 starts); monitor 114–138 ms; through the host 182–382 ms |
| Native capture FPS (720p window, 60 s) | **55.6 mean**; 1-s windows 51–60; none below 29 |
| 1080p → 720p path | Capture median 50.2 fps (live part); readback 9.4 ms mean |
| Preview FPS | Host-served ≈ 22.5 fps (30/s cap). **Displayed FPS in the WebView: NOT RUN** |
| Drops | Provider dropped 0; superseded 0; nothing after stop; preview skipped 3.1% (UI busy) |
| CPU (process) | 0.84% of the machine (≈ 13% of one logical processor) at 720p with 30/s readback |
| Memory | 35–53 MB working set; flat over 30 min |
| GPU | GPU 3D engine of the process 0.90% mean, 1.23% max (soak run 2, 800×450 window, 30/s readback) |

## 11. Ten-cycle stability results

| Level | Cycles | Result |
|---|---|---|
| Provider bench (DISPLAY3/DISPLAY1, topmost) | 10, 30, 100 | **10/10, 30/30, 100/100** (DISPLAY3); first frame ≤ 288 ms; stop ≤ 17 ms; 0 deliveries after stop |
| Preview host (real frames) | 10 | **10/10**; first preview 182–382 ms; stop ≤ 16 ms; grant revoked each time |
| App with a person | 10 | **NOT RUN** |

Resources over cycles:
- Threads return to baseline.
- **About 3 handles per run are not returned** (1 per capture item, 2 per free-threaded frame pool; isolated step by step). They are freed at process exit. Root cause UNVERIFIED (apparently OS-side), F-CAP-4.

## 12. Thirty-minute stability results

| Run | Result |
|---|---|
| Soak run 1 (host level, 1,800 s) | No crash, no failure, 0 invalid frames, memory flat (private 54–58 MB), CPU 0.44% while live, stop 5.2 ms, no frame or grant after stop. **Frames flowed for 745 s only**; after that the source delivered nothing (OS behaviour, §17) |
| Soak run 2 (host level, 1,800 s, minimized-state logging + auto-restore, GPU) | **PASS**: frames for the whole 30 min (96,837; 5-s rate min 38 / median 54 / max 60 fps), preview served at a median 22.8 fps, 0 failures and 0 invalid frames, window never minimized, CPU 0.45%, GPU 3D 0.9%, private memory flat (55.6–56.5 MB quartile means), handles 440 → 430, stop 3.2 ms, nothing after stop |
| App with a person | **NOT RUN** |

## 13. Security tests

**T-CAP-SEC-01 … 10: all PASS.**

| Level | IDs |
|---|---|
| Engine | 01–06, 09 |
| IPC with the real ACL (mock runtime) | 01, 07, 08 |
| Device | 02 (cancel), 05, 09 (host level), 10 (0 listeners, 0 UDP) |

Static guards (T-STA-11) pass, and the negative controls caught 11/11.

Not verified:
- live-WebView ACL behaviour (carried from BOOT-01);
- app-level observation of the yellow border.

Details: [CAP-02A-security-review](../security/CAP-02A-security-review.md).

## 14. Regression test counts

| Suite | BOOT-02 | CAP-02A |
|---|---|---|
| Rust routine | 141 | **189 passed, 0 failed** (+48) |
| Rust real-device (`--ignored`) | — | **11 passed** |
| Vitest | 18 | **31 passed** (+13) |

- **BOOT-01/02 tests:** all still pass. Changed deliberately, with reasons: 2 in tp-core, 2 boundary tests, 2 shell tests, the contract test, and the App and binding-count tests in Vitest ([test results §2](../qa/CAP-02A-test-results.md)).
- **Session state machine, authorization, redaction, protocol and bindings suites:** unchanged and passing.

## 15. Build, typecheck and lint results

| Check | Result |
|---|---|
| fmt, check, clippy `-D warnings`, test, bindings drift, `tsc`, ESLint, Vitest, Vite build | **9/9 PASS** (`gates/npm-check-final.log`) |
| `npm run build:app` (release, no bundle) | PASS |
| Harness release binaries | PASS |
| License inventory | No new package, so no change to the inventory; `windows` is MIT OR Apache-2.0 |
| Forbidden capabilities | None: no plugins; no core, fs, shell or http permissions; CSP unchanged |

## 16. Dependency and license findings

- **New direct dependency:** `windows` 0.62.2 (approved), MIT OR Apache-2.0, 14 features. The approved `Win32_System_Performance` turned out to be unnecessary and is not enabled.
- **Transitive dependencies:** `windows-core` 0.62.2, `windows-future` 0.3.2, `windows-collections` 0.3.2 and `windows-numerics` 0.3.1 were already locked.
- **npm:** no change.
- **App graph:** one `windows` version, and no `test-support` (tested).
- **Unchanged open items:**
  - `option-ext` (MPL-2.0) linked through Tauri;
  - legal owner unconfirmed;
  - `cargo-audit` and `cargo-deny` not installed.

## 17. Remaining technical risks

| Risk | Impact | Next step |
|---|---|---|
| App-level picker flow not verified with a person (F-CAP-7) | The trusted path is unproven end to end in the real WebView | Reviewer or operator runs `scripts/capture-qa-windows.ps1`; pass only when the app's log corroborates it |
| Covered / (probably) minimized windows and secondary-display windows stop delivering frames; new sessions get none (F-CAP-3) | A window preview can freeze; tests need a visible source | Investigate DWM behaviour (W2, other drivers); consider DirectX-presenting test windows; document the UX |
| ≈ 3 handles per capture run not returned (F-CAP-4) | Slow growth with many previews | Re-measure on other builds and drivers; report upstream if confirmed |
| WebView2 outbound HTTPS connections (F-CAP-8) | Privacy expectation; not TP Remote traffic | Evaluate WebView2 browser arguments (R-8) |
| WebView compromise could open the picker repeatedly or read preview pixels (R-1, R-2) | Local only; a user selection is still required | Isolation pattern and hardening (TM-16) |
| Preview served below its 30/s cap (≈ 23/s) | Preview smoothness | Deadline-based throttle; GPU scaler in VID-04 |
| Real device loss, UAC and lock screen, resolution changes, HDR, Intel adapter, DXGI (U-11) not tested | Unknown behaviour | Need approval to change display settings; T-DEV-02/03; FS-B5 |
| Live-WebView ACL, `dynamic-acl` default | Carried | REL-09 |
| No Mac | CAP-02B blocked | Provide DEV-M1 |

## 18. Commands executed

| Command | Outcome |
|---|---|
| `git status --short`, `git branch --show-current`, `git log -1 --oneline`, `git stash list` | Clean, `main`, `f42b230`, no stash |
| `npm run check` (baseline) | Attempt 1 FAIL (cargo not on PATH); attempt 2 9/9 PASS, 141 + 18 tests |
| `cargo check -p tp-capture --features test-support` | Errors fixed iteratively (async types, clippy) |
| `cargo test -p tp-test-harness --test capture_windows -- --ignored --test-threads=1 --nocapture` | Run 1 5/7 (findings D-1, D-2); runs 2, 3, final 7/7 |
| `cargo test -p tp-test-harness --test preview_windows -- --ignored --test-threads=1 --nocapture` | 4/4 (twice) |
| `cargo test -p tp-core --test preview / --test preview_machine / --doc` | 15/15, 2/2 (62 configurations, 1,421 edges), 3/3 compile-fail |
| `cargo test -p tp-desktop` | 13 IPC + 4 config (one config test updated deliberately) |
| `npm run bindings:generate`, `npm run bindings:check` | 21 files; up to date |
| `npm run -s typecheck / lint / test` | PASS after fixes (exhaustive error codes, lint findings) |
| `powershell scripts\capture-bench-windows.ps1 …` (15 `capture-bench` runs, 2 `preview-soak` runs) | See the performance report; invalid runs kept |
| `powershell scripts\capture-qa-windows.ps1 -Label session1-functional` | Manual session: one start and one cancel corroborated; clean exit |
| `powershell scripts\capture-qa-windows.ps1 -Label idle-network-check -NoPattern` | 0 listeners; WebView2 outbound connections recorded |
| `powershell scripts\smoke-windows.ps1 -Exe target\release\tp-desktop.exe -Label release-run1 -Resize` (and `release-restart`) | All checks PASS |
| `powershell docs\qa\evidence\CAP-02A\negative-controls\run-negative-controls.ps1` | 11/11 caught, all restored, `Cargo.lock` unchanged |
| `npm run build:app`; `cargo build --release -p tp-test-harness --bins` | PASS |
| `git archive f42b230` + `npm run build:app` (scratchpad) | BOOT-02 size baseline |
| `npm run check` (final) | **9/9 PASS**; Rust 189 passed, 11 ignored; Vitest 31 |

Read-only diagnostics:
- thread descriptions, loaded modules, `GetLastInputInfo`, `IsIconic` / `GetWindowRect`;
- `powercfg /query`, the screen-saver registry keys, WMI monitor state;
- `Get-NetTCPConnection` / `Get-NetUDPEndpoint`.

Nothing was installed. No firewall, registry, display, driver or Defender setting was changed. No unrelated process was stopped; only processes started by the phase's scripts were closed (`WM_CLOSE`), and none needed to be force-stopped.

## 19. Verdict: **PARTIAL**

| Exit-gate group | Status |
|---|---|
| Native capture: real WGC, real frames, metadata, start/stop, teardown | **Met.** Teardown leaves ≈ 3 OS handles per run (documented) |
| Local preview: real frames visible in TP Remote, user-selected target, live updates | **Not met at app level** (NOT RUN). Met at host level on real frames |
| Security: no bypass, separate grant, no remote, no input, no hidden recording, no listener, border kept, no service | **Met** (tests and static guards; app-level border observation NOT RUN) |
| Quality: builds, regressions, security tests, real-device capture checks, performance data | **Met** |
| Stability: 10 cycles, 30 minutes, no crash, no uncontrolled leaks | **Met at provider and host level**: 10/30/100 cycles; 30 min with frames and flat memory (soak run 2). Leaks: ≈ 3 OS handles per capture run, bounded, documented. App-level runs NOT RUN |

- **Why not PASS:** capture can never start without a trusted local selection, so FAIL does not apply, and no mandatory security test failed. But the prompt's "real frames visible in TP Remote" and the app-level stability runs were not verified, so the verdict is **PARTIAL**.

## 20. Readiness for CAP-02B (macOS) and NET-03

**CAP-02B can build on:**
- the amended contract: `CapturePicker` maps to `SCContentSharingPicker`, `Target` to `SCContentFilter`, `FrameReadback` to an IOSurface readback;
- the `unsupported` provider as the starting point;
- the platform-independent preview host and UI;
- the test pattern idea and the bench/soak tools, which need a macOS window backend.

It is still blocked without DEV-M1 (a macOS 15+ Apple Silicon Mac).

**NET-03 can build on:**
- unchanged remote authorization and a capture provider ready for host-side use.

NET-03 must also:
- write a new ADR before any programmatic display targeting after remote consent (ADR-007 does not authorize it);
- use the validated WGC display mapping;
- keep `platform_capabilities()` NONE until transport and consent exist.

**Recommended before either phase:**
1. Complete T-DEV-21 with a person, using `scripts/capture-qa-windows.ps1`. It passes only when the app's log corroborates the selection and the preview.
2. Decide on R-8 (WebView2 outbound connections).
3. Review the uncommitted changes on top of `7e1a661`.

**STOP.** CAP-02B and NET-03 have not been started. The assistant made no commit, push, deployment, service installation or publication.

---

## 21. CAP-02A-R2 addendum: app-level acceptance attempt and Git reconciliation (2026-10-28)

Verdict after R2: **PARTIAL (unchanged)**. Exact blocker: **no person completed a target selection in the Windows picker** on DEV-W1 (§21.2, §21.3).

### 21.1 Git HEAD and worktree

- HEAD `7e1a661` "CAP-02A: Windows capture pipeline and local preview" on `main`, level with `origin/main`.
- The repository is **public** on GitHub.
- No stash. The assistant made no commit and no push.
- At the start of R2 there were 33 uncommitted paths; all are CAP-02A work done after `7e1a661`, and all are preserved.
- R2 changes are listed in §21.8.

### 21.2 Root cause of the operator/app mismatch: **identified (operator-side interaction), app path verified**

What R2 added:
- Instrumentation: IPC entry and result logs; a status-poll counter; a window title tagged with label and PID; timestamped log lines; window-state columns.
- A read-only WinEvent hook on minimize and foreground changes.
- A read-only z-order check.

| Question (prompt §2) | Finding | Evidence |
|---|---|---|
| Correct TP Remote instance? | Yes. One `tp-desktop` process per session, titled `TP Remote - QA <label> - PID <pid>`; its UI polled the backend about once a second (5,280 polls in session 1) | `r2-session*.events.log` |
| Window minimized or obscured? | **Yes.** Session 1: fully covered by two Visual Studio Code windows (z-order), then minimized at 17:40:37. Raised topmost at 17:42:50; minimized again at 17:48:03 together with the topmost test pattern. Session 2: both windows minimized within 5 s of launch. Session 3: TP Remote, the pattern and its console window minimized one after another; each minimize came 60–90 ms after the window was activated, the signature of a click on its minimize button | `r2-session1.samples.csv`, `r2-session3.window-events.log` (other applications' titles redacted) |
| Start button visible and enabled? | Yes when the window was shown: screenshots show `Ready` and an enabled **Start local preview** | `r2-session1.shot-3.png`, `r2-session3.shot-1.png` |
| Did React dispatch the click, did IPC reach Rust? | **Yes.** One diagnostic click by the QA lead (with the operator's permission) at 20:53:02.149 → `ipc.start_local_preview result=received` at .420 → `preview.start_requested generation=1` → `result=ok state=selectingTarget` at .927 | `r2-session3.events.log`, `r2-session3.operator-notes.txt` |
| Did the OS picker appear for the correct window? | **Yes:** foreground window "Capture with TP Remote - QA r2-session3 - PID 16388" (class `ApplicationFrameWindow`) at (82, 137), 976×681, over TP Remote | `r2-session3.window-events.log` |
| Did target selection complete? | **No.** The picker closed without a selection at 20:53:42.6 (TP Remote regained focus); `preview.picker_cancelled generation=1` at 20:53:43.073. The QA lead's restore of the pattern came 317 ms after the picker had closed | same |
| Did status reach React? | Not reached: no capture started | — |
| Operator reports | Seven reports of success (four in CAP-02A, three in R2); none corroborated. In 1,029 samples over 90 minutes, TP Remote was never the foreground window; a real click on it would have made it foreground | `r2-session1.samples.csv` |

**Conclusion:** TP Remote behaved correctly throughout, and the click → IPC → picker path is proven in the real WebView. The operator's input did not reach TP Remote: its window was covered or minimized. The picker, when open, was closed without a selection.

**Environment facts recorded:**
- Chrome Remote Desktop host, TeamViewer and UltraViewer agents were running.
- The display layout changed from 3 monitors to 1 between 19:03 and 20:47; no display setting was changed by the phase.
- The console is not an RDP session.

Why the operator's view and the machine disagree remains **UNKNOWN**. Session 1's cause was not observed directly; R2 shows the same pattern (minimized windows, scroll position changed, no requests).

### 21.3 App-level capture evidence

| Required evidence (prompt §3) | Result |
|---|---|
| `preview.start_requested` | **Observed:** CAP-02A session 1 generation 1 (operator click); R2 session 3 generation 1 (diagnostic click) |
| Picker selection (`preview.target_selected`) | **Not observed.** Both picker invocations ended in `preview.picker_cancelled` |
| `preview.capturing`, `preview.first_frame`, increasing `preview.stats` (frames / painted), screenshot of synthetic content in TP Remote, `ipc.stop_local_preview` / `preview.ended` | **NOT RUN**: they need a selection |
| Cancellation captures nothing | **PASS at app level** (twice): after `picker_cancelled`, state Ready; no capture thread; no grant |
| Clean exit and restart | **PASS (idle):** 4 R2 sessions closed with `WM_CLOSE`, exit and 0 survivors; each relaunch reached Ready. **Exit while capturing: NOT RUN** |

### 21.4 Ten-cycle application results

**NOT RUN** (blocked by §21.3). Provider (10/30/100) and host-level (10) cycles from CAP-02A stand; they are not counted as application results.

### 21.5 Security and IPC

- **Unchanged:** default-deny, `LocalPreviewGrant`, capabilities and CSP.
- **New log events:** `ipc.start_local_preview result=received|ok|err`, `ipc.stop_local_preview`, `ipc.preview_status_polls`. They use only allowlisted keys: `result`, `state`, `error_code`, `count`.
- **WebView2 networking** (`CAP-02A-R2/app/webview2-network-review.txt`):
  - All TCP connections belong to WebView2 runtime processes. TP Remote's own code opens none; no listener; no UDP.
  - `8.8.4.4:443` is `dns.google`: secure DNS with this PC's configured resolvers 8.8.8.8 / 8.8.4.4.
  - Two Microsoft-range endpoints: their service is UNVERIFIED. SmartScreen is already disabled by Tauri/wry's default `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection`.
  - **No app-controlled path for pixels:** no networking crate in the app graph, no socket API in our Rust, no network API in the frontend, CSP `connect-src ipc: http://ipc.localhost`.
- **Hardening (documented, not applied):**
  - review WebView2 secure-DNS and diagnostic-data behaviour, and enterprise policies, in REL-09;
  - keep updates and certificate validation enabled.
- **Evidence privacy:**
  - The pushed commit `7e1a661` publicly contains the Windows account name in log paths and, in `CAP-02A/app/session1-functional.summary.json`, a PowerShell provider dump (profile path, drive list) and two LAN addresses. Cause: a QA-script serialization bug, now fixed.
  - The working tree is redacted (`<user>`, `<lan-ip>`; other applications' window titles redacted).
  - Rewriting public history is **the owner's decision**; it was not done.

### 21.6 Regression results

See [test results §10](../qa/CAP-02A-test-results.md#10-cap-02a-r2-reconciliation).

### 21.7 Remaining Windows limitations

- The app-level picker flow with a person is unverified.
- Covered or minimized windows deliver no frames, and secondary-display stalls are probably related to minimized test windows (unverified).
- About 3 handles per capture run.
- Real device loss, UAC and lock screen, and resolution or scale changes are untested.
- Live-WebView ACL is unverified.

### 21.8 Files modified in R2

| File | Change |
|---|---|
| `apps/desktop/src-tauri/src/lib.rs` | Start/stop entry and result logs; status-poll counter (no behaviour change) |
| `apps/desktop/src/app/useLocalPreview.ts`, `components/LocalPreviewCard.tsx`, `LocalPreviewCard.test.tsx` | "Frames drawn" counter and its test |
| `scripts/capture-qa-windows.ps1` | Window title tag and placement, `-AppTopmost`, timestamped events, window-state columns, unique labels, summary serialization fix |
| `docs/qa/evidence/CAP-02A/negative-controls/run-negative-controls.ps1` | `-Log` parameter |
| `docs/qa/evidence/CAP-02A-R2/**` (new) | Sessions, window-event logs, network review, gates, negative controls, watcher script |
| `docs/qa/evidence/CAP-02A/**` | Privacy redaction; summaries reduced to plain event lines |
| CAP-02A report, test results, performance, security review, README, TEST-MATRIX | Reconciled |

### 21.9 Verdict: **PARTIAL**

PASS needs a real person's picker selection, moving frames in TP Remote, non-zero drawn-frame counts and ten application cycles. None was achieved, for the reason in §21.2. Every other gate item is met.

### 21.10 Readiness for CAP-02B

Unchanged from §20:
- The contract, host and UI are ready for a macOS provider.
- CAP-02B needs DEV-M1.

Recommended before CAP-02B:
- Run T-DEV-21 with an operator working directly at the DEV-W1 console, using `scripts/capture-qa-windows.ps1 -AppTopmost`. The new logs show within a second whether each step reached the app.
- Decide on the public history (§21.5).
