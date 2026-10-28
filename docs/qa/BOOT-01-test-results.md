# BOOT-01 — Test Results

| Field | Value |
|---|---|
| Phase | BOOT-01 |
| Date | 2026-10-28 |
| Device | DEV-W1: Windows 11 Enterprise LTSC 26100 x64, 3 × 1080p at 96 DPI, WebView2 155.0.4283.62 |
| Toolchain | rustc 1.99.1 (MSVC 14.44), Node 24.19.0, npm 11.16.0 |
| Evidence root | `docs/qa/evidence/BOOT-01/` |
| Rule | Every result below was observed in this phase. Anything not executed is marked NOT RUN or BLOCKED, never PASS |

## 1. Summary

| Area | Result | Counts (from actual output) |
|---|---|---|
| Rust format / check / clippy (`-D warnings`) | PASS | 0 diagnostics |
| Rust tests (`cargo test --workspace --locked`) | PASS | **51 passed, 0 failed**, 0 ignored (35 test targets) |
| TypeScript typecheck / ESLint (`--max-warnings=0`) | PASS | 0 errors, 0 warnings |
| Vitest | PASS | **18 passed, 0 failed** (3 files) |
| Frontend production build | PASS | `index.html` 0.44 kB, JS 229.6 kB, CSS 14.3 kB (deterministic across rebuilds) |
| Bindings drift check | PASS | 11 files up to date |
| Native builds (`tauri build --no-bundle`, release and `--debug`) | PASS | `target\release\tp-desktop.exe` 8,679,936 B; `target\debug\tp-desktop.exe` 12,387,840 B |
| Windows runtime checks WIN-BOOT-01 … 08 | 7 PASS, 1 PASS with a scope limit (WIN-BOOT-05: 100% scaling only) | §3 |
| Local development launch (`npm run dev`) | PASS | Ready in 17.3 s; whole tree exited on close |
| One-command gate run (`npm run check`) | PASS | 9 of 9 gates (`evidence/BOOT-01/check-all.log`) |

**Not measured:** coverage percentages. No coverage tool is installed, and no figures are claimed.

## 2. Mandatory tests (BOOT-01 prompt §7)

| # | Requirement | Tests (file → test names) | Result |
|---|---|---|---|
| 1 | Bootstrap DTO serialization | `crates/tp-contracts/tests/ipc_fixtures.rs`: `rust_serialization_equals_the_valid_fixture`, `the_valid_fixture_deserializes_to_the_canonical_value`, `serialization_round_trips`, `a_missing_webview_version_serializes_as_null`, `ipc_error_uses_camel_case_codes`. `crates/tp-protocol/tests/codec.rs`: `encoding_matches_the_canonical_fixture_byte_for_byte`, `encoding_is_deterministic_across_calls`, `encode_then_decode_round_trips` | PASS |
| 2 | Invalid protocol version handling | Fixtures `version-0`, `version-2`, `version-u64-max` (→ `unsupportedVersion`) and `version-string`, `version-negative`, `version-float`, `missing-version` (→ `invalidEnvelope`) via `fixtures.rs::every_fixture_produces_its_expected_result`; `codec.rs`: `unsupported_version_reports_the_supported_range`, `version_support_is_exactly_v1`, `writer_uses_the_current_protocol_version` | PASS |
| 3 | Typed Rust→TS contract generation | `crates/tp-test-harness/tests/bindings.rs` (5): determinism, ts-rs header, camelCase shape, no `bigint`, **committed bindings have not drifted**. `apps/desktop/src/contract/appInfo.contract.test.ts` (5): the same fixture accepted unchanged, all 5 invalid fixtures rejected, only generated bindings used. Negative control: §4 | PASS |
| 4 | Frontend loading state | `App.test.tsx`: "shows Loading while get_app_info is pending, and no data" | PASS |
| 5 | Frontend successful IPC state | `App.test.tsx`: "shows Ready with the values returned by Rust" (also asserts the only command sent is `get_app_info`) | PASS |
| 6 | Frontend failed IPC state | `App.test.tsx`: "shows Failed with the error from Rust when the command rejects", "shows Failed when Tauri rejects the call itself", "shows Failed, not Ready, when the response violates the contract", "Retry calls the core again and can reach Ready" | PASS |
| 7 | Workspace crate dependency constraints | `crates/tp-test-harness/tests/workspace_boundaries.rs` (6): exact crate set, allowed edges only, acyclic, only the shell uses Tauri, no async runtime in platform crates, `forbid(unsafe_code)` present. Negative control: §4 | PASS |
| 8 | Safe startup and teardown | `apps/desktop/src-tauri/tests/ipc.rs::startup_creates_only_the_main_window_and_teardown_is_clean` (mock runtime) + WIN-BOOT-01, 06, 07 on the real host + the dev-launch tree check | PASS |

### 2.1 All Rust tests (51)

| Target | Tests | Names |
|---|---|---|
| tp-contracts `ipc_fixtures` | 6 | see #1, plus `invalid_fixtures_are_rejected_by_rust` |
| tp-core (unit) | 4 | `engine_modules_cover_every_engine_crate_exactly_once`, `no_remote_capability_crate_claims_an_implementation_in_boot01`, `app_info_reports_this_build_honestly`, `build_profile_matches_debug_assertions` |
| tp-diagnostics (unit) | 4 | `formats_plain_fields`, `values_cannot_inject_lines_or_fields`, `keys_and_events_are_restricted_to_identifier_characters`, `empty_and_quoted_values_are_escaped` |
| tp-protocol `codec` | 11 | determinism, round-trip, version, size limit before parsing, size boundary, encoder refuses invalid messages, error sanitisation, deep nesting |
| tp-protocol `fixtures` | 2 | 24 manifest cases (2 valid + 22 rejections); every fixture file is covered by the manifest |
| tp-test-harness `bindings` | 5 | see #3 |
| tp-test-harness `workspace_boundaries` | 6 | see #7 |
| tp-desktop `config` | 4 | only capability = `allow-get-app-info` for window `main`, no remote origins; locked-down `tauri.conf.json` (CSP without `unsafe-inline`/`unsafe-eval`/remote sources, `withGlobalTauri: false`, `freezePrototype: true`, `bundle.active: false`, loopback `devUrl`); no Tauri plugins; Vite bound to 127.0.0.1 |
| tp-desktop `ipc` | 9 | §2.2 |

### 2.2 IPC and ACL tests (mock runtime with the app's **real** config and capabilities)

| Test | Observed rejection text (from the `--nocapture` run) | Result |
|---|---|---|
| `get_app_info_returns_the_typed_contract` | Ok; deserializes to `AppInfo` (phase BOOT-01, `remoteFeatures` notImplemented, 11 modules) | PASS |
| `unknown_commands_are_rejected` | e.g. `launch_remote_session not allowed. Command not found` (also `start_capture`, `inject_input`, `open_listener`, `GET_APP_INFO`, `get_app_info `) | PASS |
| `plugins_that_are_not_installed_are_rejected` | e.g. `shell.execute not allowed. Plugin not found` (also shell.spawn, fs.read_text_file, fs.write_file, http.fetch, process.exit) | PASS |
| `core_commands_are_denied_because_no_core_permission_is_granted` | e.g. `window.close not allowed. Permissions associated with this command: core:window:allow-close` (also app.version, window.set_title, webview.create_webview_window, event.listen, path.resolve_directory) | PASS |
| `windows_other_than_main_cannot_call_get_app_info` | `get_app_info not allowed on window "secondary" … allowed on: [windows: "main", URL: local]` | PASS |
| `remote_origins_cannot_call_get_app_info` | `get_app_info not allowed on window "main", webview "main", URL: https://example.com/ …` (also `http://192.168.1.10:1420`) | PASS |
| `without_fault_injection_the_response_is_ok` / `debug_builds_can_inject_an_ipc_fault` | Ok / `InjectedFault` | PASS |
| `startup_creates_only_the_main_window_and_teardown_is_clean` | Windows = `["main"]`; close + `cleanup_before_exit` | PASS |

Test executables print "failed to load the default window icon from the application icon resource". Test binaries don't embed the icon resource, so this is expected there. The real app shows its icon in the title bar (screenshots in §3).

## 3. Windows runtime checks (real WebView2 on DEV-W1)

**Method.** `scripts/smoke-windows.ps1` and `scripts/smoke-dev-windows.ps1` do the following:
- Start the executable, or `npm run dev`.
- Wait, bounded and condition-based, for the app's own stderr event `ipc.get_app_info result=…` and a visible main window.
- Record the process tree, the TCP/UDP endpoints owned by that tree, the token elevation and integrity level, and the DPI awareness.
- Capture **only the app window** with `PrintWindow(PW_RENDERFULLCONTENT)`.
- Close the window with WM_CLOSE and verify that every process in the tree has exited.

Screenshots were inspected visually.

| ID | Environment | Steps | Expected | Observed | Result | Evidence |
|---|---|---|---|---|---|---|
| WIN-BOOT-01 Tauri app starts | DEV-W1, release no-bundle exe (SHA-256 `212AC17E…5B320`) | `smoke-windows.ps1 -Exe target\release\tp-desktop.exe -Label release-run1 -Resize` | Process starts, window visible, no crash | Window "TP Remote" visible; events `app.start` → `app.ready`; tree = tp-desktop + 6 msedgewebview2; token **elevated=False, integrity Medium**; DPI awareness **PerMonitorV2**, 96 DPI | PASS | `evidence/BOOT-01/windows/release-run1.summary.json` |
| WIN-BOOT-02 React renders in WebView2 | as above | Inspect `release-run1.initial.png` | Header, status, notice, diagnostics, module table render | All sections render with correct styling; app icon in the title bar | PASS | `release-run1.initial.png` |
| WIN-BOOT-03 IPC returns typed data | as above | stderr event + screenshot | `get_app_info` succeeds; UI shows Rust values | `ipc.get_app_info result=ok` **0.45 s** after launch. UI shows Ready, 0.1.0, BOOT-01, windows (windows), x86_64, release, v1 (supports v1–v1), Tauri 2.12.5, WebView 155.0.4283.62, 11 modules with their states | PASS | `release-run1.*` |
| WIN-BOOT-04 Invalid IPC exposes nothing privileged | Mock runtime with the real ACL (§2.2) + static config tests | `cargo test -p tp-desktop` | Unknown, plugin, core, other-window and remote-origin calls rejected; no plugins linked | All rejected with Tauri ACL messages; capability grants only `allow-get-app-info`; no `tauri-plugin-*`; no `devtools` feature in the app build | PASS (see limitation) | `evidence/BOOT-01/rust/test.log`; §2.2 |
| WIN-BOOT-05 Resize and standard DPI | DEV-W1 at 100% (96 DPI) | `SetWindowPos` to 500×400, 1024×700, 1600×1000, and 1100×760 on the left monitor at (−1820,171) | Layout remains usable; no overlap or clipping | Single column with scrolling at 500×400; centered max-width layout at 1600×1000; renders correctly on the negative-coordinate monitor. **Observed:** an external `SetWindowPos` is not clamped to the configured 720×560 minimum (500×400 was applied), and the layout still holds | **PASS at 100% scaling**. 125%/150% **NOT RUN** (would require changing display settings) | `release-run1.resize-*.png` |
| WIN-BOOT-06 Clean close, no orphan processes | release-run1, release-run2, debug-fault, release-fault-env-ignored | WM_CLOSE → wait ≤ 15 s → check every process in the tree | Exit code 0; no survivors | All four runs: `closeMessageSent=True`, `exitedAfterClose=True`, `exitCode=0`, `survivingPids=[]`, event `app.exit`. No process force-stopped | PASS | `*.summary.json` |
| WIN-BOOT-07 Subsequent start | release-run2 right after release-run1 | Same script | Starts and serves IPC again | `result=ok` after 0.42 s; clean exit again | PASS | `release-run2.summary.json` |
| WIN-BOOT-08 IPC failure shows an error state | Debug no-bundle exe (SHA-256 `A6BCCFF5…`) with `TP_REMOTE_DEV_FAIL_APP_INFO=1` | `smoke-windows.ps1 -Exe target\debug\tp-desktop.exe -Label debug-fault -InjectFault` | UI shows Failed with the message; never Ready | Event `ipc.get_app_info result=err code=injectedFault`. UI shows **Failed**, "get_app_info failed: fault injected for testing (debug build)", `code: injectedFault`, Retry; no diagnostics; header shows placeholders | PASS | `debug-fault.initial.png`, `debug-fault.summary.json` |

**Supplementary checks:**
- **Fault injection unavailable in release.** The release exe was run with `TP_REMOTE_DEV_FAIL_APP_INFO=1` and still returned `result=ok` (`release-fault-env-ignored.summary.json`). PASS.
- **No network listener.** No TCP or UDP endpoint was owned by any process of the app tree in any run (`tcpListeners: []`, `udpEndpoints: []`). PASS.
- **Local development launch.** `smoke-dev-windows.ps1` showed:
  - the app was Ready after 17.3 s (including the dev build);
  - Vite listened **only on 127.0.0.1:1420**;
  - no other listener existed in the 20-process tree;
  - closing the app window ended the whole tree (npm, Vite, cargo, app, WebView2), with no survivors, nothing force-stopped, and port 1420 free afterwards.

  Dev mode makes two `get_app_info` calls; this is React StrictMode's deliberate double effect in development. PASS (`dev-launch.summary.json`, `dev-launch.png`).

**Limitation for WIN-BOOT-04.** The rejection tests run through Tauri's ACL on the mock runtime. They are not script injection inside the live WebView2: that would need DevTools or a remote-debugging port, which BOOT-01 avoids. The ACL enforcement code exercised is Tauri's own, and the live app has the same configuration and capability files.

## 4. Negative controls (do the guards fail when they should?)

| Guard | Deliberate violation | Observed | Restored |
|---|---|---|---|
| Crate boundaries | Added `tp-capture → tp-transport` | Test failed: "tp-capture -> tp-transport (normal) is not allowed" | Reverted; 6 of 6 pass; `Cargo.lock` hash unchanged |
| Bindings drift | Hand-edited `BuildProfile.ts`; added a stray `Extra.ts` | `bindings:check` exit 1: "out of date: BuildProfile.ts", "not generated (remove it): Extra.ts" | Reverted; "is up to date" |
| Tauri-import restriction | `import { invoke } from "@tauri-apps/api/core"` outside `core-api/` | ESLint `no-restricted-imports` error | Probe file deleted |

Evidence: `docs/qa/evidence/BOOT-01/negative-controls/`.

## 5. Issues found and fixed during BOOT-01

| Issue | Fix | Verified by |
|---|---|---|
| Test executables linking Tauri crashed with `STATUS_ENTRYPOINT_NOT_FOUND` (0xc0000139): tauri-build embeds the Common-Controls-v6 manifest into the binary only | `build.rs` embeds the same manifest through the linker (`/MANIFEST:EMBED /MANIFESTINPUT`) for every target | `ipc.rs` 9 of 9 pass; the app still shows its icon and runs |
| Tailwind v4 automatic source detection scanned generated `src-tauri/gen/schemas`, so CSS output depended on build state (14.3 kB vs 17.4 kB) | `@import "tailwindcss" source(".")` plus `.gitignore` | Two rebuilds byte-identical (CSS SHA-256 prefix `50E55CFECEB3`) |
| `rustfmt --check` failed on hand-written code | `cargo fmt --all` | `fmt --check` exit 0 |
| ESLint: gitignore-style negation cannot re-include `@tauri-apps/api/mocks`; tests threw non-Error values | Regex import patterns; one documented helper that reproduces Tauri's string and object rejections | ESLint 0 problems |
| Smoke script: StrictMode error on an empty child list left an app instance running; summary bloated by PowerShell metadata | Empty-safe tree walk; `try/finally` always closes the launched app; string-cast log lines | Later runs clean; summaries ~2 KB |

## 6. TEST-MATRIX items in the BOOT-01 gate list

| ID | Status | Note |
|---|---|---|
| T-STA-01 TypeScript typecheck | PASS | |
| T-STA-02 ESLint with security rules | PASS | `no-restricted-imports` (regex) and `no-restricted-syntax` banning `dangerouslySetInnerHTML` |
| T-STA-03 rustfmt | PASS | |
| T-STA-04 Clippy `-D warnings` | PASS on Windows; macOS **BLOCKED** (no Mac) | |
| T-STA-05 Advisory audit | PARTIAL | `npm audit`: 0 vulnerabilities. `cargo audit`: **NOT RUN** (tool not approved for installation) |
| T-STA-06 License/source policy | PARTIAL | Inventory produced (report only). `cargo deny`: **NOT RUN** (not approved) |
| T-STA-07 Dependency compatibility | PASS on Windows (`--locked` builds, `tauri info` alignment, peer checks); macOS **BLOCKED** | |
| T-STA-08 Module boundaries | PASS | `inject(` call-site check: **N/A** (no injector exists yet) |
| T-STA-09 `unsafe` policy | PASS | |
| T-STA-10 Release hardening configuration | PASS for BOOT-01 scope | CSP, capabilities, no plugins, `bundle.active: false`, no `devtools`/`test` features and no harness or ts-rs in the app build graph |
| T-UNI-14 Serialization round-trip | PASS | JSON (U-05), not Protobuf |
| T-UNI-16 Decoder limits / malformed input | PASS | |
| T-INT-10 IPC contract Rust ↔ TS | PASS | |
| T-UNI-21 UI components | PARTIAL | Status card states covered; consent dialog and indicator not in BOOT-01 scope |
| T-UNI-01 … 06 (state machine), T-UNI-18 (redaction), T-UNI-22 (queues), T-INT-01 (capture pipeline), T-INT-13 (diagnostics snapshot) | **NOT RUN** | Not implemented: the BOOT-01 prompt limits scope to the minimum contracts for Rust↔TS integration. Deferral proposed in the BOOT-01 report §14 |
| T-UI-01 Playwright host flow | **NOT RUN** | Playwright browsers not installed (prompt §2); the UI states are covered by Vitest |
| T-SEC-16 / 17 / 18 (run early, for this app only) | PASS on DEV-W1 | No persistence entries, no listeners, medium integrity, not elevated |
| T-SEC-22 IPC ACL & CSP | PARTIAL | ACL proven on the mock runtime with the real config; no in-WebView injection attempt |

## 7. Not executed, with reasons

| Item | Reason |
|---|---|
| Any macOS build, test or runtime check | No Mac available. macOS support is **not** claimed |
| Display scaling other than 100% | Would require changing Windows display settings (not authorised) |
| Playwright / E2E in the live WebView | Browsers not installed; in-WebView injection would need DevTools or a remote-debugging listener |
| `cargo audit`, `cargo deny`, fuzzing | Tools not approved for installation |
| Coverage measurement | No coverage tool; no figures claimed |
| Remote-desktop tests (capture, input, network, streaming, four directions) | Out of BOOT-01 scope; no such capability exists |
