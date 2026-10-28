# Session 1 (functional, app level): operator notes

| Field | Value |
|---|---|
| Date | 2026-10-28 |
| App | `target\release\tp-desktop.exe`, PID 2068 (SHA-256 in `session1-functional.summary.json`) |
| Test pattern | `tp-test-pattern.exe`, PID 22352, topmost, on DISPLAY1 |
| Script | `scripts\capture-qa-windows.ps1 -Label session1-functional` |

## What the app recorded (`session1-functional.stderr.log`)

1. `preview.start_requested generation=1` at about 928 s into the session.
2. `preview.picker_cancelled generation=1` at about 939 s.
3. Nothing further. There was no second `start_requested`, no `target_selected`, no `capturing` and no `first_frame` for the rest of the session (until 1,935 s).

## What the operator reported

The operator was asked four times to start a preview of the test pattern:
- Each time, they reported that it worked: "All as described", "Done, preview is live", "Done, badge says Capturing", and "Clicked Start just now" (with a live log watch running).
- In each case, the app received **no** start request.

## Independent checks (read-only), at the time of the reports

| Check | Result |
|---|---|
| App threads (`GetThreadDescription`) | 21 threads: `main`, `MojoThread`, `EBWorkerThread`, 16 × `tokio-rt-worker`. **No** `tp-preview-start` or `tp-wgc-watchdog` thread: no capture run existed |
| Process CPU / GPU | 0–0.07 % CPU; GPU engine utilisation 0 (`session1-functional.samples.csv`) |
| Loaded modules | `GraphicsCapture.dll` loaded, consistent with the one picker that was opened and cancelled |
| Window state | Screenshot 1 (958 s) is 160×28: the TP Remote window was minimized. Screenshots 2 and 3 (1,203 s, 1,901 s) are byte-identical: the window content did not change between them (scrolled to the bottom) |
| Test-pattern window | Minimized at about 1,250 s, restored later |
| Last input in the session | `GetLastInputInfo`: 62 ms (someone was active on the console) |

## Conclusion

- **Recorded:** T-CAP-RUN-01 (picker appears) and T-CAP-SEC-02 (cancel captures nothing) are corroborated by the app log for generation 1. The cancel led to state Ready, no capture thread was ever started, and no grant was minted.
- **Not recorded as passed:** the reports of a live preview are not corroborated by the app, so T-CAP-RUN-02 … T-CAP-RUN-05 at app level are **NOT RUN**. The cause is unknown. The app log, the process threads and the unchanged screenshots all show that no start request reached this instance after the cancel.
- **Clean exit:** the session ended with `WM_CLOSE`. The app exited, no process of its tree survived, and nothing was force-stopped (`session1-functional.summary.json`).
