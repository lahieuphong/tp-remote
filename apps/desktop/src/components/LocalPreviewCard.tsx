import type { CaptureDisplays, LocalPreviewState, LocalPreviewStatus, PreviewOutcome } from "../bindings";
import { useLocalPreview } from "../app/useLocalPreview";
import { Card } from "./Card";

const STATE_LABELS: Record<LocalPreviewState, string> = {
  notAvailable: "Not available",
  ready: "Ready",
  selectingTarget: "Selecting target",
  initializing: "Initializing",
  capturing: "Capturing",
  stopping: "Stopping",
  stopped: "Stopped",
  failed: "Failed",
};

const OUTCOME_TEXT: Record<PreviewOutcome, string> = {
  cancelled: "The picker was closed without a selection. Nothing was captured.",
  stoppedByUser: "Capture stopped.",
  targetClosed: "The captured window or display went away, so capture stopped.",
  deviceLost: "The graphics device was reset, so capture stopped.",
  failed: "Capture failed.",
};

const CAN_START: readonly LocalPreviewState[] = ["ready", "stopped", "failed"];
const CAN_STOP: readonly LocalPreviewState[] = ["selectingTarget", "initializing", "capturing"];

function duration(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  return `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

function fps(value: number): string {
  return value > 0 ? value.toFixed(1) : "–";
}

function targetText(status: LocalPreviewStatus, displays: CaptureDisplays | null): string | null {
  const target = status.target;
  if (!target) return null;
  if (target.kind === "display") {
    const device = target.displayIndex === null ? undefined : displays?.displays[target.displayIndex]?.name;
    return device ? `${target.label} (${device})` : target.label;
  }
  return `Window or other item: ${target.label}`;
}

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="rounded-lg border border-neutral-800 px-3 py-2">
      <dt className="text-xs text-neutral-400">{label}</dt>
      <dd className="font-mono text-sm text-neutral-100">{value}</dd>
      {hint && <dd className="text-[11px] text-neutral-500">{hint}</dd>}
    </div>
  );
}

export function LocalPreviewCard({ className = "" }: { className?: string }) {
  const { status, displays, error, drawnFps, drawnFrames, start, stop, canvasRef } = useLocalPreview();
  const state = status?.state ?? null;
  const capturing = state === "capturing" || state === "stopping";

  return (
    <Card title="Local screen preview" labelledBy="local-preview-title" className={className}>
      <p className="text-sm leading-relaxed text-neutral-400">
        Preview your own screen on this computer only. Windows asks you which display or window to show; nothing is
        sent over the network, recorded or saved.
      </p>

      <div className="mt-4 flex flex-wrap items-center gap-3">
        <span
          data-testid="preview-state"
          data-state={state ?? "loading"}
          className={`rounded-md border px-2 py-1 font-mono text-xs ${
            state === "capturing" ? "border-red-700 text-red-300" : "border-neutral-700 text-neutral-300"
          }`}
        >
          {state === "capturing" && <span aria-hidden className="mr-1 inline-block size-2 rounded-full bg-red-500" />}
          {state ? STATE_LABELS[state] : "Loading"}
        </span>
        <button
          type="button"
          onClick={start}
          disabled={!state || !CAN_START.includes(state)}
          className="rounded-md bg-teal-700 px-3 py-1.5 text-sm font-medium text-white hover:bg-teal-600 disabled:cursor-not-allowed disabled:opacity-40"
        >
          Start local preview
        </button>
        <button
          type="button"
          onClick={stop}
          disabled={!state || !CAN_STOP.includes(state)}
          className="rounded-md border border-red-700 px-3 py-1.5 text-sm font-medium text-red-200 hover:bg-red-950 disabled:cursor-not-allowed disabled:opacity-40"
        >
          Stop capture
        </button>
      </div>

      {state === "notAvailable" && (
        <p className="mt-3 text-sm text-amber-300">Screen capture is not available on this system.</p>
      )}
      {state === "selectingTarget" && (
        <p className="mt-3 text-sm text-neutral-300">
          Choose a display or window in the Windows picker. Close the picker to capture nothing.
        </p>
      )}
      {state === "capturing" && (
        <p className="mt-3 text-sm text-red-200">
          Capturing. Windows draws a yellow border around the captured item while capture is active.
        </p>
      )}

      {status?.error && (
        <div role="alert" className="mt-3 rounded-lg border border-red-900 bg-red-950/40 p-3 text-sm">
          <p className="text-red-200">{status.error.message}</p>
          <p className="mt-1 text-red-300/80">{status.error.recovery}</p>
        </div>
      )}
      {error && (
        <div role="alert" className="mt-3 rounded-lg border border-red-900 bg-red-950/40 p-3 text-sm">
          <p className="text-red-200">{error.message}</p>
          <p className="mt-1 font-mono text-xs text-red-300/80">code: {error.code}</p>
        </div>
      )}

      <div className="mt-4 overflow-hidden rounded-lg border border-neutral-800 bg-black">
        {capturing ? (
          <canvas
            ref={canvasRef}
            data-testid="preview-canvas"
            aria-label="Live preview of the captured display or window"
            className="mx-auto block h-auto max-h-[480px] w-full object-contain"
          />
        ) : (
          <p data-testid="preview-idle" className="px-4 py-10 text-center text-sm text-neutral-400">
            No capture is active.
            {status?.lastOutcome && <span className="mt-1 block text-neutral-500">{OUTCOME_TEXT[status.lastOutcome]}</span>}
          </p>
        )}
      </div>

      {status && capturing && (
        <>
          <p data-testid="preview-target" className="mt-3 text-sm text-neutral-300">
            {targetText(status, displays)} · {status.target?.width ?? 0}×{status.target?.height ?? 0}
          </p>
          {status.target?.kind === "display" && (
            <p className="mt-1 text-xs text-neutral-500">
              If this window is on the captured display, the preview shows itself (a mirror effect). That is expected.
            </p>
          )}
          <dl data-testid="preview-stats" className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-4">
            <Stat label="Capture FPS" value={fps(status.stats.captureFps)} hint="frames from Windows" />
            <Stat label="Preview FPS" value={fps(drawnFps)} hint="frames drawn here" />
            <Stat
              label="Resolution"
              value={`${String(status.stats.frameWidth)}×${String(status.stats.frameHeight)}`}
              hint={`preview ${String(status.stats.previewWidth)}×${String(status.stats.previewHeight)}`}
            />
            <Stat label="Elapsed" value={duration(status.stats.elapsedMs)} />
            <Stat label="Frames captured" value={String(status.stats.capturedFrames)} />
            <Stat label="Frames previewed" value={String(status.stats.previewedFrames)} />
            <Stat label="Frames drawn" value={String(drawnFrames)} hint="in this window" />
            <Stat
              label="Skipped / dropped"
              value={`${String(status.stats.skippedFrames)} / ${String(status.stats.droppedFrames)}`}
              hint="UI busy / discarded"
            />
            <Stat
              label="First frame"
              value={status.stats.firstFrameMs === null ? "–" : `${String(status.stats.firstFrameMs)} ms`}
            />
          </dl>
        </>
      )}

      {displays && displays.displays.length > 0 && (
        <details className="mt-4 text-sm">
          <summary className="cursor-pointer text-neutral-400">
            Displays reported by Windows ({displays.displays.length})
          </summary>
          <table data-testid="capture-displays" className="mt-2 w-full text-left font-mono text-xs text-neutral-300">
            <thead className="text-neutral-500">
              <tr>
                <th className="py-1 font-normal">#</th>
                <th className="font-normal">Name</th>
                <th className="font-normal">Position</th>
                <th className="font-normal">Size</th>
                <th className="font-normal">Scale</th>
                <th className="font-normal">Hz</th>
                <th className="font-normal">Primary</th>
              </tr>
            </thead>
            <tbody>
              {displays.displays.map((d) => (
                <tr key={d.index} className="border-t border-neutral-800">
                  <td className="py-1">{d.index}</td>
                  <td>{d.name}</td>
                  <td>
                    {d.x}, {d.y}
                  </td>
                  <td>
                    {d.width}×{d.height}
                  </td>
                  <td>{d.scalePercent === null ? "–" : `${String(d.scalePercent)}%`}</td>
                  <td>{d.refreshHz ?? "–"}</td>
                  <td>{d.primary ? "yes" : ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      )}
    </Card>
  );
}
