# BOOT-01 — Environment Record

| Field | Value |
|---|---|
| Phase | BOOT-01 |
| Date | 2026-10-28 |
| Host | DEV-W1 (TEST-STRATEGY §3) |
| Method | Read-only checks first, then one approved toolchain install, then re-checks |

All values below were **observed** on DEV-W1 during this phase. Nothing is copied from the ARCH-00 report without being re-checked.

## 1. Pre-flight (read-only, before any change)

| Item | Observed | ARCH-00 said | Match |
|---|---|---|---|
| ARCH-00 documents | All 8 present. Sizes and timestamps identical to their creation in ARCH-00; SHA-256 prefixes recorded (e.g. ARCH-00.md `DD1F1B1A3304C034`) | — | ✔ |
| OS | Microsoft Windows 11 Enterprise LTSC 10.0.26100, build 26100, 64-bit | same | ✔ |
| CPU | 12th Gen Intel Core i9-12900K | same | ✔ |
| GPUs | Intel UHD Graphics 770 (32.0.101.7088); NVIDIA GeForce RTX 3090 (32.0.16.1088) | same | ✔ |
| Displays | DISPLAY1 (−1920,71) 1920×1080; DISPLAY2 (−3840,71) 1920×1080; DISPLAY3 (0,0) 1920×1080 primary | same | ✔ |
| Node.js / npm | 24.19.0 / 11.16.0 | same | ✔ |
| Git | 2.53.0.windows.2 (workspace is not a git repository) | same | ✔ |
| Rust (`rustc`, `cargo`, `rustup`) | **Missing**; no `%USERPROFILE%\.cargo` or `.rustup` | missing | ✔ |
| MSVC | VS Build Tools 2022 17.14.37111.16, complete; `link.exe` at `…\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\link.exe` | same | ✔ |
| Windows SDK | 10.0.26100 | same | ✔ |
| WebView2 Runtime | 155.0.4283.62 | same | ✔ |
| Shell elevation | **Not elevated** (so anything launched from it runs unelevated) | — | — |
| Port 1420 | Free | — | — |
| Long paths (`LongPathsEnabled`) | 0 (disabled). No path problems were observed in this phase | — | — |
| PowerShell execution policy | CurrentUser `RemoteSigned`. Local scripts run without `-ExecutionPolicy Bypass` | — | — |
| winget | Missing (LTSC). The Tauri-suggested `winget install Rustlang.Rustup` was therefore not available | missing | ✔ |

## 2. Approved installation (system-level change)

The user approved this explicitly in BOOT-01: "Install, add to PATH (Recommended)".

| Step | Observed |
|---|---|
| Source | `https://win.rustup.rs/x86_64`, the official link on rustup.rs (it answered HTTP 200 directly). Binary downloaded from `https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe` into an empty scratch folder outside the project |
| Integrity | SHA-256 `00F139DA614DCED87D3C3D4121C43EEBDAC3C1BF8A5EE86B9D80DFD71C8D70FD`, **matching** the official `rustup-init.exe.sha256` |
| Authenticode | `NotSigned` (observed; rustup-init.exe carries no Authenticode signature) |
| Command | `rustup-init.exe -y --default-host x86_64-pc-windows-msvc --default-toolchain stable --profile minimal --component rustfmt --component clippy` |
| Result | Exit 0. "stable-x86_64-pc-windows-msvc installed - rustc 1.99.1 (58cf8f9c3 2026-10-13)" |
| Side effects | Files under `C:\Users\LA VIET PHONG\.rustup` and `.cargo`. **User** PATH (HKCU) now includes `%USERPROFILE%\.cargo\bin`. No admin rights used; Visual Studio was not changed |

Nothing else was installed system-wide. Specifically **not** installed: pnpm, Yarn, protoc, CMake, cargo-deny, cargo-audit, nightly Rust, Playwright browsers, Docker services, WebView2 updates.

## 3. Toolchain after installation

| Tool | Observed |
|---|---|
| `rustc --version --verbose` | rustc 1.99.1 (58cf8f9c3 2026-10-13), host `x86_64-pc-windows-msvc`, LLVM 23.1.1 |
| `cargo --version` | cargo 1.99.1 (5f94df478 2026-08-27) |
| `rustup --version` / `rustup show` | rustup 1.29.2; default and active toolchain `stable-x86_64-pc-windows-msvc`; target `x86_64-pc-windows-msvc` |
| `cargo fmt --version` | rustfmt 1.10.0-stable |
| `cargo clippy --version` | clippy 0.1.99 |
| MSVC linker | Verified by compiling and running a test program: `rustc --edition 2024 main.rs` → exit 0, and the binary printed "msvc link ok" |
| Project pin | `rust-toolchain.toml`: channel 1.99.1, profile minimal, rustfmt + clippy, target x86_64-pc-windows-msvc. `tauri info` reports "overridden by rust-toolchain.toml" |

## 4. `tauri info` (observed)

```text
OS: Windows 10.0.26100 x86_64 (X64)
✔ WebView2: 155.0.4283.62
✔ MSVC: Visual Studio Build Tools 2022
✔ rustc: 1.99.1 · ✔ cargo: 1.99.1 · ✔ rustup: 1.29.2
✔ Rust toolchain: 1.99.1-x86_64-pc-windows-msvc (overridden by rust-toolchain.toml)
node: 24.19.0 · npm: 11.16.0
tauri: 2.12.5 · tauri-build: 2.7.4 · wry: 0.57.1 · tao: 0.37.2
@tauri-apps/api: 2.12.4 · @tauri-apps/cli: 2.12.4
frontendDist: ../dist · devUrl: http://127.0.0.1:1420/ · framework: React · bundler: Vite
```

Full output: `docs/qa/evidence/BOOT-01/tauri-info.log`.

## 5. System state after BOOT-01 (read-only checks)

| Check | Observed |
|---|---|
| Firewall rules for TP Remote names or repo binaries | 0. No firewall change was made, and no Windows Firewall prompt was triggered, because no listener was opened (the Vite dev server binds to 127.0.0.1) |
| Autostart (HKCU Run/RunOnce, Startup folder, scheduled tasks, services) | 0 entries |
| Running `tp-desktop` processes / port 1420 listeners | 0 / 0 |
| App data | WebView2 user-data folder `%LOCALAPPDATA%\local.tpremote.desktop\EBWebView` (320 files, ~85 MB; no image or video files). This is WebView2's standard profile and cache, not screen frames |
| Processes stopped by force during the phase | None. Every app instance and dev tree exited after WM_CLOSE. The one instance left by a script failure (PID 504) was closed with WM_CLOSE, not killed |

## 6. Not verifiable on this host

- macOS (any check): no Mac.
- Non-100% display scaling: every display is at 96 DPI, and changing scaling would modify user display settings, which this phase did not do.
- Second-device or network behaviour: there is no network feature in BOOT-01.
