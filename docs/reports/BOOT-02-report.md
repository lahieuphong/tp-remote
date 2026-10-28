# BOOT-02 Report — Core Contracts, Session State Machine, Security Gates & Regression QA

| Field | Value |
|---|---|
| Phase | BOOT-02 |
| Date | 2026-10-28 |
| Host | DEV-W1: Windows 11 Enterprise LTSC 2024, build 26100, x64 |
| Baseline | git `7a9ae99` (BOOT-01), clean tree |
| Verdict | **PASS**: Windows-only, engine-level certification (§17) |
| Next phase | CAP-02. **Not started**; waiting for reviewer approval |

Sections are tagged **[Observed]** for things executed and recorded in this phase, and **[Recommendation]** / **[Assumption]** otherwise. Details: [test results](../qa/BOOT-02-test-results.md) · [security review](../security/BOOT-02-security-review.md) · [contract map](../architecture/BOOT-02-contract-map.md) · [ADR-003](../architecture/ADR-003-session-lifecycle.md) · [ADR-004](../architecture/ADR-004-platform-support.md).

---

## 1. Executive summary [Observed]

BOOT-02 added a small, tested core that future remote features cannot bypass by accident. It has no remote capability itself:
- **Contracts:** the eight ARCH-00 §4 contracts, as interfaces with no production implementations.
- **Shared primitives:** identifiers, monotonic time, secrets, a 13-category error model, typed capabilities, cancellation.
- **Session lifecycle:** a deterministic 8-state machine with timers, ordered cleanup effects and stale-event rejection.
- **Authorization:** a default-deny policy in which authentication and local consent are separate, unforgeable proofs.
- **Gates:** an input gate that re-authorizes every event.
- **Logging:** allowlisted logging with secret redaction.

**Tests:** 141 Rust tests, of which 90 are new, and 18 Vitest tests, all passing. `npm run check` passes 9 of 9.

**Evidence the guards work:** four negative controls showed that the structural guards fail when deliberately violated. Testing also found and fixed one real defect: the redaction scrubber leaked `Bearer` tokens.

**Windows runtime:** behaviour is unchanged. The app now shows phase BOOT-02 and still advertises no remote feature.

**Change footprint:** no dependency was added or upgraded, and no IPC command, permission or UI file changed.

## 2. Baseline verification [Observed]

| Item | Found |
|---|---|
| Crates | 13 workspace members (12 ARCH-00 crates + `tp-desktop`), as in BOOT-01 |
| Dependencies | As in BOOT-01; `Cargo.lock` / `package-lock.json` unchanged through BOOT-02 |
| IPC | One command `get_app_info`; capability `main-window` → `allow-get-app-info` only |
| Tests | Baseline `npm run check`: 9 of 9 PASS, 51 Rust + 18 Vitest (`evidence/BOOT-02/baseline/`) |
| Runtime capabilities | None (remote features notImplemented) |
| Uncommitted changes | None (clean tree at `7a9ae99`) |
| Git | **A repository exists** (see §13). BOOT-01's "not a git repository" statement was wrong; errata added |
| Docs | All ARCH-00 and BOOT-01 documents present; ARCH-00 set unchanged since ARCH-00 except BOOT-01's append-only notes |

**Reconciliations with the prompt:**
1. The prompt asks for an ADR amendment for lifecycle inconsistencies. ARCH-00 §6 is not an ADR, so the amendment is a new **ADR-003**.
2. U-08 is recorded as **ADR-004**.
3. The roadmap's future ADR numbers shift: NET-03 → ADR-005, WAN-07 → ADR-006 (PROMPT-PLAN amendment).
4. Test IDs from the prompt are aliases of canonical TEST-MATRIX IDs (matrix §13).

## 3. Files created and modified [Observed]

**Created (code):**
- `tp-contracts/src/{cancel,capability,capture,codec,error,id,identity,input,permission,secret,session,time,transport}.rs`
- `tp-session/src/{machine,registry,shared}.rs`
- `tp-security/src/{authn,consent,policy,tests}.rs`
- `tp-core/src/{clock,gate,service}.rs`
- `tp-diagnostics/src/tests.rs`

**Created (tests):**
- `tp-contracts/tests/{primitives,contracts_implementable}.rs`
- `tp-session/tests/{common/mod,machine,exploration,registry}.rs`
- `tp-core/tests/{service,gate_and_logging}.rs`
- `tp-protocol/tests/reserved.rs`

**Modified (code):**
- `tp-contracts/src/lib.rs`, `tp-core/{Cargo.toml,src/lib.rs}`, `tp-diagnostics/src/lib.rs`
- `tp-protocol/src/error.rs` (`From<ProtocolError> for CoreError`)
- `tp-security/{Cargo.toml,src/lib.rs}`, `tp-session/{Cargo.toml,src/lib.rs}`
- `tp-test-harness/tests/workspace_boundaries.rs` (+3 tests)
- `apps/desktop/src-tauri/src/lib.rs` (log field `code` → `error_code`)
- `apps/desktop/src-tauri/tests/ipc.rs` (phase assertion)

**Created (docs):** this report; `qa/BOOT-02-test-results.md`; `security/BOOT-02-security-review.md`; `architecture/BOOT-02-contract-map.md`; `architecture/ADR-003-session-lifecycle.md`; `architecture/ADR-004-platform-support.md`; `qa/evidence/BOOT-02/**`.

**Modified (docs, append-only):** ARCH-00 (§2.2 status note), THREAT-MODEL (§11), TEST-STRATEGY (§14), TEST-MATRIX (§12–13), PROMPT-PLAN (BOOT-02 section, graph, ADR renumbering), README, BOOT-01-environment (§7 erratum), BOOT-01-report (erratum).

**Not modified:** any UI source, generated binding, capability file, `tauri.conf.json`, `build.rs`, lockfile or package manifest.

## 4. Session state-machine design [Observed]

- **Machine.** `tp_session::SessionMachine` is single-use. It holds role, requested and granted scope, link state, absolute deadlines, end reason, escalation and outcome.
  - `apply(event, now)` returns a `Transition` (from, to, cause, **ordered effects**) or a typed `TransitionError` (`NotAllowedInState`, `SessionEnded`, `ConsentOriginNotPermitted`, `ScopeExceedsRequest`, `AlreadyStopping`). A rejection never mutates the machine.
  - `tick(now)` fires deadlines. There are no external timers and no sleeps.
- **Lifecycle.** All eight states are implemented as specified. Every exit from an active state goes through `Disconnecting`, which emits the ordered teardown, input gate first.
  - `Disconnected` or `Failed` is chosen from the recorded outcome. A fault during teardown, or the 5 s watchdog, forces `Failed` and records an `Escalation`.
- **Registry.** `SessionRegistry` allows one active session per device. A `SessionRef` (id and strictly increasing generation) is required on every event; stale references get `StaleSession`. Stops are idempotent, and a stale stop never touches a newer session. A bounded, payload-free audit trail is kept.
- **Concurrency.** `SharedRegistry` uses a mutex, so operations are totally ordered; a poisoned lock fails closed.
- **ADR-003 clarifications:**
  - the `Busy` contradiction (row 4 vs. the outcome classes): now `PeerBye(Busy)` → Normal;
  - stale-event rules;
  - the Host offer expiry is a normal end;
  - stop or Bye during teardown is `AlreadyStopping`;
  - a remote consent decision on the Host becomes a protocol-violation fault;
  - `SessionManager` is synchronous.
- **Scenarios A–K:** all are covered by tests (test results §2).

## 5. Authorization and consent model [Observed]

- **Proof types.** `AuthenticatedPeer` (who is connected) and `LocalConsent` (what the Host user decided) are separate types with private fields. Neither has a production constructor; `for_tests` exists behind `test-support` only, and the boundary test proves the app build never enables it.
- **Grants.** `ConsentGrant::from_local` needs both proofs for the same session reference, can be revoked one-way, expires, and has **no way to widen its scope**.
- **Policy.** `tp_security::authorize` is pure and default-deny, with 8 ordered conditions. `tp_core::SessionService` is the only place where lifecycle state and the policy meet. `disconnect` revokes the grant *before* stopping.
- **Platform.** The production platform advertises `NONE`, so even a fully consented session is denied every capability.
- **Trusted-consent boundary** for the future surface (security review §3): local, out of reach of the WebView and of injected input, bound to one session, fail-closed. It is **FUTURE (NET-03 / U-06)**. No fake approval mechanism exists.

## 6. Core interface contracts [Observed]

| Contract | Status |
|---|---|
| `CaptureProvider` / `CaptureStream` / `FrameSink` | Interface only |
| `InputController` | Interface only |
| `VideoEncoder` / `VideoDecoder` | Interface only |
| `Transport` / `TransportSession` / `SignalingLink` | Interface only |
| `PermissionManager` | Interface only |
| `DeviceIdentity` | Interface only |
| `SessionManager` | **Implemented** by `SessionService` |

- **Design:** associated types and generics, no trait objects. Async methods return `impl Future + Send` with no executor dependency. Each trait documents ownership, cleanup and cancellation (`CancelToken`).
- **Honesty:** test-only "not available" doubles prove every contract is implementable (T-UNI-27). A boundary test fails if a production implementation of a platform trait appears.
- **`unsafe`:** none was introduced. The six designated crates (tp-contracts, tp-protocol, tp-session, tp-signaling, tp-diagnostics, tp-core) keep `#![forbid(unsafe_code)]`, verified by the boundary test. `tp-security` keeps the workspace-wide `deny`.

## 7. Protocol and TypeScript compatibility [Observed]

- **U-05 preserved.** v1 JSON is unchanged. The 24 fixture cases, golden bytes and 11 codec tests pass.
- **New protocol checks.** All 20 reserved types are rejected (`NotAuthorized`) on both channels and have no `MessageKind`. `ProtocolError` maps to `CoreError` without peer text.
- **Identifiers.** Exactly 32 lowercase hex characters; all-zero is rejected, and serde rejects invalid input. They are not yet in the wire envelope: that comes in NET-03 per ADR-002 A1.
- **U-13 preserved.** No IPC DTO changed. Binding drift check PASS, 11 files unchanged, 18 Vitest tests unchanged and passing. New engine types are deliberately **not** exported to TypeScript (IPC and protocol stay separate).

## 8. Logging / redaction implementation [Observed]

- **Typed events.** `tp_diagnostics::Event` with typed `FieldValue`s: `Static`, `Text` (cleaned), `U64`, `Bool`, and `Session` (logged as tag and generation only).
- **Allowlist.** `ALLOWED_KEYS` has 20 names. Any other key is written as `[REDACTED]`.
- **Scrubber (safety net).** `FieldValue::text` and `format_event` replace control characters, scrub secrets and cap length at 160 characters. Recognised patterns: `word=` / `word:` / JSON keys for 21 sensitive words, `Bearer`/`Basic`, PEM private keys, and a bare sensitive word followed by a value.
- **Sinks.** `Logger` writes to `StderrSink` or `MemorySink`. No files, no telemetry.
- **Secrets.** `Secret<T>` has no `Display`, `Serialize` or `Clone` (proved by compile-fail doctests) and a redacted `Debug`; wiping on drop is best effort. Payload types (`InjectableEvent`, `KeyUsage`, `EncodedFrame`, `TransportEvent::ControlMessage`, keys, signatures) have redacting `Debug`.
- **Engine events.** `tp-core` logs `session.transition`, `session.rejected` and `authz.denied` (with `event_kind`, never data). A test confirms they use allowlisted fields only.
- **Mandatory rules for later phases:** security review §4.
- **Defect F-1 (found by a test, fixed):** `Authorization: Bearer <token>` leaked the token.

## 9. Automated test results and counts [Observed]

| Suite | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace` | PASS |
| `cargo test --workspace` | **141 passed, 0 failed, 0 ignored**: 137 test functions + 4 doctests |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS. One new 1.99 lint (`chunks_exact_to_as_chunks`) was fixed first |
| `npm run check` | 9 of 9 PASS (Vitest **18 passed**) |

The BOOT-01 tests: **51 still pass** — 45 unchanged, 3 renamed with updated expectations, 3 updated for the phase or allowlist (test results §1.2). **90 new** tests. The per-target table is in test results §1.1.

**Exhaustive exploration** (T-UNI-04): **4,728** reachable configurations and **148,157** edges, every invariant holding. **Transition spec:** 1,116 (state, event) pairs checked against an independent spec.

## 10. Security test results [Observed]

- **T-SEC-CORE-01 … 08:** all PASS through 13 policy tests plus service-level and gate-level tests.
- **T-LOG-01 … 04:** all PASS (one defect fixed along the way).
- **T-CONTRACT-05 / 06:** PASS.
- **Mock-level gate test (T-SEC-08):** **0 injections** before consent, after revocation, with view-only scope, with a degraded link, or on the production platform.

**Negative controls** — each guard failed on its deliberate violation, then passed again after a byte-exact restore:

| ID | Violation | Guard that failed |
|---|---|---|
| NC-1 | Lifecycle accepts `TransportReady` before consent | Exploration and transition-table tests |
| NC-2 | `inject` called outside the gate | `only_the_input_gate_calls_inject` |
| NC-3 | A production `PermissionManager` implementation | `no_production_platform_implementation_exists_yet` |
| NC-4 | `test-support` forced into the app graph | `the_app_build_graph_has_no_test_support` |

**Limit:** these tests prove policy and gate *logic*. Real-world OS or network enforcement is **UNVERIFIED** until the adapters exist (security review §7).

## 11. Windows runtime regression results [Observed]

- **WIN-BOOT-01 … 08** re-run with the BOOT-01 harness. All PASS, with the same scope limits: 100% scaling only, and WIN-BOOT-04 on the mock ACL runtime.
- **Builds:** release `tp-desktop.exe` 8,697,344 B (SHA-256 `4DE228A9…3F76`), debug 12,428,800 B.
- **Behaviour:** IPC ok after 0.43–0.45 s; Medium integrity; PerMonitorV2; **0 TCP / 0 UDP** endpoints; clean exit with no survivors; the release build ignores fault injection.
- **Dev launch:** Ready in 12.5 s; Vite on 127.0.0.1:1420 only; the whole tree exits.
- **Screenshots:** the UI shows **BOOT-02**, tp-session and tp-security *partial*, the capability crates *foundation*, and "Remote features not implemented".
- **Crashes:** `STATUS_ENTRYPOINT_NOT_FOUND` did not recur. Only `tp-desktop` links Tauri, and its manifest fix is unchanged.

## 12. Dependency and licensing changes [Observed]

- **Dependencies:** none added, removed or upgraded. `Cargo.lock`, `package-lock.json` and every `package.json` are unchanged (git diff). The new internal edges (`tp-core` dev-dependencies with `test-support`) are workspace path crates.
- **Licensing:** no change to the BOOT-01 inventory. The open items remain: option-ext MPL-2.0 linked via Tauri, and the legal owner is unconfirmed.
- **Not installed:** cargo-audit, cargo-deny, proptest, zeroize, Playwright browsers.

## 13. Git initialization status [Observed]

- **Location:** the workspace **is** a git repository (`E:/Phong_Nho_IT/tp-remote/.git`) on branch `main`, which tracks `origin/main` at `7a9ae99` (`https://github.com/lahieuphong/tp-remote.git`).
- **History:** commits `3166183` (11:16:57) and `7a9ae99` (13:11:00), author `lahieuphong`, made outside the assistant's actions.
- **No action taken:** no `git init` was needed. In BOOT-02 the assistant created **no commit**, no branch and no remote, and pushed nothing.
- **Hygiene:** ignored paths are build outputs only (`target/`, `node_modules/`, `dist/`, `src-tauri/gen/`). The BOOT-02 changes and 34+ new evidence files are uncommitted and await your review.
- **Correction:** BOOT-01's "not a git repository" statement was wrong (unverified carry-over). Errata were added to the BOOT-01 report and environment documents.

## 14. U-08 provisional compatibility decision [Observed: recorded; Assumption: product tiers]

ADR-004, **PROVISIONAL**:

| Tier | Windows | macOS |
|---|---|---|
| Primary | Windows 11 x64 24H2+ | macOS 15+ (Apple Silicon) |
| Legacy candidates | Windows 10 22H2 (standard support ended 2025-10-14) | macOS 13–14 |

- **API minimums** checked against vendor metadata: ScreenCaptureKit macOS 12.3; `SCContentSharingPicker` macOS 14.0; WGC `CreateForMonitor` Windows 10 1903 (build 18362). WebCodecs in WKWebView and Tauri's macOS minimum are UNKNOWN / UNVERIFIED.
- **Security-support rule:** no shipping for an OS without vendor security updates, unless you decide explicitly.
- **Certified:** only Windows 11 build 26100 (DEV-W1), engine level.

## 15. Remaining technical risks

| Risk | Note |
|---|---|
| No Mac / no second Windows device | Blocks macOS halves of CAP-02 and all two-device work (R-02, R-03) |
| Enforcement not yet proven | Policy and gate are mock-level until CAP-02 / CTRL-05 / NET-03 adapters exist and are tested |
| Trusted consent surface (U-06) | FUTURE; must satisfy security review §3 |
| Best-effort `Secret` wipe; pattern-based scrubber | F-2, F-3: accepted for now |
| Production session-id CSPRNG | FUTURE (NET-03); only deterministic test sources exist |
| `test-support` unification in workspace test builds | Test binaries only; app graph guarded (F-5) |
| Live-WebView ACL test, Tauri `dynamic-acl` default | Open from BOOT-01 (F-6, F-7) |
| Timers depend on a driver calling `tick` | Documented in ADR-003; the driver comes in NET-03 |

## 16. Commands executed and actual outputs [Observed]

| Group | Commands | Actual result |
|---|---|---|
| Baseline | `git rev-parse`, `git log`, `git status`, `git branch -vv`, `git ls-files`; doc hashes; `cargo metadata --no-deps`; `npm run check` | Repository found (§13); clean tree; 13 members; 9 of 9 PASS (51 / 18) |
| Research (read-only) | WebFetch: Apple `screencapturekit.json`, `sccontentsharingpicker.json`; Microsoft Learn `CreateForMonitor` | macOS 12.3 / 14.0; Windows 10 1903 (18362) |
| Development checks | `cargo check -p …`, `cargo test -p tp-security / tp-session / tp-diagnostics / tp-core / tp-protocol / tp-contracts` (iterative) | Failures and fixes: scrubber Bearer leak (F-1); test expectation for `é` (test bug); then all pass |
| Formatting / lint | `cargo fmt --all` (applied to 25 new files), `cargo clippy … -D warnings` | 1 lint fixed, then 0 |
| Negative controls | NC-1/1b, NC-2, NC-3, NC-4 (scripted, hash-verified restore) | All guards failed as designed; all restored |
| Gates | `cargo fmt --all -- --check`; `cargo check --workspace`; `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `npm run check` | All exit 0; 141 / 18 |
| Builds | `npm run build:app`, `npm run build:app:debug` | Exit 0 (30 s / 12 s) |
| Runtime | `scripts/smoke-windows.ps1` ×4, `scripts/smoke-dev-windows.ps1` | All PASS (§11) |
| Footprint | `git status --porcelain`, `git diff --stat`, `git diff --quiet` per protected file, `git status --ignored` | 12 modified, 33 new source files; protected files unchanged |

Raw logs: `docs/qa/evidence/BOOT-02/{baseline,gates,tauri,windows,negative-controls}/`.

## 17. Verdict

**PASS** for BOOT-02 (Windows-only, engine-level). Against the exit gate:

| Gate | Result |
|---|---|
| A. Core | Deterministic machine; invalid transitions rejected; cleanup, timeouts and cancellation covered; contracts compile; boundaries preserved |
| B. Security | Default-deny; authentication ≠ consent; revocation and stale-event rejection tested; redaction tests passed; no new IPC; no remote capability exposed |
| C. Regression | 51 BOOT-01 tests pass (6 updated with stated reasons); 18 Vitest pass; 90 new pass; typecheck, lint and build pass; Windows app runs |
| D. Scope | No capture, input injection, WebRTC, signaling, service, deployment or dependency upgrade |
| E. Evidence | Real outputs stored; all failures documented; Windows-only certification stated |

**Caveat:** mock-level tests prove logic, not OS or network enforcement. macOS is untested.

## 18. Readiness for CAP-02

**[Observed]** CAP-02 can build on:
- the `CaptureProvider` / `CaptureStream` / `FrameSink` and `PermissionManager` contracts;
- the `ViewScreen` policy path;
- `StartCapture` / `StopCapture` effects in the right lifecycle positions;
- the boundary guards.

**[Recommendation]** Before starting CAP-02:
1. Approve BOOT-02, and decide whether to commit it (no commit has been made).
2. Make a macOS 15+ Apple Silicon Mac available, or accept a PARTIAL CAP-02.
3. Approve the capture binding dependencies once CAP-02 proposes them (`windows`, `objc2-screen-capture-kit`), after their license and compatibility review.
4. Confirm U-08 or keep it provisional.

### Engineering handoff: what Windows and macOS capture implementations must satisfy

1. Implement `tp_contracts::capture::CaptureProvider` in `tp-capture` (`windows/`, `macos/`, cfg-gated), with `type Frame` = the platform frame wrapper (D3D11 texture / IOSurface-backed `CVPixelBuffer`) and `type Stream: CaptureStream`. Confine `unsafe` there and wrap FFI callbacks in `catch_unwind`.
2. Report `CaptureCapabilities.availability` honestly: `Unavailable(PermissionDenied)` when Screen Recording is denied, `NotImplemented` where there is no backend. Never return a dummy stream.
3. `displays()` returns a `DisplayLayout` with an incrementing `epoch`, geometry in OS coordinates (physical px on Windows PMv2; points on macOS, with `scale_milli`), and sanitized names.
4. `start(config, sink)` pushes `FrameMeta` + frame from the provider thread to a non-blocking `FrameSink`, and reports `CaptureEvent::{LayoutChanged, PermissionRevoked, Stopped, Failed}`.
5. `CaptureStream::stop(self)` is bounded; `Drop` stops capture as a last resort. Teardown order is driven by the `StopCapture` effect (ADR-003 §3).
6. Never log or persist frame contents. Only `FrameMeta` may be logged, via allowlisted fields.
7. In the app, capture starts only after `SessionService::authorize(session, Capability::ViewScreen, now)` allows it. On Windows, keep the WGC yellow border (do not request borderless).
8. When the first provider lands, deliberately update `tp_core::platform_capabilities()` and the boundary test `no_production_platform_implementation_exists_yet`, and add real-device tests T-DEV-01 … 06.
9. macOS `PermissionManager`: re-query each call, handle "requires relaunch", and never automate System Settings.

**STOP.** CAP-02 has not been started. No commit, push, deployment, service installation or publication was performed.
