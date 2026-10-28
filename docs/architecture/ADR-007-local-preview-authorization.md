# ADR-007 — Local self-preview authorization and the capture target contract

| Field | Value |
|---|---|
| Status | **Accepted for CAP-02A**, pending reviewer approval of the phase |
| Date | 2026-10-28 |
| Phase | CAP-02A (Windows native capture and local preview) |
| Related | ADR-003 (session lifecycle), BOOT-02 security review §3 (trusted-consent boundary), ARCH-00 §4.2 (CaptureProvider), PROMPT-PLAN §4 (CAP-02) |
| Numbering | ADR-005 and ADR-006 stay reserved for NET-03 and WAN-07 (PROMPT-PLAN amendment, BOOT-02) |

## 1. Context

CAP-02A must show real Windows frames inside TP Remote. Two existing decisions are in the way.

1. **BOOT-02's only approval proof is remote-session consent.** `tp_security::LocalConsent` authorizes an *authenticated remote session* (`ConsentGrant` → `authorize()` → `ViewScreen` / `ControlInput`). It has no production constructor by design; the trusted consent screen is future work (BOOT-02 security review §3). Using it for a local preview would need either a fake constructor or a fake remote session. Both are forbidden, and both would blur the boundary that keeps remote viewing default-deny.
2. **The BOOT-02 capture contract assumed programmatic targeting.** `CaptureConfig { display: DisplayId, .. }` lets the caller name a display. CAP-02A requires that "a real user must select the capture target" in the Windows picker, and forbids claiming automatic monitor targeting before target-to-monitor mapping is validated.

PROMPT-PLAN §4 planned a *dev-only* preview. The CAP-02A prompt supersedes that: the preview ships in the normal build, behind a trusted selection boundary.

**Contradiction:** the requested preview cannot be authorized by anything that exists in BOOT-02 without weakening it. This ADR resolves that with the smallest amendment possible.

## 2. Decision

### 2.1 Local self-preview is a separate, local-only concept

- It has **its own lifecycle**: `tp_core::preview::PreviewMachine` (`NotAvailable`, `Ready`, `SelectingTarget`, `Initializing`, `Capturing`, `Stopping`, `Stopped`, `Failed`). It is *not* a session of the remote state machine (ADR-003), and has no peer, consent, transport or input.
- It has **its own grant**: `tp_core::preview::LocalPreviewGrant`.
  - It is minted in exactly one place: the picker-completion path, when the OS picker returns a selection for the *current* request generation.
  - It authorizes one thing only: showing local preview frames of that generation.
  - It is not `Clone`, not serializable, has private fields, and has no conversion to `tp_security::ConsentGrant`. Compile-fail doctests prove all of this.
  - It is revoked (dropped) when the run stops, and the preview buffer is cleared at the same moment.
- **Remote authorization is unchanged.** `platform_capabilities()` still returns `NONE`, `authorize()` never sees a preview grant, and an active preview leaves remote `ViewScreen` and `ControlInput` denied (tested).

### 2.2 The Windows capture picker is the trusted local selection boundary

- The IPC command `start_local_preview` **only opens the picker**. It is not consent, and it cannot name a target, because no IPC argument identifies a display or window.
- Windows draws the picker. The WebView cannot see, script or operate it, and only a selection by the local user produces a `GraphicsCaptureItem`.
  - Cancelling produces nothing, and nothing is captured.
  - A selection that arrives after a stop, or for an older request, is stale. Its target is released without being captured.
- The picker is shown only over a live window owned by this process. Before using the handle, the provider checks it with `IsWindow` and that the window's process id matches TP Remote's.
- In production, a capture target can only come from the picker. Programmatic targets (`CreateForWindow`, and `CreateForMonitor` used to build a target) exist only behind the `tp-capture/test-support` feature, which the app's build graph never enables. Two places enforce this: boundary tests and a `cargo tree` guard.
- The only other use of `CreateForMonitor` in production code reads monitor names for classification (§2.4). Those items never reach a capture session, and capture sessions are created in exactly one audited place.

### 2.3 Capture contract amendment (tp-contracts `capture`)

| Change | Reason |
|---|---|
| `CaptureProvider::Target` (opaque, `Send`); `start(target, config, sink)` | Targets come from the OS picker, not from a `DisplayId` |
| New `CapturePicker: CaptureProvider` trait with `pick(owner, done)`, `PickOutcome {Selected, Cancelled, Failed}`, and `PickerOwner` | Models the OS picker on both platforms (Windows `GraphicsCapturePicker`, macOS `SCContentSharingPicker`) |
| `CaptureConfig` loses `display` and `output_size` | Covered by `Target`; scaling happens downstream (preview readback now, a GPU video processor in VID-04) |
| `FrameMeta.generation`; `display` removed | Frames are attributed to a run; the target belongs to the stream |
| `TargetInfo { kind, label, size, display }` and `CaptureStream::{target, generation}` | Honest description of what was picked |
| `CaptureEvent::{TargetResized, TargetClosed, DeviceLost}` | Real WGC lifecycle events |
| `FrameReadback::read_rgba` | A bounded CPU copy for the local preview only |
| `DisplayInfo.{scale_percent, refresh_hz}` | Reported by Windows; `None` if unknown |
| `CaptureCapabilities.system_picker` | Advertises whether the trusted picker exists |

### 2.4 Target identity is reported honestly

- `DisplayId` is an index within one layout epoch, not a hardware identity.
- WGC names monitors differently from GDI. Observed on DEV-W1, `DISPLAY1` is `Display 2`, `DISPLAY2` is `Display 1`, and `DISPLAY3` is `Display 3`.
- A picked item is reported as a display only when its name and size match exactly one monitor's own WGC name. Validated on all three DEV-W1 monitors.
- Anything else is `Unknown`, which is usually a window. The classification is informational and no security decision depends on it.

### 2.5 Preview data path

- **What is copied:** the frames themselves stay GPU textures, released right away.
  - A frame is copied to the CPU only when the UI has taken the previous preview frame, and at most 30 times per second.
  - The copy goes through a reused staging texture, is area-averaged to ≤ 1280×720 RGBA, and is stored in a single slot (latest wins).
- **How it reaches the UI:** the WebView pulls the slot with `get_local_preview_frame` through Tauri's IPC protocol, with no socket involved, and draws it to a canvas.
- **Retention:** nothing is written to disk or logged. Frames carry their generation, and no frame is served without a live grant.

### 2.6 Capture indicator

- The Windows yellow border is never disabled. No code touches the border property or requests borderless access, and a boundary test enforces this.
- The UI shows a "Capturing" badge, a Stop button that is always available, and "No capture is active" once capture ends.

## 3. Alternatives considered

| Alternative | Why rejected |
|---|---|
| Reuse `LocalConsent` with a fake local session | Forbidden (fake approval); widens a remote-session proof to a local feature |
| An in-app "Allow capture?" dialog, then `CreateForMonitor` | A WebView-rendered dialog is not a trusted boundary (BOOT-02 §3); the monitor mapping was unvalidated |
| A native Win32 confirmation box, then programmatic capture | Weaker than the OS picker (the app still chooses the target); duplicates OS UX |
| Dev-only preview (PROMPT-PLAN §4) | Superseded by the CAP-02A scope |
| Push frames to the UI with a Tauri channel | Needs separate backpressure; the pull model bounds work to what the UI can draw |
| Serve frames from a local HTTP server | Forbidden (ARCH-00 §3.5: no localhost servers) |

## 4. Consequences

- **Positive:**
  - Remote default-deny is untouched, and the local feature has its own minimal proof.
  - The contract now fits both OS pickers.
  - Stale and cancelled selections cannot capture.
- **Negative / risks:**
  - Script running in the main WebView (for example after XSS) could open the picker repeatedly. A user selection is still required, and the start is rejected while one is in progress.
  - Preview pixels live in WebView memory while shown.
  - Window titles appear in the local UI, though never in logs.
  - `GraphicsCaptureItem.Closed` did not fire on DEV-W1 for a destroyed window, so a 250 ms liveness watchdog backs it up.
- **Future:**
  - CAP-02B maps `CapturePicker` to `SCContentSharingPicker`.
  - NET-03/VID-04 will need host-side programmatic display capture *after* remote consent. That needs a validated display mapping and a separate ADR. This ADR does not authorize it.
