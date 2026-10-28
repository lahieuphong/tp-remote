# TP Remote

Internal development workspace for **TP Remote**, a planned consent-first, cross-platform remote desktop application (macOS ↔ Windows). The architecture baseline is in [docs/architecture/ARCH-00.md](docs/architecture/ARCH-00.md).

> **Status: BOOT-01 (Windows desktop foundation).** The app runs and reports real diagnostics from its Rust core. **No remote-desktop feature exists yet:** no screen capture, remote input, signaling, networking or streaming. macOS has not been built or tested.

Private internal development, all rights reserved; see [LICENSE.txt](LICENSE.txt).

## Workspace structure

```text
tp-remote/
├── apps/desktop/            Tauri 2 desktop app
│   ├── src/                 React + TypeScript + Tailwind UI
│   │   ├── core-api/        the only code that calls Tauri IPC (+ runtime contract validation)
│   │   ├── bindings/        TypeScript types GENERATED from Rust by ts-rs — do not edit
│   │   ├── app/, components/, contract/, test/
│   └── src-tauri/           Rust shell crate `tp-desktop` (commands, capabilities, config)
├── crates/                  Rust engine, 12 crates (boundaries: ARCH-00 §3.3)
│   ├── tp-contracts         IPC DTOs + module descriptors (ts-rs behind feature `ts-bindings`)
│   ├── tp-protocol          versioned JSON wire format v1, limits, strict validation
│   ├── tp-core              app-core: wires the engine, answers get_app_info
│   ├── tp-diagnostics       stderr event lines
│   ├── tp-session, tp-security, tp-capture, tp-input,
│   │   tp-media, tp-transport, tp-signaling      foundations only (no runtime capability)
│   └── tp-test-harness      test-only: bindings generator + workspace boundary tests
├── tests/fixtures/          JSON fixtures shared by Rust and TypeScript tests
├── scripts/                 check runner, license report, Windows smoke tests
└── docs/                    architecture, ADRs, threat model, QA, roadmap, phase reports
```

## Prerequisites (Windows 11 x64)

| Tool | Version used in BOOT-01 | Notes |
|---|---|---|
| Visual Studio Build Tools 2022, "Desktop development with C++" (MSVC + Windows SDK) | 17.14, MSVC 14.44, SDK 10.0.26100 | Required by Rust's MSVC target and Tauri |
| Microsoft Edge WebView2 Runtime | 155.0.4283.62 | Preinstalled on Windows 11 |
| Rust via [rustup](https://rustup.rs/) | 1.99.1 | `rust-toolchain.toml` pins the exact toolchain; rustup installs it on first use |
| Node.js / npm | 24.19.0 / 11.16.0 | Node ≥ 24.15 is required by jsdom; npm is the only supported package manager |

Install Rust from the official installer (as done in BOOT-01):

```powershell
rustup-init.exe -y --default-host x86_64-pc-windows-msvc --default-toolchain stable --profile minimal --component rustfmt --component clippy
```

## Install

```powershell
npm ci          # exact versions from package-lock.json
cargo fetch     # optional: pre-download crates pinned in Cargo.lock
```

## Develop

```powershell
npm run dev     # tauri dev: Vite on 127.0.0.1:1420 + debug app; closing the window stops everything
```

## Test

```powershell
npm run check               # all gates: fmt, check, clippy -D warnings, cargo test, bindings drift, typecheck, ESLint, Vitest, frontend build
npm run check -- --fast     # same without the production frontend build

npm run rust:test           # cargo test --workspace --locked
npm run rust:clippy         # cargo clippy --workspace --all-targets --locked -- -D warnings
npm run typecheck           # tsc --noEmit
npm run lint                # eslint --max-warnings=0
npm run test                # vitest run
npm run contract:check      # bindings drift + Rust contract tests + Vitest contract tests
```

### Rust ↔ TypeScript bindings

DTOs are defined in Rust (`crates/tp-contracts/src/ipc.rs`). TypeScript is generated from them:

```powershell
npm run bindings:generate   # writes apps/desktop/src/bindings (deterministic)
npm run bindings:check      # fails if the committed bindings differ from the Rust types
```

`cargo test` also fails if the bindings have drifted (`committed_bindings_have_not_drifted`).

### Windows runtime smoke tests

```powershell
npm run build:app                     # release, no installer: target\release\tp-desktop.exe
npm run build:app:debug               # debug, no installer:   target\debug\tp-desktop.exe
powershell -NoProfile -File scripts\smoke-windows.ps1 -Exe target\release\tp-desktop.exe -Label release-run1 -Resize
powershell -NoProfile -File scripts\smoke-windows.ps1 -Exe target\debug\tp-desktop.exe -Label debug-fault -InjectFault
powershell -NoProfile -File scripts\smoke-dev-windows.ps1
```

These scripts start only their own processes, wait on real conditions with time limits, capture only the app window, close the app with WM_CLOSE and report any surviving process. Results go to `docs/qa/evidence/BOOT-01/windows/`.

`-InjectFault` uses the debug-only variable `TP_REMOTE_DEV_FAIL_APP_INFO` to make `get_app_info` fail, so the UI's error state can be checked. Release builds ignore it. Note that `npm run dev` rebuilds `target\debug\tp-desktop.exe` in dev mode, so run `npm run build:app:debug` again before using `-InjectFault`.

### License inventory

```powershell
npm run licenses            # writes docs/qa/evidence/BOOT-01/licenses/license-inventory.{md,json}
```

This is an engineering inventory, not a legal certification.

## Build

```powershell
npm run build               # frontend only (apps/desktop/dist)
npm run build:app           # native Windows app, release, no bundle
```

Installers, code signing and auto-update are intentionally disabled (`bundle.active: false`) until REL-09.

## Reproducing the verified Windows build

1. Install the prerequisites above.
2. `npm ci`
3. `npm run check`: expect 9/9 PASS (51 Rust tests, 18 Vitest tests).
4. `npm run build:app` → `target\release\tp-desktop.exe`.
5. `powershell -NoProfile -File scripts\smoke-windows.ps1 -Exe target\release\tp-desktop.exe -Label repro -Resize`. In `docs\qa\evidence\BOOT-01\windows\repro.summary.json`, expect:
   - `ipcObserved: true`, `cleanExit: true`;
   - no TCP or UDP endpoints;
   - `integrity=Medium`.

The BOOT-01 build's SHA-256 is recorded in the evidence. Byte-for-byte reproducibility of the executable has **not** been verified.

## Known limitations (BOOT-01)

- No remote-desktop functionality of any kind. The UI says so explicitly.
- Verified on Windows 11 x64 at 100% display scaling only. Other scaling factors and macOS are untested.
- The only IPC command is `get_app_info`. The session state machine, consent, PAKE, capture, input, codecs and transport are planned for later phases ([docs/roadmap/PROMPT-PLAN.md](docs/roadmap/PROMPT-PLAN.md)).
- License review items remain open: option-ext (MPL-2.0) is linked through Tauri, and the legal owner is unconfirmed. See [docs/architecture/BOOT-01-dependency-map.md](docs/architecture/BOOT-01-dependency-map.md).
- `cargo audit` and `cargo deny` are not installed. npm audit reported 0 vulnerabilities.

## Documentation

- Architecture: [ARCH-00](docs/architecture/ARCH-00.md), [ADR-001](docs/architecture/ADR-001-stack.md), [ADR-002](docs/architecture/ADR-002-media-transport.md), [BOOT-01 dependency map](docs/architecture/BOOT-01-dependency-map.md)
- Security: [threat model](docs/security/THREAT-MODEL.md)
- QA: [strategy](docs/qa/TEST-STRATEGY.md), [matrix](docs/qa/TEST-MATRIX.md), [BOOT-01 results](docs/qa/BOOT-01-test-results.md), [BOOT-01 environment](docs/qa/BOOT-01-environment.md)
- Reports: [ARCH-00](docs/reports/ARCH-00-report.md), [BOOT-01](docs/reports/BOOT-01-report.md)
