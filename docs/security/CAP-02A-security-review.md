# CAP-02A — Security review: Windows capture and local self-preview

| Field | Value |
|---|---|
| Phase | CAP-02A |
| Date | 2026-10-28 |
| Scope | `tp-capture::wgc`, `tp-core::preview`, the five new Tauri commands, the preview UI, the `windows` 0.62.2 dependency |
| Decision record | [ADR-007](../architecture/ADR-007-local-preview-authorization.md) |
| Status legend | IMPLEMENTED · TESTED (automated) · VERIFIED (observed on DEV-W1) · NOT RUN · UNVERIFIED · FUTURE |

## 1. Summary

CAP-02A adds the first real operating-system capability: Windows.Graphics.Capture. It is used **only** for a local self-preview.

- **Remote authorization is unchanged.** `platform_capabilities()` = NONE, there is no transport, and BOOT-02's consent and policy types are untouched.
- **Separate local authorization.** The local preview has its own minimal authorization: the Windows picker is the selection boundary, and `LocalPreviewGrant` is the proof.
- **No mandatory security test failed.**
- **Main residual risks:**
  - App-level verification of the picker flow with a person is incomplete (§9, R-6).
  - A WebView compromise could open the picker repeatedly and read preview pixels while a preview runs (R-1, R-2).
  - About 3 process handles per capture run are not returned by WGC (R-5).

## 2. Trust boundaries

| ID | Boundary | Who is on each side | Control |
|---|---|---|---|
| TB-LOCAL-PICK (new) | WebView request → Windows capture picker | Untrusted WebView script; the OS-drawn picker operated by the local user | The IPC request only opens the picker; a target exists only after the user selects one; cancel captures nothing |
| TB-IPC (existing) | WebView → Rust commands | WebView; Rust shell | Capability ACL (main window, local origin), per-command label check, typed arguments |
| TB-OS-CB (new) | WGC thread-pool callbacks → Rust | OS threads; our sink | `catch_unwind` on every callback; delivery lock; no unwinding into WinRT |
| TB-TEST (new) | Test-only code → app build | `tp-capture/test-support`, `tp-test-harness` | Feature never in the app graph (`cargo tree` guard); programmatic targets only in `testing.rs` |

## 3. Local self-preview authorization (ADR-007)

| Invariant | Mechanism | Evidence | Status |
|---|---|---|---|
| No capture without a local selection in the OS picker | `start` only calls `pick`; capture is created only on `PickOutcome::Selected` for the current generation | `tp-core/tests/preview.rs::no_capture_starts_without_a_picker_selection`; machine invariant I4 | TESTED |
| Cancel captures nothing | `cancelled(g)` → `Ready`; no grant, no `start` | `cancelling_the_picker_captures_nothing`; app log, session 1 (generation 1) | TESTED, VERIFIED |
| A stale selection cannot (re)start capture | Generation and state check in `selected(g)`; target dropped | `stale_selections_cannot_restart_capture`; machine invariant I2 | TESTED |
| The grant is minted in one place only | `LocalPreviewGrant::on_picker_selection` (private) | Boundary test `only_the_picker_completion_mints_a_preview_grant`; compile-fail doctest (no forging) | TESTED |
| The grant cannot authorize remote viewing or input | Distinct type, not `Clone`, no `From`/`Into` `ConsentGrant`; `authorize()` takes only `AuthorizationContext` | Compile-fail doctests; `a_local_preview_grant_authorizes_neither_remote_viewing_nor_input` (ViewScreen/ControlInput denied, InputGate injects nothing) | TESTED |
| Stop revokes | Grant dropped; slot cleared; sink stops accepting; late frames ignored | `stop_revokes_preview_access`; host-level real-device test | TESTED, VERIFIED (host level) |
| The picker opens only over our own live window | `verify_owner`: `IsWindow` + process id | `picker::tests::foreign_or_invalid_owners_are_rejected_before_any_picker_exists`; IPC test with the mock runtime's fabricated handle → `forbidden` | TESTED |
| One request at a time | `request_start` → `Busy` while selecting, initializing or capturing | `a_second_start_is_rejected_while_one_is_in_progress` | TESTED |
| Production targets come only from the picker | `WgcTarget::from_picker` in `picker.rs` only; `programmatic` only in `testing.rs` (test-support) | Boundary test `programmatic_capture_targets_are_test_only`; `the_app_build_graph_has_no_test_support` | TESTED |
| No remote peer can issue a preview grant | No network code; no protocol message refers to preview; grant minting is local and private | Code review; boundary tests | TESTED (static) |

## 4. IPC surface (CAP-02A)

| Command | Arguments | Effect | Thread |
|---|---|---|---|
| `get_capture_displays` | none | Enumerates monitors (sanitized device names, geometry) | main |
| `get_local_preview_status` | none | Read-only status, including the local target label | main |
| `start_local_preview` | none | Opens the Windows picker over the main window | main (required by the picker) |
| `stop_local_preview` | none | Stops and revokes; always allowed; idempotent | main |
| `get_local_preview_frame` | `generation: u32`, `painted: u32` | Moves the latest preview frame out (binary) | main |

| Control | Evidence | Status |
|---|---|---|
| Granted only to the `main` window, by the capability ACL | `config.rs` exact permission list; `ipc.rs::preview_commands_are_denied_to_other_windows_and_remote_origins` | TESTED (mock runtime with the real ACL) |
| Denied to remote origins (`https://example.com`, a LAN IP, `http://localhost:1420`) | same test | TESTED |
| Label check in each command (defence in depth) | `require_main` → `Forbidden` | TESTED (indirectly: the ACL denies first) |
| No command takes a target, a handle, a path or a monitor id | Signatures above | Code review |
| Invalid arguments rejected | `frame_requests_with_invalid_arguments_are_rejected` | TESTED |
| Unknown and capture-shaped commands do not exist | `unknown_commands_are_rejected` (13 names) | TESTED |
| No plugins, no shell/fs/http, no core permissions | BOOT-01 tests unchanged | TESTED |
| Real WebView ACL behaviour | Needs a WebView automation driver (deferred since BOOT-01; Playwright browsers not installed). Not attempted via a privileged backdoor | **UNVERIFIED** (carried) |
| Tauri `dynamic-acl` default feature | Still enabled by Tauri's defaults; no runtime ACL changes are made by the app | Open (carried) |

## 5. Data handling and logging

- **Pixels:**
  - Never written to disk; a boundary test forbids file APIs in the capture, preview and shell paths.
  - Never logged, and never sent over the network (no network code).
  - Preview frames exist in three places only: the single slot in Rust, the IPC response in transit, and the WebView canvas.
- **Target labels:**
  - The label is the window title for window targets, and can be private.
  - It is shown in the local UI only. It is never logged: a test checks this with a synthetic private title, on the real device with the test-pattern title.
- **New log keys:**
  - `generation`, `target_kind`, `width`, `height`, `elapsed_ms`, `frames`, `previewed`, `skipped`, `dropped`, `painted`, `fps`, `preview_fps`.
  - All carry counters or fixed identifiers. Tests check that no preview log line contains `[REDACTED]`, which would mean an unlisted key.
- **OS errors:**
  - HRESULTs are kept in `tp_capture::wgc::last_os_error_code()` for diagnostics only.
  - UI errors are fixed texts with recovery steps.
- **WebView exfiltration:**
  - The production CSP keeps `connect-src ipc: http://ipc.localhost`, `img-src 'self' data:`, with no remote origins.
  - Preview pixels in the canvas therefore cannot be sent anywhere by page script under this CSP. This is not a defence against a full WebView process compromise (R-2).

## 6. Capture indicator, stealth and OS restrictions

| Requirement | Status |
|---|---|
| Yellow border kept on | IMPLEMENTED; static check: no `IsBorderRequired`, `Borderless`, `GraphicsCaptureAccess` or `graphicsCaptureWithoutBorder` anywhere in production code (TESTED). Observed during provider runs; app-level observation by the operator: NOT RUN (§9) |
| Visible capturing state and Stop control | UI badge "Capturing" with a red dot; Stop always enabled while selecting, initializing or capturing; "No capture is active" afterwards (Vitest) |
| No background capture, no service, no autostart | Capture exists only inside the app process while a preview runs; stopped on `ExitRequested`/`Exit`; no services or tasks added |
| No capture without user interaction | Picker required; no programmatic target in the app build |
| Secure desktop, protected content | Not attempted and not bypassed. UAC and lock-screen behaviour: NOT RUN (T-DEV-03) |
| Elevation | None. Medium integrity re-checked (test results) |

## 7. Unsafe code

| File | Unsafe use | Justification |
|---|---|---|
| `wgc/d3d.rs` | `D3D11CreateDevice`, `CreateDirect3D11DeviceFromDXGIDevice`, `GetInterface`, `GetDesc`, `CopySubresourceRegion`, `Map`/`Unmap`, `slice::from_raw_parts` over mapped memory | The mapped slice is bounded by `RowPitch × (h−1) + w×4` and dropped before `Unmap`; the context is used only under the staging lock |
| `wgc/monitors.rs` | `EnumDisplayMonitors` callback, `GetMonitorInfoW`, `GetDpiForMonitor`, `EnumDisplaySettingsW` | The callback receives a pointer to a local `Vec` that is valid for the call; struct sizes are set |
| `wgc/picker.rs` | `IInitializeWithWindow::Initialize`, `IsWindow`, `GetWindowThreadProcessId` | The owner is verified before use |
| `wgc/target.rs` | `CreateForMonitor` (name lookup) | Handle from the current enumeration; the call validates it |
| `wgc/testing.rs` (test-support) | Window class, message loop, GDI painting, `CreateForWindow`/`CreateForMonitor` | Test-only |

- **Static guard:** `unsafe_code_is_confined_and_justified` asserts that `unsafe` occurs only under `crates/tp-capture/src/wgc/`, each time preceded by a `SAFETY` comment.
- **Finding:** it found two callback declarations without one (F-CAP-1).
- Every other crate keeps `#![forbid(unsafe_code)]` or the workspace `deny`.

## 8. Dependency

| Crate | Version | License | Change |
|---|---|---|---|
| `windows` (Microsoft windows-rs) | 0.62.2 | MIT OR Apache-2.0 | New **direct** dependency of `tp-capture` (approved). Already locked through Tauri; `Cargo.lock` gained only dependency-list lines (`windows` for tp-capture; internal crates for tp-test-harness). No new package |

Features used:
- `Foundation`, `Graphics_Capture`, `Graphics_DirectX_Direct3D11`, `Win32_Foundation`;
- `Win32_Graphics_{Direct3D, Direct3D11, Dxgi, Dxgi_Common, Gdi}`;
- `Win32_System_WinRT_{Direct3D11, Graphics_Capture}`;
- `Win32_UI_{HiDpi, Shell, WindowsAndMessaging}`.

`Win32_System_Performance` was approved but turned out to be unnecessary and is **not** enabled. The app build graph has one `windows` version (test).

## 9. Findings

| ID | Finding | Severity | Status |
|---|---|---|---|
| F-CAP-1 | Two `unsafe extern "system"` callbacks (monitor enumeration, test window procedure) had their SAFETY reasoning only inside the body | Low | **Fixed** (SAFETY contracts added; enforced by test) |
| F-CAP-2 | `GraphicsCaptureItem.Closed` was not raised when a captured window was destroyed (DEV-W1) | Medium (a preview could otherwise keep "running" a dead target) | **Mitigated**: 250 ms liveness watchdog; TESTED on the device |
| F-CAP-3 | No frames for a covered GDI window, and intermittently for windows on DISPLAY1/DISPLAY2 even when topmost; new sessions on such a window get no frame at all. 100/100 runs fine on the primary display. Pool starvation excluded (no queued frames found) | Medium (functional; honest 0 fps, no false data) | **Open, OS behaviour, root cause UNVERIFIED** (performance report §5) |
| F-CAP-4 | Each capture run leaves 3 process handles (1 per `GraphicsCaptureItem`, 2 per free-threaded frame pool) although both are closed and released | Low (≈3 handles per preview start; freed at exit) | **Open, apparently OS-side, root cause UNVERIFIED**; our own extra monitor items now cached (R-5) |
| F-CAP-5 | Script in the main WebView can call `start_local_preview` repeatedly | Low | Accepted: a local user selection is still required; one in-flight request; picker is OS-drawn (R-1) |
| F-CAP-6 | Preview pixels live in WebView memory while shown; window titles appear in the local UI | Low | Accepted for local preview; CSP blocks exfiltration by page script; never logged (R-2, R-3) |
| F-CAP-7 | App-level picker flow with a person (select, live preview, border, stop, 10 cycles, 30 min) not verified: the operator's reports were not corroborated by the app | Medium (assurance gap) | **Open**: NOT RUN; host-level substitutes run on real frames (test results §4) |
| F-CAP-8 | TCP connections were seen in the app's process tree (WebView2 processes) | — | Characterised in the test results (T-CAP-SEC-10) |

## 10. Residual risks

| ID | Risk | Owner phase |
|---|---|---|
| R-1 | A WebView compromise can trigger the picker (user must still select) | UX-08 / REL-09 hardening (isolation pattern, TM-16) |
| R-2 | A WebView compromise can read preview pixels while a preview runs | Same; acceptable only for local self-preview |
| R-3 | Window titles shown locally | Accepted (local user's own data) |
| R-4 | Live-WebView ACL behaviour and `dynamic-acl` remain unverified | REL-09 |
| R-5 | WGC handle growth per run (F-CAP-4) | Re-measure on other drivers and builds (W2); report upstream if confirmed |
| R-6 | App-level manual verification incomplete (F-CAP-7) | Reviewer / CAP-02A follow-up run |
| R-7 | Future remote capture will need programmatic display targets after consent; not authorized by ADR-007 | NET-03 / VID-04 (new ADR) |
