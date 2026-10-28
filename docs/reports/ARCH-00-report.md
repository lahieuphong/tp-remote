# ARCH-00 Report — Architecture, Scope, Security and QA Baseline

| Field | Value |
|---|---|
| Phase | ARCH-00 |
| Date | 2026-10-28 |
| Executed on | DEV-W1 (Windows 11 Enterprise LTSC, build 26100, x64) |
| Verdict | **PASS** against the ARCH-00 criteria (self-assessed). It awaits user review and approval. |
| Next phase | BOOT-01. **Not started.** It is blocked on user approvals; see §11. |

---

## 1. Executive summary

ARCH-00 produced the architecture, security, QA and roadmap baseline for TP Remote. TP Remote is a consent-first, attended remote desktop app supporting macOS↔Windows, Windows↔Windows and macOS↔macOS.

**Architecture:**
- A Tauri 2 shell with a React/TypeScript/Tailwind UI, which is *only* the shell.
- A UI-independent Rust engine of 12 crates, with all cross-module traits defined first in `tp-contracts`.
- Native capture: ScreenCaptureKit on macOS; Windows Graphics Capture on Windows, with DXGI as a candidate fallback.
- Native input: CGEvent on macOS, SendInput on Windows.
- A Rust-side WebRTC stack on both peers, with H.264 Constrained Baseline and a reliable `tp-control` DataChannel.

**Security model:**
- A one-time Host code feeds a PAKE. The authenticated transcript binds identity keys, the protocol version and the DTLS fingerprints.
- **Local consent is still required**; the code alone never grants access.
- Input and media gates are expressed as state-machine invariants.

**Session state machine:**
- 8 states, single-use per session.
- Every exit passes through `Disconnecting`; every transition not in the table is rejected.

**Status of the technical approaches.** No technical approach is verified yet. All ten feasibility areas (FS-A … FS-J) are documented with tests and fallbacks and marked UNVERIFIED or UNKNOWN.

**Biggest risks:**
- No Rust toolchain on the dev host (blocks BOOT-01).
- No macOS hardware and no second Windows device (block the four-direction certification).
- Maturity of the Rust WebRTC stacks.
- WebCodecs behaviour in WKWebView.
- Social-engineering misuse.

**What was not done:** no application code was written, nothing was installed, nothing was built or committed, and BOOT-01 was not started.

## 2. Files created or modified

All files are **new**. The workspace was empty before ARCH-00.

| File | Purpose |
|---|---|
| `docs/architecture/ARCH-00.md` | Scope; decisions D-01 … D-15; unresolved U-01 … U-16; modules and directory layout; contracts; runtime model; state machine; protocol; coordinates and keyboard; flows; FS-A … FS-J; risks R-01 … R-15 |
| `docs/architecture/ADR-001-stack.md` | Stack decision, alternatives, version policy |
| `docs/architecture/ADR-002-media-transport.md` | WebRTC decision, library bake-off rules, channels, codec, render-path decision tree |
| `docs/security/THREAT-MODEL.md` | Assets, actors, trust boundaries, SR-01 … SR-14 mapping, identity verification, TM-01 … TM-26, review gates |
| `docs/qa/TEST-STRATEGY.md` | Principles, levels and tools, devices, commands, fakes, measurement methods M-01 … M-08, LAN profiles, targets, evidence, verdict rules |
| `docs/qa/TEST-MATRIX.md` | 118 test cases (ID, phase, preconditions, device, steps, expected result, evidence, P/F/B criteria) + phase gates + direction grid |
| `docs/roadmap/PROMPT-PLAN.md` | Global scope rules, report template, dependency graph, ARCH-00 … REL-09 phase plans, prompt header template |
| `docs/reports/ARCH-00-report.md` | This report |

Outside the project, two helper scripts were written to the session scratchpad: `check_tables.py` and `check_ids.py`. They are used only to verify the consistency of these documents and are not part of the project.

## 3. Confirmed architecture decisions

| ID | Decision |
|---|---|
| D-01 | Tauri 2 + React + TS + Tailwind as the desktop **shell only** |
| D-02 | Rust Cargo workspace engine, UI-independent, headless-testable |
| D-03 | Contracts first (`tp-contracts`); platform code isolated |
| D-04 | Capture: ScreenCaptureKit; WGC primary (DXGI fallback candidate) |
| D-05 | Input: CGEvent posting; SendInput |
| D-06 | WebRTC transport, with a Rust-side stack on both peers (no WebView RTCPeerConnection) |
| D-07 | Reliable, ordered `tp-control` DataChannel for input and control |
| D-08 | H.264 Constrained Baseline preferred; VP8 fallback candidate |
| D-09 | Consent-first, attended-only, one session per device, Host-authoritative |
| D-10 | One-time code → PAKE → transcript binding of identities, version and DTLS fingerprints; code alone never grants access |
| D-11 | 8-state, single-use session machine; all exits via `Disconnecting`; invalid transitions rejected |
| D-12 | Fixed-point normalized coordinates + layout epoch; HID key codes; `ValidatedInputEvent` gate |
| D-13 | One host display streamed at a time in the MVP |
| D-14 | POC exclusions: database, cloud, mobile, billing, audio, files, clipboard, recording, unattended access, elevation |
| D-15 | Windows 11 x64 primary (Windows 10 best-effort); Apple Silicon primary |

## 4. Unresolved architecture decisions

| ID | Question | Owner phase | Recommendation |
|---|---|---|---|
| U-01 | Rust WebRTC library | NET-03 | Bake-off: str0m / webrtc-rs 0.20 / LiveKit libwebrtc |
| U-02 | Controller render path | VID-04 | WebCodecs in the WebView first; native fallbacks |
| U-03 | PAKE primitive and crate; post-PAKE framing | NET-03 | SPAKE2 or CPace; Noise `NNpsk0` acceptable; security review required |
| U-04 | H.264 encoder and licensing | VID-04 + legal | Hardware first; OpenH264 Cisco binary policy |
| U-05 | Wire serialization | BOOT-01 | Protobuf (`prost` + `protox`) |
| U-06 | Consent prompt surface | NET-03 / UX-08 | Native dialog or hardened WebView dialog |
| U-07 | Cross-OS modifier mapping default | CTRL-05 / XPLAT-06 | User-selectable; default chosen from UX tests |
| U-08 | Minimum OS versions; Intel Mac support | **Product owner, before CAP-02** | macOS 14+ on Apple Silicon; Windows 11 |
| U-09 | JS package manager | BOOT-01 | npm (already installed) |
| U-10 | macOS distribution and entitlements | REL-09 | Developer ID + hardened runtime; evaluate persistent-content-capture |
| U-11 | DXGI fallback needed? | CAP-02 | Decide from measurements |
| U-12 | LAN discovery | NET-03 | Manual address + code |
| U-13 | Rust↔TS type sharing | BOOT-01 | Hand-written types + contract tests |
| U-14 | WAN infrastructure ownership | Before WAN-07 | Requires approval (outside the POC) |
| U-15 | TP Remote license / commercial model | **Before BOOT-01 adds dependencies** | Needed for the `deny.toml` license policy |
| U-16 | Device key storage backend | NET-03 | OS keystore only |

## 5. Environment audit findings

All checks were read-only. Commands and raw results are in §8.

### 5.1 Verified

| Item | Finding |
|---|---|
| Workspace | `E:\Phong_Nho_IT\tp-remote` was **empty**; **not a git repository**; no conflicts with existing files |
| OS | Microsoft Windows 11 Enterprise LTSC, version 10.0.26100 (build 26100), 64-bit, AMD64 |
| CPU / RAM | 12th Gen Intel Core i9-12900K; reported 16 cores / 16 logical processors; 127.8 GB RAM |
| GPUs | Intel UHD Graphics 770 (driver 32.0.101.7088); NVIDIA GeForce RTX 3090 (driver 32.0.16.1088). This is a hybrid-adapter system, relevant for FS-B and FS-E |
| Displays | 3 × 1920×1080 at 96 DPI (100%). DISPLAY3 is primary at (0,0); DISPLAY1 is at (−1920,71); DISPLAY2 is at (−3840,71). The negative-origin layout is used as a test fixture |
| Node.js / npm / npx | v24.19.0 / 11.16.0 / present |
| Yarn / Corepack | 1.22.22 / 0.35.0 |
| Git | 2.53.0.windows.2 |
| Python | 3.14.5 (plus the `py` launcher) |
| MSVC | **Visual Studio Build Tools 2022 17.14.37111.16** (instance 55ef3e8b, complete, launchable); MSVC toolset 14.44.35207; `cl.exe` 19.44.35225 x64; `vcvars64.bat` present; component `VC.Tools.x86.x64` present |
| Windows SDK | 10.0.26100.0 (registry and Include folder); x64 `kernel32.lib` and `ucrt.lib` present; Windows 11 SDK component registered |
| signtool | Present: `Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe` |
| WebView2 Runtime | 155.0.4283.62 (machine-wide) |
| Docker / WSL | Docker 29.7.2; WSL distro `docker-desktop` running (WSL 2). Virtual network adapters are likely (relevant for FS-I) |
| Chocolatey | Present (not used) |
| Windows Firewall | Domain, Private and Public profiles enabled; DefaultInboundAction NotConfigured |
| Global npm packages | @anthropic-ai/claude-code 2.1.307, @nestjs/cli 12.0.8, @openai/codex 0.162.0, npm 11.16.0 |

### 5.2 Missing

| Item | Impact |
|---|---|
| **Rust toolchain** (`rustc`, `cargo`, `rustup`; no `~/.cargo`, no `~/.rustup`) | **Blocks BOOT-01** (R-01) |
| Tauri CLI | Expected to arrive as a project dev dependency in BOOT-01 |
| pnpm, bun | None, if npm is chosen (U-09) |
| protoc | Use `protox` (pure Rust) if Protobuf is chosen (U-05) |
| CMake, Ninja, clang | Needed only if a C/C++ dependency is chosen (for example a libwebrtc source build) |
| GitHub CLI (`gh`), winget, scoop | Not needed for BOOT-01 |
| `cl`/`link` on PATH | Normal for Build Tools; Rust's MSVC detection is expected to find them through the VS setup configuration (**UNVERIFIED** until Rust is installed) |

### 5.3 Unverified / unknown

| Item | Status |
|---|---|
| Rust's MSVC linker auto-detection with this Build Tools instance | UNVERIFIED (no Rust) |
| `C:\Program Files\Microsoft Visual Studio\18` | The folder exists with no edition or instance inside; it is not registered with vswhere. Effect: none expected |
| Hardware H.264 MFTs (NVENC, Quick Sync) through Media Foundation | UNVERIFIED (needs code; VID-04) |
| Second Windows device, macOS devices, NET-LAB, ≥ 120 fps camera | Not available or unknown |
| macOS Xcode / Command Line Tools | Cannot be audited from a Windows host |
| Code-signing certificate, Apple Developer account | Unknown |
| Reported 16 logical processors on an i9-12900K | Reported as-is (the CPU has 24 threads when Hyper-Threading is enabled). Not relevant to ARCH-00 |

**Platform certification note.** This Windows host can certify Windows behaviour only. No macOS behaviour can be verified from it.

## 6. Security risks (top items)

| Risk | Reference | Mitigation status |
|---|---|---|
| Social-engineering misuse (tech-support scams) | TM-11, R-11 | Consent UX, warnings, View-only option, indicator, TTL, no unattended access. Residual risk is inherent |
| MITM or rogue signaling substituting DTLS fingerprints | TM-01 … TM-03 | Designed: PAKE transcript binding + independent fingerprint check. **Not yet implemented.** No "end-to-end authenticated" claim until T-UNI-20, T-SEC-12 and T-INT-06 pass |
| Input before consent or after revoke | TM-07 | Designed: gate invariants + `ValidatedInputEvent` + single owner of the injector |
| WebView compromise abusing the consent IPC | TM-16 | CSP, ACL, sanitization; U-06 (native consent dialog option) |
| The Controller operating the Host's own TP Remote UI | TM-12 | Privileged-UI guard feasibility is UNKNOWN (FS-C) |
| Supply chain (crates, npm, prebuilt libwebrtc / OpenH264) | TM-18 | `cargo deny`/`audit`, lockfiles, checksum pinning; tools need installation approval |
| Secrets or frames persisted or logged | TM-13, TM-14 | Redaction layer, zeroization, no frame writes; tests defined |
| Code brute force | TM-04 | PAKE (online guessing only) + 3 attempts + TTL + per-IP limits |

## 7. Test strategy and coverage

- **118 test cases** are defined in TEST-MATRIX. Each has an ID, phase, preconditions, device/OS, steps or command, expected result, evidence, and PASS/FAIL/BLOCKED criteria.

| Category | Count |
|---|---|
| Static | 10 |
| Unit | 22 |
| Integration | 13 |
| UI | 5 |
| Real device | 17 |
| Security | 22 |
| Network | 12 |
| Performance | 10 |
| Release | 7 |

- **Mandatory categories from the brief are covered.** STATIC: T-STA-01 … 07. UNIT: state machine, input validation, coordinates, serialization, permissions. INTEGRATION: capture, codec, transport, startup/teardown, reconnect. REAL DEVICE: capture and input on both OSes, all four directions. SECURITY: denied permission, expired, rejected, revoked, input after disconnect, malicious messages. NETWORK: LAN, loss, interruption, reconnect, different networks. PERFORMANCE: FPS, latency, CPU/memory, drops, startup, cleanup.
- **Measurement methods** M-01 … M-08 are defined, including glass-to-glass and instrumented latency. LAN profiles LP-1 … LP-4 are defined. Targets (720p30; p95 ≤ 200 ms; 30 min; 10 cycles) are recorded as **hypotheses**, with acceptance rules and a revision procedure.
- **Exit gates** are defined for every phase (TEST-MATRIX §10, PROMPT-PLAN §3 … §11).
- **Security requirements** SR-01 … SR-14 are each mapped to controls and tests (THREAT-MODEL §5).

## 8. Commands executed and actual results

All commands were run in PowerShell 5.1 on DEV-W1. Every command was read-only, with the exceptions noted.

| # | Command (summary) | Actual result |
|---|---|---|
| 1 | `Get-ChildItem -Force -Recurse -Depth 3 "e:\Phong_Nho_IT\tp-remote"` | No output: the directory was empty |
| 2 | `Get-CimInstance Win32_OperatingSystem`, `Win32_Processor`, `Win32_VideoController`; `$PSVersionTable` | Windows 11 Enterprise LTSC 10.0.26100, 64-bit, AMD64; i9-12900K 16/16; 127.8 GB; UHD 770 (32.0.101.7088) and RTX 3090 (32.0.16.1088) at 1920×1080; PowerShell 5.1.26100.9711 |
| 3 | `Get-Command` for 28 tools | Found: node, npm, yarn, npx, git, python, py, choco, docker, wsl. Missing: pnpm, bun, rustc, cargo, rustup, gh, cl, link, clang, cmake, ninja, winget, scoop, vswhere (not on PATH), tauri, cargo-tauri, protoc, go |
| 4 | `node --version`, `npm --version`, `yarn --version`, `git --version`, `python --version`, `corepack --version`; `Test-Path` for `~/.cargo/bin/cargo.exe` and `~/.rustup`; vswhere query | v24.19.0 · 11.16.0 · 1.22.22 · 2.53.0.windows.2 · 3.14.5 · 0.35.0 · False · False · vswhere present at its default path. **The vswhere query listed no instances because it was malformed:** `-property` was given a comma-separated list, which vswhere does not accept. It was corrected in #9 |
| 5 | Windows Kits folder and registry; VS folders; WebView2 registry keys | SDK 10.0.26100.0 (Include folder + registry ProductVersion 10.0.26100); VS folders `…\Microsoft Visual Studio\18`, `(x86)\…\2022`, `Installer`, `Shared`; WebView2 HKLM pv = 155.0.4283.62; HKCU key absent |
| 6 | Enumerate `VC\Tools\MSVC` under each VS folder; repeat the vswhere query (same malformed form) | `2022\BuildTools`: toolset 14.44.35207, x64 `cl.exe` True, `vcvars64.bat` True; folder `18` has no editions; vswhere again printed only its banner (same cause as #4) |
| 7 | Run `cl.exe` with no arguments to read its banner (stdout/stderr redirected to two temp files in `%TEMP%`, **which were then deleted**); `Test-Path` on SDK libs; VS `_Instances` folder; `npm ls -g --depth=0` | "Microsoft (R) C/C++ Optimizing Compiler Version 19.44.35225 for x64"; `kernel32.lib` and `ucrt.lib` x64 True; instance folder 55ef3e8b; global npm packages as in §5.1 |
| 8 | Read `_Instances\55ef3e8b\state.json`; enumerate screens through System.Windows.Forms; read `AppliedDPI` | installationPath `…\2022\BuildTools`, version 17.14.37111.16, product `Microsoft.VisualStudio.Product.BuildTools`; screens: DISPLAY1 (−1920,71) 1920×1080, DISPLAY2 (−3840,71) 1920×1080, DISPLAY3 (0,0) 1920×1080 primary; AppliedDPI 96 |
| 9 | Corrected vswhere: `-all -products * -format text`; `-requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64`; `-requiresAny` for the Windows 11 SDK components | instanceId 55ef3e8b, "Visual Studio Build Tools 2022", 17.14.37111.16, isComplete 1, isLaunchable 1; both component queries returned the BuildTools path |
| 10 | Locate signtool; `Get-NetFirewallProfile`; `wsl.exe -l -v`; `docker --version` | signtool at `…\10.0.26100.0\x64\signtool.exe`; all three firewall profiles Enabled = True, DefaultInboundAction NotConfigured; `docker-desktop` Running, version 2; Docker 29.7.2 |
| 11 | `python -I check_tables.py docs` (scratchpad script) | `table problems: 0`, after fixing one unescaped pipe in a PROMPT-PLAN table cell |
| 12 | `python -I check_ids.py docs` (scratchpad script) | Defined: 118 tests, 15 D, 16 U, 15 R, 26 TM, 14 SR; `undefined refs: 0` |

**Web research** (read-only; used for the feasibility evidence; findings are cited as documentation, not as test results):
- webrtc-rs status, str0m, LiveKit Rust SDK and libwebrtc;
- Tauri 2 versions and WebView support;
- macOS Sequoia screen-recording prompts and the persistent-content-capture entitlement;
- WGC border and access APIs;
- WebCodecs in Safari and WKWebView;
- OpenH264 licensing;
- CGEvent posting permissions;
- the RustDesk license.

Sources are listed in ARCH-00 §13.

**Not executed:**
- no installation;
- no build;
- no `git init`, commit or push;
- no firewall, network or system changes;
- no access to directories outside the workspace other than read-only toolchain and system locations (Program Files, Windows Kits, the registry, the user profile's `.cargo` and `.rustup` paths, the global npm prefix).

## 9. Tests NOT executed, and why

| Tests | Reason |
|---|---|
| **All 118 test cases** in TEST-MATRIX | ARCH-00 is a documentation-only phase. No application code exists, so there is nothing to test. Status: NOT RUN |
| Every Rust-based test (T-STA-03/04/05/06/07/08/09, T-UNI-*, T-INT-*) | They also need a Rust toolchain, which is not installed (R-01) |
| macOS tests (T-DEV-04/05/06/09/10, macOS halves of the others) | No Mac (R-02) |
| Four-direction tests (T-DEV-11 … 14) and two-device network tests | No Mac and no second Windows device (R-02, R-03) |
| Fuzzing (T-SEC-09) | Needs a nightly toolchain and `cargo-fuzz`, which need approval |
| Release tests (T-REL-02/03/04) | No signing certificate or Apple account confirmed (R-09) |
| Glass-to-glass latency (T-PRF-02) | Needs a ≥ 120 fps camera and two devices |

No test results are claimed in any document.

## 10. Verdict

**PASS** for ARCH-00, self-assessed against the brief's PASS criteria. It still requires user review.

| PASS criterion | Status | Evidence |
|---|---|---|
| All requested documentation completed | ✔ All 8 files exist | §2 |
| Architecture boundaries internally consistent | ✔ One module catalog and set of dependency rules; IDs cross-checked (0 undefined references); tables validated | §8 #11–12; ARCH-00 §3.3 |
| High-risk assumptions explicitly identified | ✔ FS-A … FS-J marked UNVERIFIED/UNKNOWN with tests and fallbacks; R-01 … R-15; open security questions SQ-1 … SQ-5 | ARCH-00 §10–11; THREAT-MODEL §10 |
| Test matrix and exit gates complete | ✔ 118 cases; gates per phase | TEST-MATRIX §10; PROMPT-PLAN |
| No unauthorized implementation or system changes | ✔ Only Markdown documents were created. Two temp files used to read the compiler banner were deleted. Two helper scripts live in the session scratchpad, outside the project | §8 |

**Caveat.** The PASS concerns the *documentation baseline only*. It does not mean any technical approach is feasible; every feasibility item remains unproven.

## 11. Readiness for BOOT-01

**Not ready to start** until the user provides the following:

| # | Item | Needed for |
|---|---|---|
| 1 | Review and approval of this ARCH-00 baseline | Every phase |
| 2 | **Approval to install the Rust toolchain** on DEV-W1: rustup with the stable `x86_64-pc-windows-msvc` toolchain plus the `rustfmt` and `clippy` components. Optionally also `cargo-deny`/`cargo-audit` (T-STA-05/06) and Playwright browsers (T-UI-01) | BOOT-01 build and tests |
| 3 | Decisions, or acceptance of the recommendations, for **U-05** (Protobuf/prost), **U-09** (npm), **U-13** (hand-written TS types) and **U-15** (TP Remote license model → dependency license policy) | BOOT-01 start |
| 4 | Optional: approval for `git init` in the workspace (no commits unless asked) | Evidence traceability (commit hashes) |
| 5 | Early notice: a Mac is needed by CAP-02, a second Windows device by NET-03, and **U-08** (minimum OS versions) must be decided before CAP-02 | Later phases |

BOOT-01 itself needs no Mac. Its macOS-side static checks (the macOS halves of T-STA-04/07) will be reported as BLOCKED unless a Mac is available.

**STOP.** BOOT-01 has not been started. Waiting for user review and approval.
