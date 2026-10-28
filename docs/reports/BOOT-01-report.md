# BOOT-01 Report — Windows Desktop Foundation, Rust Workspace, Tauri Shell & QA

| Field | Value |
|---|---|
| Phase | BOOT-01 |
| Date | 2026-10-28 |
| Host | DEV-W1: Windows 11 Enterprise LTSC 26100 x64 |
| Verdict | **PASS** for the Windows-only foundation scope. Limitations are listed in §14 and §15 |
| Next phase | CAP-02. **Not started**; waiting for user review |

Sections are marked **[Observed]** for results executed and recorded in this phase, and **[Recommendation]** / **[Assumption]** otherwise. Detailed results: [BOOT-01-test-results](../qa/BOOT-01-test-results.md) · Environment: [BOOT-01-environment](../qa/BOOT-01-environment.md) · Dependencies and licenses: [BOOT-01-dependency-map](../architecture/BOOT-01-dependency-map.md).

---

## 1. Executive summary [Observed]

TP Remote now has a real, locally runnable Windows desktop foundation:
- **Rust workspace:** 12 engine crates, following the ARCH-00 boundaries, plus the Tauri 2 shell crate.
- **UI:** a React/TypeScript/Tailwind interface that calls one real IPC command, `get_app_info`.
- **Shared types:** TypeScript types are generated from Rust by ts-rs, with drift checks on both sides.
- **Protocol:** a versioned, strictly validated JSON protocol foundation (v1), with no handshake or commands.

The Rust toolchain was installed with your approval: rustup 1.29.2 and rustc 1.99.1 for MSVC, SHA-256 verified.

**Checks, all passing:**
- format, `cargo check` and clippy with `-D warnings`;
- **51 Rust tests** and **18 Vitest tests**;
- typecheck, ESLint, the production frontend build and the native release and debug builds.

**On the real Windows host,** the app:
- launches unelevated at Medium integrity, with PerMonitorV2 DPI awareness;
- renders inside WebView2 and shows live Rust data;
- shows a visible **Failed** state when the IPC call fails (debug-only fault injection; release builds ignore the switch);
- holds its layout at four window sizes and on a negative-coordinate monitor;
- opens **no TCP or UDP endpoint**;
- closes cleanly with no orphan process, and starts again.

The development launch (`tauri dev`) also works, and its whole process tree exits when the window closes.

**No remote-desktop capability exists:** no capture, input, signaling, streaming or PAKE, and the UI says so.

## 2. Changes to architecture decisions U-05, U-09, U-13, U-15 [Observed]

| ID | Recorded in | Decision as implemented |
|---|---|---|
| U-05 | ADR-002 amendment A1 | **Versioned JSON with Serde.** Explicit `protocolVersion` (v1, supported 1..=1); typed `Message`/`Body`; deterministic encoding (golden bytes); strict decoding with no unknown or duplicate fields; size limits checked before parsing (64 KiB signaling, 16 KiB control). Unknown types → `UnknownMessageType`; reserved future types or a known type on the wrong channel → `NotAuthorized`. Only the *shape* of `hello` exists; there are no remote-control commands. UI IPC contracts (`tp-contracts::ipc`) are kept separate from network protocol contracts (`tp-protocol`). |
| U-09 | ADR-001 amendment A1 | **npm** (installed 11.16.0 on Node 24.19.0, unchanged); npm workspaces; `package-lock.json` committed; `npm ci`. pnpm and Yarn not used. |
| U-13 | ADR-001 amendment A1 | **Rust-first DTOs with ts-rs 12.0.1.** Generated into `apps/desktop/src/bindings` by `npm run bindings:generate`. Drift check: `npm run bindings:check` plus a Rust test. React consumes the generated `AppInfo` and validates the response at runtime. ts-rs is behind a feature enabled only by the test harness, so it is not linked into the app. |
| U-15 | ADR-001 amendment A1 | **Private internal development, all rights reserved.** `LICENSE.txt`; Cargo `LicenseRef-TP-Remote-Proprietary` with `publish = false`; npm `UNLICENSED` with `private`. A transitive license inventory exists; flagged items are documented, not auto-rejected. No legal certification is claimed. |

**Contradictions in the baseline resolved before implementation:**

| # | Contradiction | Resolution |
|---|---|---|
| C-1 | ARCH-00 recommended Protobuf (U-05) and hand-written TS types (U-13) | Superseded by your decisions. ADR amendments added; a status note was added to ARCH-00 §2.2 |
| C-2 | ARCH-00 §7.2 envelope (`proto_major`/`minor`; unknown fields ignored) vs. strict versioned JSON | v1 uses a single `protocolVersion` and rejects unknown fields; recorded in ADR-002 A1 |
| C-3 | ARCH-00 §3.5 names the command `app_info`; the BOOT-01 prompt says `get_app_info` | `get_app_info` used; noted in ARCH-00 §2.2 |
| C-4 | PROMPT-PLAN's BOOT-01 scope (full state machine, redaction layer, all §4 traits, fakes, Playwright skeleton, cargo-deny) vs. this prompt ("minimum viable contracts", no extra tools or browsers) | The prompt was followed. The deferred items are listed in §14 |
| C-5 | The prompt's layout adds a root `tests/` and `README.md` that are not in ARCH-00 §3.4 | Added; this is additive and changes no boundary |

## 3. Toolchains installed and verified [Observed]

| Tool | How | Verified |
|---|---|---|
| rustup 1.29.2 + stable-x86_64-pc-windows-msvc (rustc 1.99.1, cargo 1.99.1, rustfmt 1.10.0, clippy 0.1.99) | Approved install. `rustup-init.exe` from `win.rustup.rs`, SHA-256 matched the official `.sha256` (binary not Authenticode-signed). Minimal profile. User PATH updated | `rustc -vV`, `cargo -V`, `rustup show`, `cargo fmt/clippy --version`; MSVC link test executed ("msvc link ok") |
| Project pin | `rust-toolchain.toml` = 1.99.1 | `tauri info`: "overridden by rust-toolchain.toml" |
| Node 24.19.0 / npm 11.16.0, MSVC 14.44, Windows SDK 10.0.26100, WebView2 155.0.4283.62 | Pre-existing, unchanged | Re-checked |

**Not installed:** pnpm, Yarn, protoc, CMake, cargo-deny, cargo-audit, nightly toolchain, Playwright browsers, Docker services.

## 4. Final directory tree [Observed]

Build outputs (`target/`, `node_modules/`, `dist/`, `gen/`) and evidence files are omitted.

```text
tp-remote/
├── .gitignore  Cargo.toml  Cargo.lock  LICENSE.txt  README.md  package.json  package-lock.json
├── rust-toolchain.toml  rustfmt.toml
├── apps/desktop/
│   ├── app-icon.svg  eslint.config.js  index.html  package.json  tsconfig.json  vite.config.ts  vitest.config.ts
│   ├── src/
│   │   ├── main.tsx  index.css
│   │   ├── app/          App.tsx  App.test.tsx  useAppInfo.ts
│   │   ├── bindings/     AppInfo.ts … ShellInfo.ts  index.ts   (11 generated files)
│   │   ├── components/   AppMark  Card  CoreStatusCard  DiagnosticsCard  EngineModulesCard  RemoteFeaturesCard
│   │   ├── contract/     appInfo.contract.test.ts
│   │   ├── core-api/     appInfo.ts  errors.ts  errors.test.ts  parseAppInfo.ts
│   │   └── test/         setup.ts
│   └── src-tauri/
│       ├── Cargo.toml  build.rs  tauri.conf.json  windows-app-manifest.xml
│       ├── capabilities/main-window.json
│       ├── icons/        32x32 64x64 128x128 128x128@2x icon.png icon.ico icon.icns
│       ├── permissions/autogenerated/get_app_info.toml   (generated by tauri-build)
│       ├── src/          lib.rs  main.rs
│       └── tests/        config.rs  ipc.rs
├── crates/
│   ├── tp-contracts/     src/{lib,ipc,module}.rs   tests/ipc_fixtures.rs
│   ├── tp-protocol/      src/{lib,envelope,error,limits,message,version}.rs   tests/{codec,fixtures}.rs
│   ├── tp-core/  tp-diagnostics/      src/lib.rs (with unit tests)
│   ├── tp-session/  tp-security/  tp-capture/  tp-input/  tp-media/  tp-transport/  tp-signaling/   src/lib.rs (foundation)
│   └── tp-test-harness/  src/{lib,bindings}.rs  src/bin/tp-bindings.rs   tests/{bindings,workspace_boundaries}.rs
├── tests/  README.md  fixtures/ipc/{app-info.valid.json, invalid/5}  fixtures/protocol/{cases.json, valid/2, invalid/21}
├── scripts/  check.mjs  license-report.mjs  smoke-windows.ps1  smoke-dev-windows.ps1
└── docs/
    ├── architecture/  ARCH-00.md  ADR-001-stack.md  ADR-002-media-transport.md  BOOT-01-dependency-map.md
    ├── security/      THREAT-MODEL.md
    ├── qa/            TEST-STRATEGY.md  TEST-MATRIX.md  BOOT-01-test-results.md  BOOT-01-environment.md  evidence/BOOT-01/
    ├── roadmap/       PROMPT-PLAN.md
    └── reports/       ARCH-00-report.md  BOOT-01-report.md
```

## 5. Rust workspace / crate compilation status [Observed]

| Item | Status |
|---|---|
| Workspace | 13 members (12 ARCH-00 crates + `tp-desktop`); resolver 3; edition 2024; `rust-version` 1.90 |
| `cargo check --workspace --all-targets --locked` | exit 0, 0 warnings |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0, 0 findings. A control run with pedantic lints proved clippy lints `tp-desktop`'s lib and test targets |
| Boundary rules | 6 tests PASS; a deliberate violation was caught (negative control) |
| Crate honesty | 7 crates are `Foundation` (no runtime code). `tp-core` asserts in a test that no remote-capability crate claims progress. Workspace lints deny `todo!`, `unimplemented!` and `dbg!` |
| `unsafe` | None anywhere. `forbid(unsafe_code)` in 6 crates; `deny` elsewhere |

## 6. Tauri / React integration status [Observed]

| Item | Status |
|---|---|
| Versions | tauri 2.12.5, tauri-build 2.7.4, wry 0.57.1, tao 0.37.2, `@tauri-apps/api` and `@tauri-apps/cli` 2.12.4 (aligned per `tauri info`) |
| Frontend | React 19.3.1, TypeScript 6.0.3 (strict, `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`), Tailwind 4.3.3, Vite 8.3.5 |
| IPC | One command, `get_app_info`, behind an app-command manifest. The capability `main-window` grants only `allow-get-app-info` to window `main`. No core, plugin or event permissions. `withGlobalTauri: false`; `freezePrototype: true` |
| CSP | Production `script-src 'self'`, with no `unsafe-inline`, `unsafe-eval` or remote sources. A separate `devCsp` allows only what Vite HMR needs on 127.0.0.1 |
| Generated types in React | `AppInfo` and the related types are imported from `src/bindings`. `parseAppInfo` rebuilds the object field by field against the generated types (so drift breaks compilation) and rejects extra fields and unknown enum values at runtime |
| UI states | Loading → Ready only when the call succeeds **and** the data matches the contract; otherwise Failed (with code and Retry) |
| Builds | `tauri build --no-bundle` release (8,679,936 B) and `--debug` (12,387,840 B): exit 0. Installers disabled (`bundle.active: false`) |

## 7. IPC test results [Observed]

- **Real IPC path, mock runtime with the app's real config and ACL** (9 tests, PASS): `get_app_info` returns a value that deserializes to `AppInfo`. Rejected: unknown commands ("Command not found"), uninstalled plugins (shell, fs, http, process: "Plugin not found"), un-granted core commands ("not allowed … core:window:allow-close"), another window, and non-local origins.
- **Frontend** (Vitest via Tauri's official `mockIPC`): Loading, Ready, Failed (Rust error), Failed (Tauri string rejection), Failed (contract violation), Retry. The only command sent is `get_app_info`.
- **Contract** (Rust + TypeScript on the same fixtures): the valid fixture equals Rust's serialization and TypeScript accepts it unchanged. All 5 invalid fixtures are rejected by both sides.
- **Live:** `ipc.get_app_info result=ok` 0.42–0.45 s after launch. With fault injection: `result=err code=injectedFault` and the UI shows Failed.

## 8. Windows native runtime evidence [Observed]

| Check | Result | Evidence (`docs/qa/evidence/BOOT-01/windows/`) |
|---|---|---|
| WIN-BOOT-01 starts | PASS | `release-run1.summary.json` |
| WIN-BOOT-02 renders in WebView2 | PASS (visually inspected) | `release-run1.initial.png` |
| WIN-BOOT-03 typed IPC data shown | PASS | `release-run1.*` |
| WIN-BOOT-04 invalid IPC exposes nothing | PASS (mock runtime with the real ACL; no in-WebView injection) | `../rust/test.log` |
| WIN-BOOT-05 resize / DPI | PASS at 100%; 125%/150% NOT RUN | `release-run1.resize-*.png` |
| WIN-BOOT-06 clean close, no orphans | PASS (4 of 4 runs) | `*.summary.json` |
| WIN-BOOT-07 subsequent start | PASS | `release-run2.summary.json` |
| WIN-BOOT-08 failure shows an error state | PASS | `debug-fault.initial.png` |
| Dev launch (`npm run dev`) | PASS: ready 17.3 s; Vite 127.0.0.1:1420 only; tree fully exited | `dev-launch.*` |

## 9. Static checks and test counts [Observed]

| Gate | Command | Result |
|---|---|---|
| Rust format | `cargo fmt --all -- --check` | PASS. The first run failed on hand-written formatting; fixed with `cargo fmt` |
| Rust check | `cargo check --workspace --all-targets --locked` | PASS |
| Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS (0) |
| Rust tests | `cargo test --workspace --locked` | **51 passed / 0 failed / 0 ignored** |
| TypeScript | `tsc --noEmit` | PASS (0 errors) |
| ESLint | `eslint . --max-warnings=0` | PASS (0) |
| Vitest | `vitest run` | **18 passed / 0 failed** (3 files) |
| Frontend build | `vite build` | PASS; CSS byte-identical across rebuilds |
| Bindings drift | `npm run bindings:check` | PASS (11 files) |
| All gates | `npm run check` | 9 of 9 PASS (`evidence/BOOT-01/check-all.log`) |

No coverage percentages are claimed; no coverage tool is installed.

## 10. Dependency / license findings [Observed]

- **Direct dependencies:**
  - Rust: 6 crates, all MIT and/or Apache-2.0.
  - npm: 3 runtime packages (MIT; Apache-2.0 OR MIT) and 17 dev packages (MIT; Apache-2.0 for TypeScript). None deprecated.
  - `npm audit`: 0 vulnerabilities.
- **Transitive inventory** (`npm run licenses`):
  - Rust linked into the app: 129 crates. 113 use preferred licenses, 15 other permissive, and **1 is flagged** (option-ext, MPL-2.0, via tauri → dirs → dirs-sys).
  - Rust build-time only: 92 crates, with 4 MPL-2.0 (cssparser, cssparser-macros, selectors and dtoa-short, via tauri-utils → dom_query; not linked into the exe).
  - Rust dev/test only: 24.
  - npm in the bundle: 4, all preferred.
  - npm dev tooling: 230, with 5 flagged (lightningcss MPL-2.0, caniuse-lite CC-BY-4.0).
- **No GPL, AGPL or LGPL in any scope.** The flagged items need legal review before any distribution; none was rejected, because each is part of Tauri's or Tailwind's own toolchain.
- **Not run:** `cargo audit` and `cargo deny` (installation not approved).

## 11. Security boundary verification [Observed]

| Requirement | Evidence | Result |
|---|---|---|
| No remote input capability | No input code, no input crate dependencies, `tp-input` is Foundation; the core test asserts it | PASS |
| No screen capture capability | No capture code; no screen-recording permission requested; the only screenshots were QA captures of the app's own window | PASS |
| No unsolicited network listener | 0 TCP/UDP endpoints owned by the app tree in 5 live runs; the Vite dev server binds to 127.0.0.1 only (dev mode); no firewall rule created or prompted | PASS |
| Minimal Tauri permissions | One capability with one permission; ACL tests; no plugins; no `devtools` or `test` feature in the app build | PASS |
| No privileged or background process | Medium integrity, not elevated; no services, scheduled tasks or Run/Startup entries; no process survives close | PASS |
| No shell, filesystem or global hooks | No plugins; `plugin:shell` and `plugin:fs` rejected; no hook code | PASS |
| Fault injection cannot reach release | Compiled under `cfg(debug_assertions)`; release run with the variable set returned `ok` | PASS |
| Peer-text sanitisation (protocol) | Errors truncate to 32 printable characters (test); diagnostics escaping prevents line and field injection (test) | PASS |

## 12. Files created or modified [Observed]

**Created:**
- Root: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `rustfmt.toml`, `package.json`, `package-lock.json`, `.gitignore`, `README.md`, `LICENSE.txt`.
- `crates/**`: 12 crates.
- `apps/desktop/**`: UI, shell, configuration, icons, tests, generated bindings.
- `tests/**`: fixtures and README.
- `scripts/**`: 4 scripts.
- Docs: `docs/reports/BOOT-01-report.md`, `docs/qa/BOOT-01-test-results.md`, `docs/qa/BOOT-01-environment.md`, `docs/architecture/BOOT-01-dependency-map.md`, `docs/qa/evidence/BOOT-01/**`.

**Modified (append-only):**
- `docs/architecture/ADR-001-stack.md`: §10 amendment A1.
- `docs/architecture/ADR-002-media-transport.md`: §9 amendment A1.
- `docs/architecture/ARCH-00.md`: status note after the §2.2 table. **Out-of-scope note:** the prompt asked to update ADRs only. This one non-destructive note was added so ARCH-00 doesn't present superseded recommendations without comment. Remove it if unwanted.

**Generated, outside the repo or ignored:**
- `target/`, `node_modules/`, `apps/desktop/dist/`, `apps/desktop/src-tauri/gen/schemas/`.
- WebView2 profile `%LOCALAPPDATA%\local.tpremote.desktop\EBWebView`.
- Scratch files in the session scratchpad: rustup download, link test.

**Removed:** only files this phase had just generated (mobile and Store icon sets from `tauri icon`; temporary negative-control probes).

## 13. Commands executed and results [Observed]

| Group | Commands | Result |
|---|---|---|
| Preflight (read-only) | `Get-ChildItem`/`Get-FileHash` on docs; CIM OS/CPU/GPU; `node`/`npm`/`git` versions; `Get-Command` for tools; `vswhere -requires …VC.Tools…`, `-find …link.exe`; SDK and WebView2 registry; shell elevation; port 1420; `Get-ExecutionPolicy -List` | As in BOOT-01-environment §1 |
| Research (read-only) | WebFetch rustup.rs, v2.tauri.app/start/prerequisites; `npm view` versions, peerDeps, engines, licenses, deprecations; `cargo info` tauri, tauri@2, tauri-build@2, ts-rs, serde, serde_json, thiserror | Compatibility decisions in the dependency map §3 |
| Install (approved) | Download and hash-verify rustup-init → `rustup-init.exe -y … --profile minimal --component rustfmt --component clippy` | Exit 0, rustc 1.99.1 |
| Toolchain verification | `rustc -vV`, `cargo -V`, `rustup show`, `cargo fmt/clippy --version`, rustc link test | All OK |
| Project dependencies | `cargo fetch`; `npm install --save-exact -w @tp-remote/desktop …` (2 runs) | 240 packages audited, 0 vulnerabilities |
| API verification | Read the ts-rs 12.0.1, tauri 2.12.5, tauri-build 2.7.4, tauri-codegen 2.7.4 and `@tauri-apps/api` mocks sources | APIs confirmed before use |
| Generation | `tp-bindings generate apps/desktop/src/bindings`; `npx tauri icon app-icon.svg` | 11 binding files; desktop icons |
| Gates | `cargo fmt/check/test/clippy`; `npm run typecheck/lint/test/build`; `npm run check` | §9 |
| Native builds | `npm run build:app`, `npm run build:app:debug` (×2 after the CSS fix) | Exit 0 |
| Runtime | `smoke-windows.ps1` (release ×3 incl. fault env, debug fault); `smoke-dev-windows.ps1` | §8 |
| Investigation | `cargo tree -i …`, `-e features,normal,build`; `npx tauri info`; `npm audit`; `node scripts/license-report.mjs` | §10, §11 |
| Negative controls | Boundary violation; bindings drift (edit + stray file); restricted import | All caught, all reverted |
| Post-state (read-only) | Firewall rules, Run/RunOnce, Startup, scheduled tasks, services, app data, leftover processes and ports | Clean |

Failures observed along the way, with their fixes, are in BOOT-01-test-results §5:
- the IPC test exe crashed with `STATUS_ENTRYPOINT_NOT_FOUND`;
- `fmt --check` failed on hand-written code;
- ESLint reported 5 errors;
- the smoke script hit a StrictMode error and left PID 504 running, which was closed with WM_CLOSE;
- Tailwind's CSS output was nondeterministic.

## 14. Remaining risks and blockers

| Item | Type | Detail |
|---|---|---|
| No Mac | Blocker for CAP-02's macOS half (R-02) | macOS is not built or tested. CAP-02's exit gate needs both operating systems |
| No second Windows device | Blocker for NET-03 two-device tests (R-03) | — |
| U-08 minimum OS versions | Decision needed before CAP-02 | Affects the ScreenCaptureKit and WebCodecs baseline |
| Scope deferred from PROMPT-PLAN BOOT-01 | Scope decision | Session state machine and timers (T-UNI-01 … 06), redaction layer (T-UNI-18), bounded queues (T-UNI-22), synthetic capture and fakes (T-INT-01), diagnostics snapshot (T-INT-13), ARCH-00 §4 trait definitions, Playwright skeleton (T-UI-01), cargo-deny/audit. **[Recommendation]** schedule them as a BOOT-02 (pure logic, no OS access) before NET-03 |
| License review | Legal | option-ext (MPL-2.0) is linked; the build-time MPL crates and CC-BY data; the legal owner of TP Remote is unconfirmed |
| Non-100% DPI untested | Test gap | Requires changing display settings or a second device |
| WIN-BOOT-04 not exercised inside the live WebView | Test gap | ACL proven on the mock runtime with the real config |
| Tauri default `dynamic-acl` feature enabled | Hardening | It is unused. **[Recommendation]** evaluate disabling Tauri default features in a later phase |
| An external `SetWindowPos` bypasses the 720×560 minimum | Observation | The layout still works at 500×400 |
| No git repository | Traceability | Evidence cannot reference commit hashes. **[Recommendation]** approve `git init` plus an initial commit by you |
| `npm run dev` overwrites `target\debug\tp-desktop.exe` with the dev-mode build | Workflow | Documented in the README |
| `LongPathsEnabled = 0`; user profile path contains spaces | Environment | No problem observed in BOOT-01 **[Assumption: may matter for deeper toolchains later]** |

## 15. Verdict

**PASS** for BOOT-01 (Windows-only foundation). Against the exit gate:
- **Architecture:** boundaries respected and tested, no cycles. The only out-of-scope change is the ARCH-00 note in §12.
- **Build:** Rust, React/TS and native Windows Tauri builds all PASS.
- **Runtime:** the app launches, shows actual Rust IPC data, and closes cleanly.
- **Testing:** fmt, cargo tests, clippy, typecheck, frontend tests and contract tests all PASS.
- **Security:** no input, no capture, no listener, minimal permissions, no privileged process.
- **Evidence:** commands and outputs are recorded; runtime behaviour is verified; limitations are documented.

Caveats that do **not** fail the gate but must not be overlooked:
- DPI was verified at 100% only.
- WIN-BOOT-04 was verified on the mock runtime.
- T-STA-05 and T-STA-06 are partial (no cargo-audit or cargo-deny).
- The PROMPT-PLAN BOOT-01 items above were deferred.
- **macOS: untested.**

## 16. Readiness for CAP-02

**[Observed]** The Windows foundation, contracts crate, platform-crate slots (`tp-capture`, `tp-security` OS permissions), tests and smoke tooling are in place for CAP-02 to build on.

**[Recommendation]** Before CAP-02 starts:
1. Review and approve this report.
2. Decide U-08 (minimum macOS and Windows versions).
3. Make a Mac available, or accept that CAP-02 can end PARTIAL.
4. Decide whether the deferred items become BOOT-02.
5. Approve the CAP-02 dependencies once proposed (`windows`, `objc2-*` crates).
6. Optionally approve `git init`.

## 17. Recommended next action

**[Recommendation]** Review BOOT-01, then run a short **BOOT-02** before CAP-02. BOOT-02 would contain the deferred items in §14: the session state machine with property tests, the redaction layer, the ARCH-00 §4 contract traits with fakes, and an optional cargo-deny gate. All of it is pure logic, testable on this Windows host, and needs no OS permissions. In parallel, decide U-08 and arrange macOS hardware for CAP-02.

**STOP.** CAP-02 and BOOT-02 have not been started.

---

## Erratum (added in BOOT-02, 2026-10-28)

§14 ("No git repository") and §16–17 (the recommendation to approve `git init`) rest on an unverified statement. The workspace has been a git repository since commit `3166183` (2026-10-28 11:16:57 +07:00), before BOOT-01 began. BOOT-01 was later committed as `7a9ae99` and pushed to `origin/main`, outside the assistant's actions. See BOOT-01-environment §7. No BOOT-01 test result is affected.
