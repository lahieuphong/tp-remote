# ADR-004 — Operating-System Support Policy (U-08) — PROVISIONAL

| Field | Value |
|---|---|
| Status | **PROVISIONAL.** Proposed by the user in BOOT-02. Stays provisional until macOS runtime checks (CAP-02) and the compatibility review are done |
| Date | 2026-10-28 |
| Decides | U-08 (ARCH-00 §2.2) |
| Certified by BOOT-02 | **Nothing beyond Windows 11 build 26100 on DEV-W1.** BOOT-02 tests are Windows-only and engine-level |

## 1. Policy (proposed)

| Tier | Windows | macOS |
|---|---|---|
| **Primary** | Windows 11 x64, version **24H2** (build 26100) or newer | macOS **15 Sequoia** or newer, Apple Silicon |
| **Legacy candidates (not supported unless decided)** | Windows 10 x64 22H2 | macOS 13 Ventura, macOS 14 Sonoma |

Windows 10 standard support ended on **2025-10-14**.

## 2. Technical API minimums (checked against vendor metadata during BOOT-02)

| API / component | Minimum | Source | Status |
|---|---|---|---|
| ScreenCaptureKit framework | macOS **12.3** | Apple documentation metadata (`screencapturekit.json`, `metadata.platforms`) | Verified as documentation, not tested |
| `SCContentSharingPicker` | macOS **14.0** | Apple documentation metadata (`sccontentsharingpicker.json`) | Verified as documentation |
| WGC `IGraphicsCaptureItemInterop::CreateForMonitor` | Windows 10 **1903** (build 18362) | Microsoft Learn, "Minimum supported client" | Verified as documentation |
| `GraphicsCaptureSession.IsBorderRequired` | Windows build **20348** | Microsoft Learn (ARCH-00 research) | Documentation |
| WebView2 Runtime | Windows 10/11 | Tauri v2 "Webview Versions" | Documentation |
| WebCodecs in WKWebView | **UNKNOWN** | ARCH-00 FS-F | Needs FS-F2 on a Mac |
| Tauri 2 minimum macOS version | **UNVERIFIED** | Not checked in BOOT-02 | Check before CAP-02 |

None of these APIs is used by BOOT-02 code. Their minimums are recorded so the product minimums above are not set below what the APIs support.

**Reading.** Every primary target is above every documented API minimum listed. macOS 13 sits above ScreenCaptureKit's 12.3 minimum but below `SCContentSharingPicker`'s 14.0. Supporting macOS 13 would therefore require the non-picker capture path (FS-A). This is a technical consequence, not a claim that macOS 13 cannot capture.

## 3. Product support minimums vs. technical minimums

- The **product minimum** is the primary tier in §1. Only versions that are still receiving vendor security updates may be product targets.
- A **technical minimum** (§2) is the lowest version where an API exists. It does not make that version a supported product target.

## 4. Security-support rule

- TP Remote is security-sensitive (remote control). Shipping it for an OS without vendor security updates is **not allowed** without an explicit, recorded user decision.
- This applies to Windows 10 22H2 after 2025-10-14, unless it is covered by Extended Security Updates, and to any macOS release Apple no longer patches.
- Legacy candidates stay candidates until such a decision, and until they pass the same real-device matrix as the primary tier.

## 5. Required test environments

| ID | Environment | Purpose | Available |
|---|---|---|---|
| DEV-W1 | Windows 11 Enterprise LTSC 2024, build 26100 (24H2 code base), x64 | Primary Windows | **Yes** |
| DEV-W2 | Second Windows 11 24H2+ device, ideally 150% scaling | Win↔Win, mixed DPI | No |
| DEV-M1 | macOS 15+ on Apple Silicon, Retina | Primary macOS | No |
| DEV-M2 | Second macOS 15+ device | Mac↔Mac | No |
| (legacy) | Windows 10 22H2 VM / device; macOS 13–14 device | Only if a legacy candidate is promoted | No |

## 6. Unsupported or unverified combinations

| Combination | Status |
|---|---|
| Any macOS version | **UNVERIFIED**: no macOS run has ever happened |
| Windows 11 below 24H2, Windows 10 | Unsupported (legacy candidate for Windows 10 22H2 only) |
| Windows on ARM64, Intel Macs | Not considered (ARCH-00 D-15); undecided |
| Display scaling other than 100% on Windows | UNVERIFIED (BOOT-01 tested 100% only) |
| macOS 13 with picker-based capture | Not possible (`SCContentSharingPicker` requires 14.0) |

## 7. Revisit triggers

- First macOS runtime results (CAP-02).
- A user decision on the legacy candidates.
- A vendor ends security support for a primary-tier version.
