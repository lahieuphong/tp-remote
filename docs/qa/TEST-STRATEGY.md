# TP Remote — Test Strategy

| Field | Value |
|---|---|
| Status | Draft (ARCH-00) |
| Date | 2026-10-28 |
| Related | [Test matrix](TEST-MATRIX.md) (every test case and its criteria) · [ARCH-00](../architecture/ARCH-00.md) · [Threat model](../security/THREAT-MODEL.md) · [Prompt plan](../roadmap/PROMPT-PLAN.md) |

---

## 1. Principles

1. **Evidence or it did not happen.** A result counts as PASS only with stored evidence (§9). If evidence is missing the result is NOT RUN or BLOCKED, never PASS.
2. **Never fabricate.** Do not write expected values as if they were observed, do not carry results over from another device or OS, and do not "assume pass".
3. **Platform certification rule.** Only a run on that platform certifies behaviour for that platform. A Windows host cannot certify macOS behaviour, and the reverse holds too. Cross-compiling or type-checking is not certification.
4. **Separate test seams.** The engine is testable headless (`tp-test-harness`), with fakes behind every contract (ARCH-00 §4). The UI is testable without the engine (mocked IPC).
5. **Security tests are first-class.** Any security test that fails or is BLOCKED blocks the exit gate of its phase. It can be waived only by an explicit, recorded user decision.
6. **Targets are hypotheses.** Performance targets (§8) are revised only with measured data and a written justification.

## 2. Test levels and tooling

| Level | Scope | Tools (PROPOSED; installation needs approval) | Runs on | Automation |
|---|---|---|---|---|
| Static | Types, lint, format, dependencies, boundaries, configuration hardening | `tsc --noEmit`, ESLint, `cargo fmt`, `cargo clippy`, `cargo deny`, `cargo audit`, `npm audit`, boundary script | Windows dev host; macOS dev host for cfg(macos) code | `scripts/check.{ps1,sh}` |
| Unit | Pure logic: state machine, validation, mapping, serialization, authorization, redaction, crypto handshake | `cargo test`, `proptest`, Vitest + Testing Library | Any dev host (platform-specific mapping tests run on every OS because they are pure functions) | Yes |
| Integration | Module boundaries with fakes, or with real implementations on that OS | `cargo test --test …`, `tp-test-harness` headless host/controller, `tp-transport::sim` | Dev host; real-device runs for platform implementations | Yes (mostly) |
| UI | React flows with mocked IPC | Playwright against the Vite dev server with `@tauri-apps/api` mocks (`mockIPC`); WebView2 via CDP if feasible (UNKNOWN) | Any OS (browser); WebView2 on Windows | Yes |
| Real device | Capture, input, rendering and the four directions | Manual scripts plus harness probes; measurement tools (§7) | DEV-W1/W2, DEV-M1/M2 | Semi-manual |
| Security | Consent, gating, malformed and malicious input, MITM, secrets | Crafted peer (`tp-test-harness` "evil client"), MITM proxy harness, `cargo-fuzz`, log scanners | Dev host + devices | Mostly automated |
| Network | LAN variants, impairment, interruption, multi-NIC, WAN | `tp-transport::sim` (no admin); Linux `tc netem` router; macOS Network Link Conditioner; OS-level adapter disable | NET-LAB | Semi-manual |
| Performance | FPS, latency, CPU/memory, drops, startup, soak, cycling | Diagnostics metrics JSON, glass-to-glass camera method, OS counters | Devices + NET-LAB profile | Semi-manual |
| Release | Installer, signing, notarization, SBOM | `signtool`, `codesign`, `spctl`, `stapler`, SBOM tools | Clean VMs or devices | Manual + scripted |

**Notes on E2E tooling:**
- Tauri's WebDriver tooling does not support macOS. Therefore UI logic is covered by Playwright with mocked IPC on every OS, and full end-to-end coverage on macOS is manual and evidence-based.
- Driving WebView2 through Playwright's `connectOverCDP` requires starting the WebView with a remote-debugging argument. Its feasibility is UNKNOWN (UX-08), and it must never be enabled in release builds (T-STA-10).

## 3. Environments and devices

| ID | Device | Status (2026-10-28) | Used for |
|---|---|---|---|
| DEV-W1 | Windows 11 Enterprise LTSC (build 26100) x64, i9-12900K, 127.8 GB RAM, Intel UHD 770 + NVIDIA RTX 3090, **three 1920×1080 displays at 96 DPI with a negative-origin layout** | **AVAILABLE** (this workspace's host) | Windows capture and input, hybrid-GPU and multi-monitor cases, Windows Host or Controller |
| DEV-W2 | Second Windows 10/11 device, ideally a laptop at 150% scaling with Wi-Fi | **NOT AVAILABLE** (unverified) | DIR-3, mixed DPI, Wi-Fi, two-device LAN |
| DEV-M1 | Apple Silicon Mac with a Retina display, macOS at the U-08 minimum and at the latest version | **NOT AVAILABLE** | macOS capture, input, WKWebView, DIR-1/2/4 |
| DEV-M2 | Second Mac (Intel, if U-08 includes Intel) | **NOT AVAILABLE** | DIR-4 |
| NET-LAB | Gigabit switch / router; optional Linux box with `tc netem` | **UNVERIFIED** | Network and performance profiles |

**Direction coverage:**

| Direction | Controller → Host | Devices | Possible today |
|---|---|---|---|
| DIR-1 | macOS → Windows | DEV-M1 → DEV-W1 | No (no Mac) |
| DIR-2 | Windows → macOS | DEV-W1 → DEV-M1 | No (no Mac) |
| DIR-3 | Windows → Windows | DEV-W1 ↔ DEV-W2 | No (one Windows device) |
| DIR-4 | macOS → macOS | DEV-M1 → DEV-M2 | No |

**Single-device stopgap** (does **not** certify any direction): DEV-W1 can run a Host and a Controller as two processes on the loopback or LAN interface, with the mock injector or injection disabled. This catches pipeline bugs early.

## 4. Commands (planned; created in BOOT-01)

| Purpose | Windows (PowerShell) | macOS (zsh/bash) |
|---|---|---|
| All static + unit gates | `./scripts/check.ps1` | `./scripts/check.sh` |
| Rust format | `cargo fmt --all -- --check` | same |
| Rust lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | same |
| Rust tests | `cargo test --workspace --locked` | same |
| Dependency policy | `cargo deny check` · `cargo audit` | same |
| TS typecheck | `npm run typecheck --workspace apps/desktop` | same |
| TS lint | `npm run lint --workspace apps/desktop -- --max-warnings=0` | same |
| TS unit | `npm run test --workspace apps/desktop` (Vitest) | same |
| UI E2E (mocked IPC) | `npm run e2e --workspace apps/desktop` (Playwright) | same |
| npm advisories | `npm audit --omit=dev` | same |
| Boundary check | `./scripts/boundaries.ps1` | `./scripts/boundaries.sh` |
| Headless session | `cargo run -p tp-test-harness --bin tp-headless -- host …` / `-- controller …` | same |
| Fuzz (nightly, approval needed) | `cargo +nightly fuzz run <target> -- -runs=1000000` | same |

## 5. Test data and fixtures

- **Display layouts:** the DEV-W1 layout (VX = −3840, VY = 0, VW = 5760, VH = 1151); mixed DPI (100% + 150%); Retina 2× with a secondary display left of and above the main display; a single display.
- **Key maps:** `keymap.csv` as the single source; golden tables for the Windows scan code and macOS `kVK` columns.
- **Synthetic video:** the *TP test pattern*, rendered by a harness page or app — a millisecond clock, a frame counter as a visual barcode block, scrolling text, a full-screen moving gradient and a static region. **No personal content is ever used in tests.**
- **Pre-encoded H.264 streams:** generated by our encoder backends in the harness, with SHA-256 per access unit.
- **Protocol corpora:** valid messages of every type, mutations, oversized and truncated inputs. Fuzz seeds live in `fuzz/corpus/`.
- **PAKE test vectors:** RFC test vectors where they exist (for example RFC 9382 for SPAKE2), plus our own transcript-binding vectors.

## 6. Fakes and harness

| Fake / tool | Replaces | Purpose |
|---|---|---|
| `SyntheticCapture` | `CaptureProvider` | Deterministic frames with an embedded frame counter and timestamp |
| `MockInput` (recording) | `InputController` | Asserts on exactly what *would* be injected (counts, order, coordinates) |
| `PassthroughEncoder` / `PassthroughDecoder` | Codecs | Pipeline tests without codec cost |
| `LoopbackTransport` / `SimNet` | `Transport` | In-memory peers, with loss, delay, reorder and partition, driven by `FakeClock` (deterministic when the chosen library is sans-IO) |
| `FakeClock` | Time | Timers (consent, TTL, grace) tested in microseconds |
| `MockPermissionManager` | OS permissions | Denied, revoked, relaunch-required paths |
| `tp-headless` | UI | Host/Controller sessions in CI-like runs. Auto-consent exists **only** here, behind the `test-harness` feature |
| `evil-client` | Peer | Crafted, malformed, replayed, out-of-state and flooding messages |
| `mitm-proxy` | Network path | Relays signaling, attempts SDP and fingerprint substitution |

## 7. Measurement methodologies

All measurements record: device IDs, OS build, app build (commit hash once git is initialized), network profile, display mode, power state, other running load, and tool versions.

**Statistics:**
- Warm-up: exclude the first 30 s.
- Report n, mean, p50, p95, p99 and max.
- Run 3 repetitions and report the worst repetition against the target.
- Minimum sample sizes: latency ≥ 300 samples per run; FPS ≥ 300 one-second windows.

| ID | Metric | Method | Evidence |
|---|---|---|---|
| M-01 | **Rendered FPS** (Controller) | Count *presented* frames: canvas draws per `requestAnimationFrame`, or `requestVideoFrameCallback` where applicable, or the native renderer's present count. Received or decoded counts are recorded separately. Frames are not "rendered" until drawn | `metrics.json` (per-second series), summary |
| M-02 | **Capture-to-display latency** | **(a) Glass-to-glass, ground truth:** the Host shows the TP test pattern with its ms clock, and the Controller shows the remote view next to it. A camera at ≥ 120 fps (240 preferred) records both screens; latency = Controller clock − Host clock per frame, read from the video (manually or by OCR), ≥ 300 samples. **(b) Instrumented:** each frame carries `captured_at` (Host monotonic). Clock offset is estimated over `tp-control` heartbeats (NTP-style, offset ± RTT/2 bound reported) and render time is recorded at draw. (a) validates (b); (b) gives continuous data | Camera file (test pattern only) + extracted samples CSV; `metrics.json` with the offset error bound |
| M-03 | **Input latency** | Instrumented: Controller event timestamp → Host injection timestamp (offset-corrected), plus "click-to-photon" by camera on a test page that changes colour on click | CSV + video |
| M-04 | **CPU and memory** | Windows: `Get-Counter` on `\Process(*)\% Processor Time`, `Working Set - Private`, `Handle Count`, `Thread Count`, sampled at 1 s. **Include the WebView processes** (`msedgewebview2.exe`). macOS: `ps`/`top -l` sampling or Instruments; include the `com.apple.WebKit.*` processes. CPU is normalized to the logical core count and reported both ways | Counter CSV |
| M-05 | **Dropped frames by stage** | Diagnostics counters: capture replaced, encoder dropped, network lost (RTCP/NACK), decoder dropped-until-IDR, render skipped | `metrics.json` |
| M-06 | **Session startup time** | Timestamps: Connect click → `Prepared` → consent shown; consent clicked → `Connected` → first frame rendered. **Human consent time is reported separately and excluded** from the system startup time | Log excerpt + `metrics.json` |
| M-07 | **Resource cleanup** | Gauges (ARCH-00 §5.6) and OS counters (handles, threads, RSS) before the first session and after each of N cycles. Pass if the gauges return exactly to baseline, and OS counters stay within tolerance with no monotonic growth | CSV + chart |
| M-08 | **Network impairment** | Profiles applied by SimNet (deterministic), `tc netem` on a Linux router, or macOS Network Link Conditioner. Each run records the profile parameters | Profile config + results |

**LAN test profiles (PROPOSED):**

| Profile | Definition |
|---|---|
| LP-1 (reference) | Both devices wired at 1 GbE on the same switch and subnet; no impairment; Host display 1920×1080 (or Retina scaled) captured and downscaled to 1280×720 at 30 fps; H.264 CB at 4 Mbps initial; TP test pattern content; AC power; no other heavy load |
| LP-2 | LP-1 but on the same 5 GHz Wi-Fi access point (no client isolation) |
| LP-3 | LP-1 + 2% random loss + 20 ms added RTT ± 5 ms jitter |
| LP-4 | LP-1 + bandwidth cap at 2 Mbps |

## 8. Initial engineering targets (hypotheses, not results)

| Target | Value | Measured by | Acceptance rule (PROPOSED) |
|---|---|---|---|
| Video | 720p at 30 FPS under LP-1 | M-01 (T-PRF-01) | Mean rendered FPS ≥ 29.0 **and** ≥ 95% of one-second windows ≥ 27, over 5 min, worst of 3 runs |
| LAN video latency | p95 ≤ 200 ms under LP-1 | M-02 (T-PRF-02) | Glass-to-glass p95 ≤ 200 ms (worst of 3 runs), with the instrumented method consistent within ±20 ms |
| Stability | ≥ 30 min continuous | T-PRF-07 | No crash or hang; FPS target held; RSS growth after warm-up ≤ 10% with no monotonic trend |
| Cycling | 10 connect/disconnect cycles | T-PRF-08 | No crash; gauges return to baseline each cycle |
| Dropped frames | ≤ 1% end-to-end under LP-1 | M-05 (T-PRF-05) | Proposal, to be revised after baseline |
| Startup (system, excluding human consent) | Baseline first; proposed ≤ 5 s under LP-1 | M-06 (T-PRF-06) | Proposal |
| CPU / memory | Baseline first; budgets set after VID-04 | M-04 (T-PRF-04) | No budget in ARCH-00 |
| Input latency | Baseline first | M-03 (T-PRF-03) | No budget in ARCH-00 |

**Revision rule.** A target may be changed only in a phase report section titled "Target revision". It must include the measured baseline, its root-cause analysis, the proposed new value and the user's approval.

## 9. Evidence requirements

- **Location:** `docs/qa/evidence/<PHASE>/<TEST-ID>/<YYYYMMDD-HHMM>-<device>/`.
- **Contents:**
  - `env.txt`: device, OS build, tool versions, commit;
  - `command.txt`: exact command or steps;
  - `output.log`: raw output, unedited;
  - `metrics.json` and/or CSV;
  - media (screenshots or videos **of test patterns only**);
  - `verdict.md`: result, criteria applied, deviations.
- **Large media:** files over 20 MB are stored outside the repository, with a SHA-256 checksum and location recorded in `verdict.md`.
- **Privacy:** evidence must not include real personal screen content, codes, keys or credentials. The redaction scan (T-SEC-14 scanner) runs on evidence folders too.
- **Results ledger:** `docs/qa/results/<PHASE>-results.md`, with columns Test ID | Date | Device(s) | Build | Result | Evidence path | Notes.

**Result statuses:**

| Status | Meaning |
|---|---|
| PASS | Executed; expected result met; evidence stored |
| FAIL | Executed; expected result not met. The defect is logged |
| BLOCKED | Could not execute because a precondition was missing (device, permission, tool, dependency). The blocker is named |
| NOT RUN | Not attempted in this phase |
| WAIVED | Not required for the phase gate, by explicit user decision (recorded, with a reason) |

## 10. Entry and exit criteria

- **Entry, for any phase:** the previous phase's report is approved by the user; the phase's prerequisites (PROMPT-PLAN) are met or explicitly waived.
- **Exit:** every test listed in the phase's exit gate is PASS, or BLOCKED/WAIVED with a recorded user decision. No open S1 defect. No unauthorized scope changes.
- **Phase verdict:**

| Verdict | Condition |
|---|---|
| PASS | All gate tests PASS |
| PARTIAL | All executable tests PASS; some BLOCKED, each with a named blocker and the user's waiver or carry-forward |
| FAIL | Any gate test FAIL, or any S1 defect open |
| BLOCKED | The phase cannot be meaningfully executed |

## 11. Defect severity

| Severity | Definition | Gate impact |
|---|---|---|
| S1 | Security or consent violation; input or stream without authorization; data leak; crash in a core flow | Blocks exit; no waiver without the user's written acceptance |
| S2 | A main flow is broken on one platform or direction; a target is missed by more than 50% | Blocks exit unless waived |
| S3 | Degraded behaviour with a workaround | Tracked |
| S4 | Cosmetic | Tracked |

## 12. Coverage map

| Area | Tests |
|---|---|
| Session state machine | T-UNI-01 … T-UNI-06, T-INT-07 … T-INT-09, T-INT-11 |
| Input validation and gating | T-UNI-07 … T-UNI-09, T-SEC-05, T-SEC-07, T-SEC-08, T-SEC-10 |
| Coordinates and DPI | T-UNI-10 … T-UNI-12, T-DEV-07, T-DEV-09 |
| Keyboard | T-UNI-13, T-DEV-08, T-DEV-09, T-DEV-15, T-DEV-16 |
| Serialization and protocol | T-UNI-14 … T-UNI-16, T-SEC-09, T-SEC-10 |
| Permissions and authorization | T-UNI-17, T-SEC-01, T-SEC-02 |
| Capture | T-INT-01, T-INT-02, T-DEV-01 … T-DEV-06 |
| Codec | T-INT-03, T-INT-04 |
| Transport and control | T-INT-05, T-INT-06, T-NWK-01 … T-NWK-12 |
| Four directions | T-DEV-11 … T-DEV-14 |
| Security requirements SR-01 … SR-14 | THREAT-MODEL §5 table |
| Performance targets | T-PRF-01 … T-PRF-10 |
| UI | T-UNI-21, T-UI-01 … T-UI-05, T-INT-10 |
| Release | T-REL-01 … T-REL-07 |

## 13. What the current environment cannot test (as of ARCH-00)

| Item | Reason |
|---|---|
| Any Rust build or test | No Rust toolchain installed (R-01) |
| All macOS tests (capture, input, WKWebView, DIR-1/2/4) | No Mac (R-02) |
| DIR-3 and two-device LAN tests | No second Windows device (R-03) |
| Impairment with OS tools on Windows | Would need driver-based tools and installation (R-13); SimNet covers the deterministic cases |
| Release signing | No certificates or Apple account confirmed (R-09) |
| Glass-to-glass latency | Needs a ≥ 120 fps camera and two devices |

## 14. Additions from BOOT-02

1. **Exhaustive state exploration instead of property-test libraries.** For pure state machines, every reachable configuration is explored and invariants are checked on every edge (`crates/tp-session/tests/exploration.rs`; 4,728 configurations and 148,157 edges in BOOT-02). An independently written transition spec (`tests/common/mod.rs::spec_allows`) is compared against the implementation. No new dependency is needed.
2. **Negative controls are standard practice for guards.** Every structural guard (boundaries, inject call sites, absence of production implementations, test-only features, drift checks) must be shown to fail on a deliberate violation once. The run must restore files byte-for-byte (hash-checked) and record the result under `docs/qa/evidence/<PHASE>/negative-controls/`.
3. **Mocks prove logic, not enforcement.** A test using a mock controller, platform or clock is reported as "mock level". Real enforcement is verified only in the phase that adds the real adapter.
4. **Synthetic secrets.** Logging tests use recognisable synthetic values (e.g. `Hunter2-SYNTHETIC`, distinctive coordinates and key codes) and assert they never appear in rendered output.
5. **Alias IDs.** Prompt-specific IDs (e.g. T-CORE-xx) are mapped to canonical TEST-MATRIX IDs (§13 of the matrix) and counted once.
6. **Time.** Engine tests never sleep. Every time-dependent decision takes `now: MonoTime`. Only the Windows smoke scripts wait, and they wait on observable conditions with time limits.

## 15. Additions from CAP-02A

1. **Real-device tests are `#[ignore]` and run explicitly.**
   - Tests that open windows or capture (`tp-test-harness/tests/capture_windows.rs`, `preview_windows.rs`) never run in routine `cargo test`.
   - Real-device QA runs them with `--ignored --test-threads=1` and stores the output under `docs/qa/evidence/<PHASE>/provider/`.
2. **Prove real frames with known content.**
   - Capture tests target a synthetic test-pattern window with fixed colours.
   - They read pixels back and compare them with the pattern, which proves the frames came from the OS and were not fabricated.
   - No test captures private content into a file.
3. **Monitor capture only for metadata.** Tests that capture whole monitors (approved per phase) check only size, timing and target mapping. They never read pixels back.
4. **Measurements come from observations.**
   - Frame rates come from OS frame timestamps (bench) or observed arrivals (host), and process metrics are sampled from outside the process (PowerShell).
   - A run whose source stopped changing is not a performance result. The bench logs the source's own repaint counter (`pattern_paints`), so a stalled source can be told apart from a slow pipeline.
5. **Host-level substitutes are labelled as such.**
   - When an app-level manual step cannot be completed, the production host may be driven over real frames with a test-only picker stand-in (`WindowPickerForTests`).
   - Such results are reported as "host level". They never stand in for the OS picker UI or the WebView rendering.
6. **Operator reports are corroborated.** A manual step passes only if the app's own evidence (log events, process state, screenshots of TP Remote's window) agrees with the operator's report.
7. **Diagnostics are temporary.**
   - Temporary diagnostic tests are removed after use. Their output is kept as evidence with a header naming the removed file (for example `perf/diagnostic-handles-per-step.log`).
