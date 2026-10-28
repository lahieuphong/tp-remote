# ADR-001 — Technology Stack

| Field | Value |
|---|---|
| Status | **Accepted (baseline)**, subject to user review of ARCH-00. Version pins are deferred (see §6). |
| Date | 2026-10-28 |
| Phase | ARCH-00 |
| Related | [ARCH-00](ARCH-00.md) D-01, D-02, D-03, D-04, D-05, D-14, D-15 · [ADR-002](ADR-002-media-transport.md) · [Threat model](../security/THREAT-MODEL.md) |

## 1. Context

TP Remote is a cross-platform (macOS and Windows) attended remote desktop application. Its main components are:
- native screen capture;
- native input injection;
- a real-time video pipeline;
- an encrypted peer-to-peer transport;
- a small but security-critical UI: consent prompt, session indicator and permission guidance.

The project brief fixes a target stack. This ADR records it, explains the boundaries that make it work, and lists the alternatives and the consequences.

Key fact: **Tauri is a desktop application shell. It is not a remote desktop engine.** It provides no screen capture, no codecs, no input injection and no media transport. All of those must be built in Rust (with platform FFI), behind contracts that the shell only orchestrates.

## 2. Decision drivers

1. Two desktop OSes from one codebase, with platform-specific native code where it is unavoidable (capture, input, codecs).
2. Memory safety for network-facing parsers and security logic.
3. A small attack surface and a small footprint compared with bundling a browser runtime.
4. Engine testability without a UI: headless, deterministic and fast.
5. A UI that can be built quickly and covered by tests (Vitest, Playwright).
6. Licensing that is compatible with an independent, possibly commercial product (U-15).

## 3. Decision

| Layer | Choice | Notes |
|---|---|---|
| Desktop shell | **Tauri 2** | Window management, WebView (WebView2 on Windows, WKWebView on macOS), IPC (commands, events, channels), ACL capabilities, CSP, bundling and signing hooks |
| UI | **React + TypeScript + Tailwind CSS**, built with Vite (the Tauri default template; confirmed in BOOT-01) | All Tauri calls go through `src/core-api/` |
| Engine | **Rust**, in a Cargo workspace under `crates/` | No dependency on Tauri; headless harness for tests |
| Contracts | `tp-contracts` crate | Traits and shared types: CaptureProvider, InputController, SessionManager, Transport, VideoEncoder/VideoDecoder, PermissionManager, DeviceIdentity |
| macOS capture | **ScreenCaptureKit** | FS-A |
| Windows capture | **Windows.Graphics.Capture** (primary); DXGI Desktop Duplication (fallback candidate, U-11) | FS-B |
| macOS input | **Quartz Event Services** (`CGEventPost`) | FS-C |
| Windows input | **Win32 `SendInput`** | FS-C |
| Media and network | **WebRTC** (see ADR-002) | Rust-side stack; H.264 preferred |
| Async runtime | **tokio** in `tp-core`, `tp-signaling` and `tp-transport`. Platform crates stay synchronous and runtime-agnostic | Chosen so that platform code does not depend on a runtime |
| Windows bindings | Microsoft **`windows`** crate (windows-rs) | Version chosen in CAP-02 |
| macOS bindings | **`objc2`** family of crates (preferred) | Evaluated in CAP-02 |
| Logging and metrics | `tracing` plus a TP Remote redaction layer | `tp-diagnostics` |
| Tests | `cargo test`, `proptest` (proposed), `cargo-fuzz` (proposed, needs a nightly toolchain), Vitest, Playwright, real-device runs | See TEST-STRATEGY |
| Static gates | `cargo fmt`, `cargo clippy -D warnings`, `tsc --noEmit`, ESLint, `cargo deny`, `cargo audit`, `npm audit` | Installing the extra tools needs approval |
| JS package manager | npm (PROPOSED; U-09) | npm 11.16.0 is installed; pnpm is not |

Excluded from the POC: database, cloud services, mobile, billing (D-14).

## 4. Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Tauri 2 + Rust engine** (chosen) | Small runtime (system WebView); Rust safety; ACL/CSP security model; one language for engine and shell | Two WebView engines behave differently, especially for media (FS-F); building native video surfaces is harder; macOS has no WebDriver (affects E2E tests) | **Chosen** (mandated by the brief and fits the drivers) |
| Electron + Node native addons | Mature; one Chromium engine on both OSes (uniform WebCodecs/WebRTC); Playwright support | Large footprint; bigger attack surface; native addons still needed for capture and input; JS on the security-critical path | Rejected |
| Qt / C++ | Native widgets and video surfaces; mature multimedia | Licensing (LGPL/commercial); C++ memory-safety risk on parsers; slower UI iteration | Rejected |
| Flutter desktop + Rust (FFI) | Custom rendering; texture widgets for video | Two FFI layers; platform-channel complexity; less mature on desktop | Rejected |
| Fully native UIs (SwiftUI + WinUI) over a Rust core | Best platform integration and native video surfaces | Two UIs to build and test; slower | Rejected for the MVP. **Partial reuse:** the F-2/F-3 fallbacks in FS-F borrow from this option |
| egui/iced (pure-Rust UI) | One language; direct GPU rendering for video | Accessibility and polish gaps; differs from the brief | Rejected (could support F-3) |

## 5. Consequences

**Positive**
- The engine can be tested without a UI. `tp-test-harness` drives real `tp-core` sessions headless.
- Security-critical logic (authentication, consent gating, validation) runs in safe Rust. The WebView displays decisions; it does not *enforce* them.
- Platform code is isolated, so a missing platform implementation does not block the rest of the work.

**Negative / costs**
- The Controller-side video rendering path is uncertain inside the WebView (FS-F, U-02, R-05).
- The two WebView engines mean behaviour must be tested twice (WebView2 on Chromium, WKWebView on WebKit).
- Tauri's WebDriver support does not cover macOS. Plan: Playwright against the Vite-served UI with mocked IPC on every OS; WebView2 via the Chrome DevTools Protocol (UNKNOWN feasibility); macOS end-to-end testing is manual. See TEST-STRATEGY §2.
- Native code in Objective-C and WinRT through FFI requires `unsafe`, confined to the platform crates.

## 6. Version policy (no pins in ARCH-00)

- ARCH-00 deliberately **pins no dependency versions**. Versions observed during research are references, not pins:
  - Tauri 2.9.6 is cited in December 2025 material; the Tauri changelog lists updates through June 2026.
  - webrtc-rs 0.20.x and str0m 0.23.x appear on docs.rs.
- BOOT-01 selects versions by doing the following:
  1. Query the registries (`cargo search` / `cargo info`, `npm view <pkg> version`).
  2. Check compatibility: Tauri CLI, the `@tauri-apps/api` JS package and the `tauri` crate must share a minor version; check `tauri info`.
  3. Record the exact versions in `Cargo.lock` and `package-lock.json` and in the BOOT-01 report.
  4. Run `cargo deny` license and advisory checks.
- The Rust toolchain is pinned in `rust-toolchain.toml` (stable channel, exact version chosen in BOOT-01).
- Any dependency added in a later phase must be listed in that phase's report with its version, license, reason and compatibility-check output (PROMPT-PLAN §1).

## 7. Risks

R-01 (no Rust toolchain installed), R-05 (WKWebView media), R-12 (supply chain) — see ARCH-00 §11.

## 8. Validation

- BOOT-01: the workspace builds; static gates T-STA-01 … T-STA-10 pass on Windows; the Tauri app window launches with a mocked session flow.
- CAP-02 / CTRL-05: the platform binding crates (`windows`, `objc2-*`) are proven by the T-DEV tests.

## 9. Revisit triggers

- FS-F fails on both platforms, so no acceptable render path exists inside or alongside Tauri.
- Tauri 2 security or maintenance status changes materially.
- The brief changes the platform set (for example, adds Linux or mobile).
