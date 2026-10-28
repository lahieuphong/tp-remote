# TP Remote — Test Matrix

| Field | Value |
|---|---|
| Status | Draft (ARCH-00). **Every test below is NOT RUN**, because no application code exists yet. |
| Date | 2026-10-28 |
| Related | [Test strategy](TEST-STRATEGY.md) (methods M-01 … M-08, profiles LP-1 … LP-4, evidence rules, statuses) · [ARCH-00](../architecture/ARCH-00.md) · [Threat model](../security/THREAT-MODEL.md) |

**Devices** (TEST-STRATEGY §3):
- **W1** = DEV-W1 (available).
- **W2** = DEV-W2, **M1** = DEV-M1, **M2** = DEV-M2 (none available).
- **ANY** = any dev host with the toolchain.
- **BOTH** = must run on a Windows host *and* a macOS host.

**Criteria columns:**
- **P** (PASS): the expected result is met *and* the listed evidence is stored.
- **F** (FAIL): any deviation from the expected result.
- **B** (BLOCKED): the named precondition is missing. BLOCKED never counts as PASS.

**Evidence** is stored under `docs/qa/evidence/<PHASE>/<TEST-ID>/…` (TEST-STRATEGY §9).

---

## 1. Static (T-STA)

| ID | Test | Phase | Preconditions | Device / OS | Steps / command | Expected result | Evidence | P / F / B |
|---|---|---|---|---|---|---|---|---|
| T-STA-01 | TypeScript typecheck | BOOT-01+ | npm deps installed | ANY | `npm run typecheck --workspace apps/desktop` | Exit 0, 0 errors | output.log | P: exit 0 · F: any error · B: Node or deps missing |
| T-STA-02 | ESLint, including security rules (`react/no-danger`; `@tauri-apps/api` imported only from `src/core-api/`) | BOOT-01+ | ESLint config committed | ANY | `npm run lint --workspace apps/desktop -- --max-warnings=0` | 0 errors, 0 warnings | output.log | P: exit 0 · F: any finding · B: deps missing |
| T-STA-03 | Rust format | BOOT-01+ | Rust toolchain | ANY | `cargo fmt --all -- --check` | Exit 0 | output.log | P: exit 0 · F: diff reported · B: **no Rust toolchain (R-01)** |
| T-STA-04 | Clippy, deny warnings | BOOT-01+ | Rust toolchain on each OS | BOTH | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Exit 0 on Windows **and** macOS (cfg-gated code) | output.log per OS | P: both exit 0 · F: any lint · B: OS host missing. A Windows-only result does **not** pass macOS code |
| T-STA-05 | Advisory audit | BOOT-01+ | `cargo-audit` or `cargo-deny` installed (approval) | ANY | `cargo audit` (or `cargo deny check advisories`); `npm audit --omit=dev` | No unwaived High/Critical advisories | output.log + waiver list | P: clean or waived with reason · F: unwaived High/Critical · B: tool not approved |
| T-STA-06 | License, source and ban policy | BOOT-01+ | `deny.toml` (policy after U-15) | ANY | `cargo deny check licenses sources bans`; npm license report (tool chosen in BOOT-01) | No disallowed licenses (e.g. AGPL/GPL in shipped code); no unreviewed git or unknown sources | output.log + license summary | P: clean · F: violation · B: tool not approved |
| T-STA-07 | Dependency compatibility audit | BOOT-01+, every phase that adds deps | Lockfiles present | BOTH | `cargo check --workspace --locked`; `cargo tree -d` (reviewed); `npm ls --all`; `npx tauri info` | Builds locked on both OSes; duplicate versions justified; no missing or invalid peers; `tauri` crate, CLI and `@tauri-apps/api` minor versions aligned | output.log + versions table in phase report | P: all clean and recorded · F: mismatch or unresolvable · B: OS host missing |
| T-STA-08 | Module boundaries and `inject` call sites | BOOT-01+ | `scripts/boundaries.*` | ANY | `./scripts/boundaries.ps1` (or `.sh`) | No forbidden crate edges (ARCH-00 §3.3); `inject(` called only from `tp-core::input_gate` (plus `tp-input` implementations and tests) | output.log | P: zero violations · F: any · B: script missing |
| T-STA-09 | `unsafe` policy | BOOT-01+ | — | ANY | Script: check for `#![forbid(unsafe_code)]` in listed crates; list `unsafe` occurrences | `forbid` present in tp-contracts, -protocol, -session, -signaling, -diagnostics, -core; `unsafe` only in platform modules | output.log | P: compliant · F: violation |
| T-STA-10 | Release hardening configuration | BOOT-01+, REL-09 | Release build config | ANY | Script checks: CSP present with no remote or inline script; capabilities limited to TP Remote commands and the main window; DevTools disabled in release; `cargo tree -e features --release` has no `test-harness` feature or `tp-test-harness`; no WebView remote-debugging args | All checks true | output.log | P: all true · F: any false |

## 2. Unit (T-UNI)

| ID | Test | Phase | Preconditions | Device / OS | Steps / command | Expected result | Evidence | P / F / B |
|---|---|---|---|---|---|---|---|---|
| T-UNI-01 | State machine: valid transitions | BOOT-01 | tp-session | ANY | `cargo test -p tp-session transitions_valid` | Every row 1–21 of ARCH-00 §6.3 gives the listed state and actions | test log | P: all rows · F: any row · B: no toolchain |
| T-UNI-02 | State machine: invalid transitions rejected | BOOT-01 | — | ANY | `cargo test -p tp-session transitions_invalid` (exhaustive states × events × origins) | Every unlisted pair → `TransitionError`; state unchanged; rejection counter incremented | test log | P: exhaustive pass · F: any mutation or acceptance |
| T-UNI-03 | Terminal paths and outcome classes | BOOT-01 | — | ANY | `cargo test -p tp-session outcomes` | Every exit passes through `Disconnecting`; terminal states accept no events; Normal → `Disconnected`, Error → `Failed` (ARCH-00 §6.3) | test log | P / F |
| T-UNI-04 | Gate invariants (property test) | BOOT-01 | proptest | ANY | `cargo test -p tp-session gates_prop` (≥ 10,000 sequences) | Never `Connected` without a prior host-local `ConsentGranted` and `TransportReady`; input gate open ⇔ the §6.6 conditions hold; media gate respected | test log + seed | P: no counterexample · F: counterexample found |
| T-UNI-05 | Consent origin guard | BOOT-01 | — | ANY | `cargo test -p tp-session consent_origin` | Host: a remote-origin consent is rejected and raises `Fault{Protocol}`. Controller: a local-origin consent is rejected | test log | P / F |
| T-UNI-06 | Timers (FakeClock) | BOOT-01 | FakeClock | ANY | `cargo test -p tp-session timers` | Code TTL 10 min and 3-failure invalidation; consent 60 s auto-deny; connecting 20 s; degraded 3 s; grace 15 s; TTL 60 min with warning at T−5 min; extend local-only; watchdog 5 s. Each fires at its exact simulated time | test log | P / F |
| T-UNI-07 | Input validation rules | BOOT-01 (types), CTRL-05 (complete) | tp-protocol | ANY | `cargo test -p tp-protocol validate_input` | Valid events → `ValidatedInputEvent`. Each invalid case returns its specific error code: unknown display, stale epoch, unknown button, unknown HID usage, wheel out of bounds, batch > 64, `seq` ≤ last, wrong session, scope ViewOnly | test log | P / F |
| T-UNI-08 | Rate limiter and abuse threshold | CTRL-05 | FakeClock | ANY | `cargo test -p tp-protocol rate_limits` | Limits per ARCH-00 §7.6 enforced; key-ups never dropped; > 20 invalid messages in 10 s → `ProtocolAbuse` | test log | P / F |
| T-UNI-09 | Pressed-state tracking and `release_all` | CTRL-05 | MockInput | ANY | `cargo test -p tp-core release_all` | `release_all` emits ups for exactly the pressed set; idempotent | MockInput record | P / F |
| T-UNI-10 | Coordinate mapping, Windows | CTRL-05 | Fixtures (DEV-W1 layout; 100/150% mixed DPI) | ANY | `cargo test -p tp-core map_windows` | Corners, edges and centre map to the expected OS px and `SendInput` absolute values with ≤ 1 px error; gap regions are unreachable | test log | P / F |
| T-UNI-11 | Coordinate mapping, macOS | CTRL-05 | Fixtures (Retina 2×, secondary display left of and above main) | ANY | `cargo test -p tp-core map_macos` | Expected global points within ≤ 0.5 pt | test log | P / F |
| T-UNI-12 | Controller view → normalized (TS) | CTRL-05 | Vitest | ANY | `npm run test -- input/mapping` | Correct output with letterbox and pillarbox, DPR 1 / 1.5 / 2, and resize; events outside the content area ignored | Vitest log | P / F |
| T-UNI-13 | Keymap tables | CTRL-05 | `keymap.csv` | ANY | `cargo test -p tp-input keymap` + `npm run test -- input/keymap` | Every MVP HID usage maps to a Windows scan code (+E0) and a macOS `kVK`; no duplicate reverse mappings; both modifier policies (U-07) produce the expected sequences | test logs | P / F |
| T-UNI-14 | Serialization round-trip | BOOT-01 | U-05 decided | ANY | `cargo test -p tp-protocol roundtrip` (proptest) | `decode(encode(m)) == m` for every type; v1 golden bytes stable | test log | P / F |
| T-UNI-15 | Version negotiation and downgrade floor | BOOT-01 (logic), NET-03 (with transcript) | — | ANY | `cargo test -p tp-protocol versions` | Overlap → highest common version; no overlap → `UnsupportedVersion`; below floor → rejected; tampered Hello → key confirmation fails (with T-UNI-20) | test log | P / F |
| T-UNI-16 | Decoder limits and malformed input | BOOT-01 | — | ANY | `cargo test -p tp-protocol malformed` | Oversized, truncated, invalid enums and unknown input fields → error; no panic; bounded allocation | test log | P / F |
| T-UNI-17 | Authorization and permission policy matrix | CAP-02 / CTRL-05 | MockPermissionManager | ANY | `cargo test -p tp-security authz_matrix` | Only allowed combinations pass. For example, `InjectInput` requires ViewAndControl + Connected + link Up + permission Granted; `ExtendSession` requires a local origin | test log | P / F |
| T-UNI-18 | Log redaction | BOOT-01 | tp-diagnostics | ANY | `cargo test -p tp-diagnostics redaction` | Code, keys, `a=ice-pwd`, resume token and TURN credentials appear as `[REDACTED]` | captured log | P: zero leaks · F: any leak |
| T-UNI-19 | Device identity | NET-03 | Keystore abstraction (mock + OS) | BOTH (OS keystore) | `cargo test -p tp-security identity` | Stable device ID; domain-separated sign/verify; fingerprint format; secrets redacted in `Debug`/`Display`; OS keystore round-trip | test log per OS | P: both OSes · B: no Mac (macOS keystore) |
| T-UNI-20 | PAKE handshake and transcript binding | NET-03 | U-03 decided | ANY | `cargo test -p tp-security handshake` | Correct code → keys and confirmation match. Wrong code → failure and attempt counted. Tampered Hello, PAKE message or in-channel SDP fingerprint → failure. RFC vectors pass | test log | P / F |
| T-UNI-21 | UI components (Vitest + Testing Library) | BOOT-01 (skeleton), NET-03, UX-08 | — | ANY | `npm run test` | ConsentDialog escapes names, shows "self-reported", fingerprint and scope, counts down to auto-deny; SessionIndicator shown in `Connected`; PermissionGuide states render | Vitest log | P / F |
| T-UNI-22 | Bounded queue policies | BOOT-01 / VID-04 | — | ANY | `cargo test -p tp-core queues` | FrameSlot is latest-wins; EncodedQueue drops the newest non-key frame and forces a keyframe; drop-until-IDR on the Controller; InputQueue coalesces moves and never drops keys (overflow → Fault) | test log | P / F |

## 3. Integration (T-INT)

| ID | Test | Phase | Preconditions | Device / OS | Steps / command | Expected result | Evidence | P / F / B |
|---|---|---|---|---|---|---|---|---|
| T-INT-01 | Capture pipeline (synthetic) | BOOT-01 / CAP-02 | SyntheticCapture, Passthrough encoder | ANY | `cargo test -p tp-core --test capture_pipeline` (60 fps source, 30 fps consumer) | ≈ 30 encoded/s; replaced ≈ 30/s; monotonic timestamps; memory flat | metrics.json | P / F |
| T-INT-02 | Real capture lifecycle × 10 | CAP-02 | Real provider; OS permission | W1, M1 | `cargo run -p tp-test-harness --bin capture-bench -- --cycles 10` | Each start delivers its first frame ≤ 1 s; stop ≤ 1 s; gauges, handles and threads back to baseline | metrics.json + counters CSV | P: per OS · B: no Mac → macOS half BLOCKED |
| T-INT-03 | Encoder/decoder boundary | VID-04 | Encoder backends | W1, M1 | `cargo test -p tp-media --test codec_roundtrip -- --backend <name>` | Annex B with SPS/PPS before every IDR; no B-frames; `force_keyframe` → next output is IDR; PSNR ≥ 35 dB on the test pattern (PROPOSED) | test log + PSNR CSV | P: per backend · B: backend or OS missing |
| T-INT-04 | Encoder output → WebCodecs decode | VID-04 | Pre-encoded streams; app or test page in the real WebView | W1 (WebView2), M1 (WKWebView) | Harness page in the app: `isConfigSupported`, decode ≥ 300 frames, record decode time and HW status | Supported **and** ≥ 300 frames decoded without error; p95 decode time recorded | page log + metrics.json | P: per WebView · B: no Mac for WKWebView |
| T-INT-05 | DataChannel over SimNet | NET-03 | Chosen WebRTC library (U-01), SimNet | ANY | `cargo test -p tp-transport --test sim_datachannel` (0/2/5/10% loss) | 10,000 messages, each exactly once and in order; latency stats recorded | test log + stats | P / F |
| T-INT-06 | Two-process transport with fingerprint check | NET-03 | `tp-headless` | W1 (two processes, loopback and LAN IP) | `tp-headless host …` + `tp-headless controller …`; harness flag `--corrupt-fingerprint` | ICE connects; remote DTLS fp equals authenticated fp; a corrupted fp → `Fault{Authentication}`; pre-encoded H.264 access units hash-identical at the receiver | logs + hashes | P / F |
| T-INT-07 | Headless session startup | NET-03 | Mocks for capture and input; harness auto-consent | W1 | `cargo test -p tp-test-harness --test session_startup` | Both sides go through exactly the sequence in ARCH-00 §9.1; UI events emitted in order | test log | P / F |
| T-INT-08 | Teardown ordering and gauges | NET-03 | Instrumented fakes | ANY | `cargo test -p tp-core --test teardown_order` | Steps 1–8 of ARCH-00 §5.5 in order; each ≤ 2 s; gauges back to baseline | test log | P / F |
| T-INT-09 | Disconnect/reconnect (simulated) | NET-03 | SimNet partition | ANY | `cargo test -p tp-test-harness --test reconnect` (5 s and 20 s partitions) | 5 s: Degraded → Up, 0 injections during the outage, no new consent. 20 s: both sides `Failed(TransportLost)`, resources freed | test log | P / F |
| T-INT-10 | IPC contract (Rust ↔ TS) | BOOT-01+ | Mocked core / `mockIPC` | ANY | `cargo test -p tp-desktop commands` + `npm run test -- core-api` | Malformed arguments rejected in Rust; event payloads match the TS types | logs | P / F |
| T-INT-11 | Handshake interruption | NET-03 | evil-client | ANY | `cargo test -p tp-test-harness --test handshake_interrupt` (drop after each message) | Both sides terminal within their timeouts; no hang; listener state consistent | test log | P / F |
| T-INT-12 | Busy rejection | NET-03 | An active session | ANY | `cargo test -p tp-test-harness --test busy` | The second Controller gets `Busy`; no consent prompt shown; the active session is unaffected | test log | P / F |
| T-INT-13 | Diagnostics snapshot | BOOT-01+ | — | ANY | `cargo test -p tp-core --test diagnostics_snapshot` | Metrics and gauges present; the secret scanner finds nothing | snapshot JSON | P / F |

## 4. UI (T-UI) — Playwright against the Vite-served UI with mocked IPC

| ID | Test | Phase | Preconditions | Device / OS | Steps / command | Expected result | Evidence | P / F / B |
|---|---|---|---|---|---|---|---|---|
| T-UI-01 | Host flow | BOOT-01 (skeleton), UX-08 | Playwright installed (approval) | ANY | `npm run e2e -- host.spec.ts` | Start hosting → code shown → consent request → Allow → indicator → Disconnect → idle | trace.zip + report | P / F / B: tool not approved |
| T-UI-02 | Controller flow and error states | NET-03, UX-08 | — | ANY | `npm run e2e -- controller.spec.ts` | Wrong code, Denied, Timeout, Busy and lost-link states show the correct messages | trace + report | P / F |
| T-UI-03 | Consent dialog safety | NET-03, UX-08 | — | ANY | `npm run e2e -- consent.spec.ts` | Initial focus is not on Allow; Enter does not approve; Esc denies; countdown auto-denies; "self-reported" label and fingerprint visible; warning text present | trace | P / F |
| T-UI-04 | Accessibility | UX-08 | `@axe-core/playwright` (approval) | ANY | `npm run e2e -- a11y.spec.ts` | No serious or critical axe violations on key screens | report | P / F / B |
| T-UI-05 | i18n completeness | UX-08 | en/vi bundles | ANY | `npm run test -- i18n` + e2e long-string layout | No missing keys; no overflow or truncation of critical text | logs + screenshots | P / F |

## 5. Real device (T-DEV)

| ID | Test | Phase | Preconditions | Device / OS | Steps / command | Expected result | Evidence | P / F / B |
|---|---|---|---|---|---|---|---|---|
| T-DEV-01 | Windows capture, per monitor | CAP-02 | Animated test pattern on each monitor | W1 (3 monitors, negative origins) | `capture-bench --display <id> --duration 600 --scale 1280x720` for each display | Mean delivered fps ≥ 29 with animated content; frame-interval histogram; correct display geometry | metrics.json, preview screenshot (test pattern) | P: all 3 displays · F / B: code not built |
| T-DEV-02 | Windows capture under layout changes | CAP-02 | — | W1 (+ W2 at 150%) | During capture: change resolution, change scale 100 → 150%, disable a monitor | `LayoutChanged` raised, epoch incremented; capture recovers or the session faults cleanly; no crash | log + metrics | P / F / B: W2 for the 150% case |
| T-DEV-03 | Windows secure desktop (UAC, lock) | CAP-02 | — | W1 | Trigger a UAC prompt; Win+L; then return | Behaviour documented (black or frozen); no crash; Host UI informs the user; capture resumes afterwards | log + screenshots (test content) | P: documented and no crash · F: crash or hang |
| T-DEV-04 | macOS capture (Retina) | CAP-02 | Screen Recording granted | M1 | `capture-bench --display main --duration 600` | Mean delivered fps ≥ 29 (animated); correct pixel size and scale | metrics.json | B: **no Mac** |
| T-DEV-05 | macOS capture permission flows | CAP-02 | Fresh TCC state | M1 | NotDetermined → request; Deny; grant later; revoke while streaming | Guidance shown; no crash; revoke → `Fault{PermissionRevoked}`; relaunch requirement documented | screen video (test content) + log | B: no Mac |
| T-DEV-06 | macOS multi-display and own-window exclusion | CAP-02 | Two displays | M1 | Capture each display with the TP Remote windows visible | Each display capturable; TP Remote windows absent from frames | screenshots + log | B: no Mac |
| T-DEV-07 | Windows pointer accuracy | CTRL-05 | Test page reporting click coordinates on every monitor | W1 Host; W2 Controller (or headless controller on W1 with scripted events) | Click corners, edges and centre on all 3 monitors, including negative coordinates; drag; wheel | Position error ≤ 1 px; drag and wheel work | page log CSV | P / F / B: none (headless is acceptable for accuracy) |
| T-DEV-08 | Windows keyboard | CTRL-05 | Notepad; an elevated Notepad | W1 | Type a pangram, use modifiers, extended keys (arrows, Home/End, Insert, right Ctrl/Alt) and F-keys; attempt input into the elevated Notepad | Exact text and behaviour; elevated window blocked (UIPI) **and** the user informed | screenshots + log | P / F |
| T-DEV-09 | macOS input | CTRL-05 | Accessibility flow | M1 | Grant flow; type in TextEdit; pointer accuracy on Retina | Correct text; ≤ 1 pt error | video + log | B: no Mac |
| T-DEV-10 | macOS secure input field | CTRL-05 | A password field | M1 | Type into the password field remotely | Behaviour documented (works or is blocked); no crash | log | B: no Mac |
| T-DEV-11 | DIR-1 macOS → Windows full session | XPLAT-06 | Both devices on LP-1; TTL test config 3 min | M1 → W1 | Consent → view → control → revoke; repeat → expiry | Every SR observation passes (consent, indicator, revoke ≤ 100 ms, expiry) | checklist + evidence set | B: no Mac |
| T-DEV-12 | DIR-2 Windows → macOS | XPLAT-06 | as above | W1 → M1 | as above | as above | as above | B: no Mac |
| T-DEV-13 | DIR-3 Windows → Windows | XPLAT-06 | as above | W1 ↔ W2 | as above, both ways | as above | as above | B: no W2 |
| T-DEV-14 | DIR-4 macOS → macOS | XPLAT-06 | as above | M1 → M2 | as above | as above | as above | B: no Macs |
| T-DEV-15 | Cross-OS keyboard mapping | XPLAT-06 | Both modifier policies | DIR-1, DIR-2 | Copy/paste, select all, undo on the Host; "Send keys" menu items | Expected Host actions for each policy | checklist + video | B: no Mac |
| T-DEV-16 | No stuck keys or buttons | CTRL-05 / XPLAT-06 | — | Any available direction | Hold Shift or a mouse button on the Controller, then: Host Disconnect / kill the Controller / unplug the network | No stuck key or button on the Host within 1 s of teardown or `Degraded` | video + log | P / F / B: no second device (headless kill test acceptable for CTRL-05) |
| T-DEV-17 | Session indicator and OS indicators | NET-03 (basic), XPLAT-06 | — | Each Host OS | Run a 5 min session; try to close or minimize the indicator | Indicator always visible with correct remaining time; it can only be dismissed by disconnecting; WGC border and macOS recording indicator present | screenshots/video | P / F / B (macOS) |

## 6. Security (T-SEC)

| ID | Test | Phase | Preconditions | Device / OS | Steps / command | Expected result | Evidence | P / F / B |
|---|---|---|---|---|---|---|---|---|
| T-SEC-01 | Denied capture permission | CAP-02 | macOS: Screen Recording denied. Windows: MockPermissionManager | M1; ANY (mock) | Click "Allow a remote session" | `PermissionDenied` with guidance; **no listener opened**; no frames captured | log + listener check | P / F / B: no Mac for the real OS case |
| T-SEC-02 | Denied input permission | CTRL-05 | macOS: Accessibility denied | M1; ANY (mock) | Request a ViewAndControl session | Consent offers View-only; the Controller cannot obtain control; 0 injections | log + MockInput count | P / F / B: no Mac (real) |
| T-SEC-03 | Rejected connection | NET-03 | — | ANY (headless), then devices | Host clicks Deny | Controller → `Disconnected(Denied)`; **capture start count = 0; media bytes sent = 0** | log + transport stats | P / F |
| T-SEC-04 | Consent timeout | NET-03 | — | ANY | No action for 60 s (FakeClock in tests; real time on devices) | Auto-deny; both sides `Disconnected(ConsentTimeout)` | log | P / F |
| T-SEC-05 | Expired session | CTRL-05 | TTL = 2 min (test config) | ANY + devices | Let the TTL expire; evil-client keeps sending input | Both sides end with `Expired`; 0 injections after expiry; the Controller cannot extend | log + counts | P / F |
| T-SEC-06 | Revoked authorization | CTRL-05 | Continuous input stream | W1 (+ devices) | Host clicks Disconnect during control | Gate closed ≤ 100 ms (instrumented); capture stopped ≤ 1 s; Controller notified ≤ 2 s; 0 injections after the gate-close timestamp | timestamps CSV | P / F |
| T-SEC-07 | Remote input after disconnect | CTRL-05 | evil-client | ANY | Send `InputBatch` after `Bye`, after teardown, and on a new unauthenticated connection | 0 injected; rejected and counted; no crash | log + counts | P / F |
| T-SEC-08 | Input before consent | NET-03 (mock), CTRL-05 (real) | evil-client | ANY | Send input during Preparing, AwaitingConsent and Connecting | 0 injected; rejected; `ProtocolAbuse` once over the threshold | log + counts | P / F |
| T-SEC-09 | Fuzzing | NET-03 | Nightly toolchain + `cargo-fuzz` (approval) | ANY | `cargo +nightly fuzz run <target> -- -runs=1000000` for envelope, each decoder, the validator and the handshake parser | 0 crashes, timeouts or OOMs per target | fuzz logs + corpus stats | P: all targets clean · F: any crash · B: tooling not approved (**never PASS without running**) |
| T-SEC-10 | Malicious messages | NET-03 | evil-client | ANY | Oversized messages; 10× rate floods; wrong `session_id`; replayed `seq`; unknown types; version downgrade | Rejected per ARCH-00 §7.6; flood → rate-limited, then `ProtocolAbuse`; memory bounded | log + metrics | P / F |
| T-SEC-11 | Code brute force | NET-03 | evil-client | ANY | Repeated wrong codes from one IP and from several IPs | 3 failures invalidate the code and the Host is notified; per-IP limit enforced; further attempts refused | log | P / F |
| T-SEC-12 | MITM on signaling | NET-03 (LAN), WAN-07 (server) | mitm-proxy | ANY | Substitute the SDP fingerprint, PAKE messages or Hello versions | Handshake fails → `Failed(Authentication)`; no media or input exchanged | log + proxy log | P / F |
| T-SEC-13 | Replay of a recorded signaling session | NET-03 | Recorded transcript | ANY | Replay it against a new Host session | Rejected; no state progress beyond Preparing | log | P / F |
| T-SEC-14 | Secrets not in logs or evidence | NET-03+ | Debug logging on | ANY + devices | Full session; scan logs, diagnostics snapshot and evidence folders for code, keys, ice-pwd, resume and TURN patterns | 0 matches | scanner output | P: 0 · F: any match |
| T-SEC-15 | No frame persistence | VID-04 | — | W1, M1 | Run 3 sessions; scan app data, temp and WebView profile directories for new image, video or large binary files | None (harness opt-in dumps are deleted) | scan output | P / F / B: no Mac |
| T-SEC-16 | No background persistence | NET-03, REL-09 | — | W1, M1 | Quit the app; list processes (including WebView children); check autostart entries (Run keys, Startup folder, Scheduled Tasks, services / LaunchAgents, Login Items) | Nothing remains; no entries created | command outputs | P / F / B: no Mac |
| T-SEC-17 | Listener exposure | NET-03 | — | W1, M1 | `Get-NetTCPConnection -State Listen`, `Get-NetUDPEndpoint` (Windows); `lsof -nP -iTCP -sTCP:LISTEN` (macOS) before, during and after hosting | No TP Remote listener unless hosting; listener open only in Preparing; closed after the session | outputs | P / F / B: no Mac |
| T-SEC-18 | No elevation | CAP-02+, REL-09 | — | W1, M1 | Query the process token integrity level (Windows harness probe); check for root processes or helpers (macOS) | Medium integrity; no elevated or root components | output | P / F / B: no Mac |
| T-SEC-19 | Peer-string sanitization | NET-03 | evil-client | ANY | Device name contains `<script>`, `<img onerror>`, U+202E, control characters, 1 MB of data | Rendered as inert text; truncated to ≤ 64 chars; bidi and control characters stripped; no script runs | screenshot + log | P / F |
| T-SEC-20 | Privileged-UI guard | CTRL-05 | Feasibility per FS-C | W1, M1 | Remote clicks on the Host's "Extend session" or settings | Ignored where the guard is feasible; elsewhere a documented limitation with the residual risk accepted by the user | video + log | P / F / B: no Mac |
| T-SEC-21 | Busy and prompt spam | NET-03 | Active or pending session | ANY | Repeated connection attempts | No extra prompts; `Busy` replies; rate limits applied; the active session is unaffected | log | P / F |
| T-SEC-22 | IPC ACL and CSP | BOOT-01 (config), NET-03 (runtime) | Debug build | W1 (+ M1) | From the WebView console, invoke non-allowlisted commands and plugins and inject an inline script | Denied by the ACL; blocked by CSP; DevTools off in release (T-STA-10) | console capture | P / F |

## 7. Network (T-NWK)

| ID | Test | Phase | Preconditions | Device / OS | Steps / command | Expected result | Evidence | P / F / B |
|---|---|---|---|---|---|---|---|---|
| T-NWK-01 | LAN, wired | NET-03 | LP-1 | W1 + W2 | Establish a session | `Connected`; signaling and ICE times recorded; control RTT p95 recorded | metrics.json | B: **no W2** |
| T-NWK-02 | LAN, Wi-Fi ↔ wired | XPLAT-06 | LP-2 | W2 / M1 on Wi-Fi | Establish a session; run 5 min | Stable; stats recorded | metrics.json | B: devices missing |
| T-NWK-03 | Packet loss 1/5/10% | NET-03 (control), VID-04 (video) | SimNet and/or netem | ANY / NET-LAB | Apply the profiles | Session survives; all control messages delivered; video recovers via NACK/PLI; stats recorded | metrics.json + profile | P / F / B: NET-LAB for real links (SimNet portion runnable) |
| T-NWK-04 | Latency/jitter 50 ± 20 ms | NET-03 / VID-04 | as above | as above | Apply the profile | Session stable; latency recorded against the baseline | metrics.json | as above |
| T-NWK-05 | Interruption shorter than grace | NET-03 | Two devices (or SimNet) | W1 + W2 | Disable the adapter for 5 s | Degraded → Up with no new consent; input gate closed during the outage; keyframe after resume | log + metrics | P / F / B: W2 (SimNet covered by T-INT-09) |
| T-NWK-06 | Interruption longer than grace | NET-03 | as above | W1 + W2 | Disable the adapter for 20 s | Both sides `Failed(TransportLost)`; Host resources freed; a new session needs a new code and consent | log | P / F / B: W2 |
| T-NWK-07 | Multi-NIC / virtual adapters | NET-03 | WSL/Docker vEthernet present (as on W1) | W1 (+ W2) | Establish a session; inspect candidate pairs | Physical LAN pair selected; virtual candidates excluded or deprioritized per policy | ICE log | P / F |
| T-NWK-08 | Firewall behaviour | NET-03 | Clean Windows user profile | W1 / W2 | First host start; diff `Get-NetFirewallRule` before and after | OS prompt behaviour documented; **TP Remote created no firewall rules** | outputs + screenshot | P / F |
| T-NWK-09 | Different networks (STUN) | WAN-07 | Approved infrastructure | Two networks | Session across NATs | srflx pair used; stats recorded | ICE log | B: until WAN-07 is approved |
| T-NWK-10 | Symmetric NAT / CGNAT via TURN | WAN-07 | TURN | Two networks | Force relay | Relay pair used; DTLS still bound to the authenticated fingerprints | ICE log | B: until WAN-07 |
| T-NWK-11 | TURN over TCP/TLS 443 | WAN-07 | UDP blocked | Two networks | Block UDP | Session over TURN/TLS | ICE log | B: until WAN-07 |
| T-NWK-12 | Bandwidth cap 2 Mbps | VID-04 | LP-4 | NET-LAB / SimNet | Apply the cap | Encoder bitrate converges below the cap within 5 s (PROPOSED); no stall; fps and latency recorded | metrics.json | P / F |

## 8. Performance (T-PRF)

| ID | Test | Phase | Preconditions | Device / OS | Steps / command | Expected result | Evidence | P / F / B |
|---|---|---|---|---|---|---|---|---|
| T-PRF-01 | Rendered FPS (M-01) | VID-04, XPLAT-06 | LP-1, test pattern | Each available direction | 5 min × 3 runs | Mean ≥ 29.0 and ≥ 95% of 1 s windows ≥ 27 (worst run) | metrics.json series | P / F / B: devices |
| T-PRF-02 | Capture-to-display latency (M-02) | VID-04, XPLAT-06 | LP-1, ≥ 120 fps camera | as above | Glass-to-glass (≥ 300 samples) + instrumented | p95 ≤ 200 ms (worst of 3 runs); the two methods agree within ±20 ms | video + CSV + metrics | P / F / B: camera or devices |
| T-PRF-03 | Input latency (M-03) | CTRL-05, XPLAT-06 | LP-1 | as above | Instrumented + click-to-photon | Baseline recorded (no target in ARCH-00) | CSV + video | P: recorded with evidence · B |
| T-PRF-04 | CPU and memory (M-04) | VID-04+ | LP-1 | Host and Controller | 10 min sampling, including WebView processes | Baseline recorded; budgets set after VID-04 | counters CSV | P: recorded · B |
| T-PRF-05 | Dropped frames per stage (M-05) | VID-04 | LP-1 | as above | 5 min | ≤ 1% end-to-end (PROPOSED) | metrics.json | P / F |
| T-PRF-06 | Session startup (M-06) | NET-03 (headless), VID-04 | LP-1 | as above | 10 sessions | Baseline recorded; ≤ 5 s system time excluding consent (PROPOSED) | logs + metrics | P / F |
| T-PRF-07 | 30-minute stability | XPLAT-06 | LP-1 | Each available direction | 30 min with animated content and periodic scripted input | No crash or hang; FPS target held; RSS growth after warm-up ≤ 10% with no monotonic trend | metrics + counters | P / F / B |
| T-PRF-08 | 10 connect/disconnect cycles (M-07) | NET-03 (headless), XPLAT-06 (real) | — | W1 headless; directions | Script 10 cycles | No crash; gauges back to baseline after every cycle; counters within tolerance | CSV | P / F |
| T-PRF-09 | Bitrate and bandwidth usage | VID-04 | LP-1, LP-4 | as above | 5 min | Recorded (mean, p95) | metrics.json | P: recorded |
| T-PRF-10 | Tauri channel throughput and latency (FS-F3) | VID-04 | Harness page | W1, M1 | 30 msg/s × 20–200 KB for 5 min | 0 drops; p95 one-way IPC latency recorded (≤ 5 ms PROPOSED) | metrics.json | P / F / B: no Mac |

## 9. Release (T-REL)

| ID | Test | Phase | Preconditions | Device / OS | Steps / command | Expected result | Evidence | P / F / B |
|---|---|---|---|---|---|---|---|---|
| T-REL-01 | Windows install and uninstall | REL-09 | Clean VM or device | W2 / clean VM | Per-user install, run, uninstall | No admin prompt; no services, autostart or firewall rules; clean removal (user-data policy documented) | outputs + screenshots | P / F / B: clean machine |
| T-REL-02 | Authenticode verification | REL-09 | Signing certificate | W1 | `signtool verify /pa /v <installer and exe>` | Valid signature and timestamp | output | B: **no certificate (R-09)** |
| T-REL-03 | macOS signing and notarization | REL-09 | Developer ID; Apple account | M1 | `codesign --verify --deep --strict -v`, `spctl -a -vv`, `xcrun stapler validate` | All valid; notarized and stapled | outputs | B: no Mac or account |
| T-REL-04 | Gatekeeper first launch | REL-09 | Clean Mac user | M1 | Download, open | Opens without bypass steps | screenshot | B |
| T-REL-05 | SBOM, licenses, notices | REL-09 | SBOM tooling (approval) | ANY | Generate SBOMs (Cargo + npm); license report; check the notices file | Complete; matches the policy (U-15) | SBOM files + report | P / F / B: tooling |
| T-REL-06 | WebView2 bootstrap | REL-09 | Windows 10 VM without the runtime (best effort, D-15) | VM | Install | Runtime installed or a clear error | screenshot + log | P / F / B: VM |
| T-REL-07 | TCC grants across signing identities | CAP-02 (dev), REL-09 | Dev build and signed build | M1 | Grant permissions in the dev build; install the signed build; update the signed build | Behaviour documented: grants reset on identity change; persist across same-identity updates (expected) | log + screenshots | B: no Mac |

---

## 10. Phase × category summary

| Phase | Gate tests (minimum) |
|---|---|
| BOOT-01 | T-STA-01 … T-STA-10 (Windows; T-STA-04/07 macOS half may be BLOCKED), T-UNI-01 … T-UNI-06, T-UNI-14, T-UNI-16, T-UNI-18, T-UNI-21 (skeleton), T-UNI-22, T-INT-01, T-INT-10, T-INT-13, T-UI-01 (skeleton) |
| CAP-02 | T-INT-02, T-DEV-01 … T-DEV-06, T-SEC-01, T-SEC-18, T-UNI-17 (capture part), T-REL-07 (dev part) |
| NET-03 | T-UNI-15, T-UNI-19, T-UNI-20, T-INT-05 … T-INT-09, T-INT-11, T-INT-12, T-SEC-03, T-SEC-04, T-SEC-08 … T-SEC-14, T-SEC-16, T-SEC-17, T-SEC-19, T-SEC-21, T-SEC-22, T-NWK-01, T-NWK-03 … T-NWK-08, T-PRF-06, T-PRF-08 (headless), T-UI-02, T-UI-03, T-DEV-17 (basic) |
| VID-04 | T-INT-03, T-INT-04, T-NWK-03/04 (video), T-NWK-12, T-PRF-01, T-PRF-02, T-PRF-04, T-PRF-05, T-PRF-09, T-PRF-10, T-SEC-15 |
| CTRL-05 | T-UNI-07 … T-UNI-13, T-UNI-17, T-DEV-07 … T-DEV-10, T-DEV-16, T-SEC-02, T-SEC-05, T-SEC-06, T-SEC-07, T-SEC-20, T-PRF-03 |
| XPLAT-06 | T-DEV-11 … T-DEV-17, T-NWK-02, T-PRF-01, T-PRF-02, T-PRF-07, T-PRF-08 (real), plus re-run of every SR test on both OSes |
| WAN-07 | T-NWK-09 … T-NWK-11, T-SEC-12 (server variant) |
| UX-08 | T-UI-01 … T-UI-05, T-UNI-21 (full), T-SEC-19 |
| REL-09 | T-REL-01 … T-REL-07, T-STA-10, T-SEC-16, T-SEC-18 |

## 11. Direction coverage grid

| Test group | DIR-1 mac→win | DIR-2 win→mac | DIR-3 win→win | DIR-4 mac→mac |
|---|---|---|---|---|
| Full session (T-DEV-11 … 14) | T-DEV-11 | T-DEV-12 | T-DEV-13 | T-DEV-14 |
| Keyboard cross-OS (T-DEV-15) | ✔ | ✔ | n/a (same OS) | n/a (same OS) |
| FPS / latency (T-PRF-01/02) | ✔ | ✔ | ✔ | ✔ |
| Stability / cycles (T-PRF-07/08) | ✔ | ✔ | ✔ | ✔ |
| Stuck keys (T-DEV-16) | ✔ | ✔ | ✔ | ✔ |
| **Executable today** | No (no Mac) | No (no Mac) | No (no W2) | No (no Macs) |

---

## 12. BOOT-02 additions (new canonical IDs)

Status: all PASS on DEV-W1 at engine level (see [BOOT-02-test-results](BOOT-02-test-results.md)).

| ID | Test | Phase | Preconditions | Device / OS | Steps / command | Expected result | Evidence | P / F / B |
|---|---|---|---|---|---|---|---|---|
| T-UNI-23 | Session registry: identity, stale-event rejection, idempotent stop | BOOT-02 | tp-session | ANY | `cargo test -p tp-session --test registry` | New id and higher generation per session; events and stops for other references → `StaleSession` with no change; repeated stops → `AlreadyStopping` / `AlreadyEnded` | test log | P / F |
| T-UNI-24 | Registry concurrency | BOOT-02 | `SharedRegistry` | ANY | `cargo test -p tp-session --test registry scenario_j` | Exactly one stop starts teardown per round; no `Connected` after an accepted stop | test log (rounds, race winners) | P / F |
| T-UNI-25 | Error model: classification, public form, mapping | BOOT-02 | — | ANY | `cargo test -p tp-contracts --test primitives`; `-p tp-protocol --test reserved`; `-p tp-security` | 13 categories, 3 classes; `PublicError` has no internal context; protocol and deny mappings drop peer text | test logs | P / F |
| T-UNI-26 | Secret wrapper and redacted `Debug` | BOOT-02 | — | ANY | `cargo test -p tp-contracts` (incl. doctests); `-p tp-core --test gate_and_logging` | `Secret` cannot be displayed, serialized or cloned (compile-fail); `Debug` of secrets, ids, input, frames and control bytes contains no payload | test logs | P / F |
| T-UNI-27 | Contracts implementable; honest capability advertisement | BOOT-02 | — | ANY | `cargo test -p tp-contracts --test contracts_implementable`; `-p tp-core`; `-p tp-test-harness` | Every ARCH-00 §4 trait is implementable generically; `platform_capabilities()` = NONE; capability crates Foundation; no production implementation of platform traits | test logs | P / F |
| T-UNI-28 | Input gate authorizes every event (mock controller) | BOOT-02 | `RecordingController` | ANY | `cargo test -p tp-core --test gate_and_logging` | 0 injections before consent, after revoke, with view-only scope, with link degraded, or with the production platform; injections only while fully authorized | test log | P / F (mock level only) |
| T-UNI-29 | Reserved message types cannot decode | BOOT-02 | — | ANY | `cargo test -p tp-protocol --test reserved` | All reserved types → `NotAuthorized` on both channels; no `MessageKind` | test log | P / F |

**Notes on existing IDs:**
- T-UNI-04's property test is implemented without `proptest`, as exhaustive reachability exploration (`crates/tp-session/tests/exploration.rs`).
- T-UNI-01 … 06, T-UNI-17 and T-UNI-18 moved from NOT RUN to PASS (engine level) in BOOT-02.
- T-UNI-22, T-INT-01 and T-INT-13 remain NOT RUN (no media pipeline or diagnostics command).

## 13. Prompt alias IDs (BOOT-02) → canonical IDs

| Alias | Canonical | Alias | Canonical |
|---|---|---|---|
| T-CORE-01 | T-UNI-01 | T-SEC-CORE-01 | T-UNI-17, T-UNI-28 |
| T-CORE-02 | T-UNI-01 | T-SEC-CORE-02 | T-UNI-17 |
| T-CORE-03 | T-UNI-02, T-UNI-04, T-UNI-05 | T-SEC-CORE-03 | T-UNI-17 |
| T-CORE-04 | T-UNI-03 | T-SEC-CORE-04 | T-UNI-17 |
| T-CORE-05 | T-UNI-06 | T-SEC-CORE-05 | T-UNI-17 |
| T-CORE-06 | T-UNI-23 | T-SEC-CORE-06 | T-UNI-17 |
| T-CORE-07 | T-UNI-01 | T-SEC-CORE-07 | T-UNI-17, T-UNI-28 |
| T-CORE-08 | T-UNI-23 | T-SEC-CORE-08 | T-UNI-17 |
| T-CORE-09 | T-UNI-23 | T-LOG-01, T-LOG-03 | T-UNI-18 |
| T-CORE-10 | T-UNI-24 | T-LOG-02 | T-UNI-26 |
| T-CONTRACT-01, -05 | T-UNI-27 | T-LOG-04 | T-UNI-25 |
| T-CONTRACT-02 | T-STA-08 | T-CONTRACT-03 | T-UNI-14, T-UNI-15, T-UNI-16 |
| T-CONTRACT-04 | T-INT-10 | T-CONTRACT-06 | T-UNI-29 |

**BOOT-02 gate list:** T-STA-01 … 04, 07, 08, 09, 10; T-UNI-01 … 06, 14 … 18, 23 … 29; T-INT-10; the BOOT-01 Windows runtime checks re-run.
