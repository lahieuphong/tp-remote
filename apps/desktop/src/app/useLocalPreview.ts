import { type RefObject, useCallback, useEffect, useRef, useState } from "react";

import type { CaptureDisplays, LocalPreviewState, LocalPreviewStatus } from "../bindings";
import { type CoreApiError, toCoreApiError } from "../core-api/errors";
import {
  getCaptureDisplays,
  getLocalPreviewStatus,
  getPreviewFrame,
  startLocalPreview,
  stopLocalPreview,
} from "../core-api/localPreview";

export interface LocalPreviewView {
  status: LocalPreviewStatus | null;
  displays: CaptureDisplays | null;
  /** The last failed command; cleared by the next start. */
  error: CoreApiError | null;
  /** Frames this window drew during the last second (measured; not the capture rate). */
  drawnFps: number;
  /** Frames this window has drawn in the current run. */
  drawnFrames: number;
  start: () => void;
  stop: () => void;
  canvasRef: RefObject<HTMLCanvasElement | null>;
}

const ACTIVE: readonly LocalPreviewState[] = ["selectingTarget", "initializing", "capturing", "stopping"];
const ACTIVE_POLL_MS = 250;
const IDLE_POLL_MS = 1000;

type Scheduled = { cancel: () => void };

function nextFrame(callback: () => void): Scheduled {
  if (typeof requestAnimationFrame === "function") {
    const id = requestAnimationFrame(callback);
    return { cancel: () => { cancelAnimationFrame(id); } };
  }
  const id = setTimeout(callback, 16);
  return { cancel: () => { clearTimeout(id); } };
}

/**
 * Drives the local self-preview: status polling, start/stop, and the frame loop. Frames
 * are pulled one at a time (the next request starts only after the previous frame was
 * drawn), so a slow UI never queues frames; they go straight to a canvas and are never
 * stored or sent anywhere.
 */
export function useLocalPreview(): LocalPreviewView {
  const [status, setStatus] = useState<LocalPreviewStatus | null>(null);
  const [displays, setDisplays] = useState<CaptureDisplays | null>(null);
  const [error, setError] = useState<CoreApiError | null>(null);
  const [drawnFps, setDrawnFps] = useState(0);
  const [drawnFrames, setDrawnFrames] = useState(0);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  const active = status !== null && ACTIVE.includes(status.state);
  const capturing = status?.state === "capturing";
  const generation = status?.generation ?? 0;

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      try {
        const next = await getLocalPreviewStatus();
        if (!cancelled) setStatus(next);
      } catch (e) {
        if (!cancelled) setError(toCoreApiError(e));
      }
      if (!cancelled) timer = setTimeout(() => void poll(), active ? ACTIVE_POLL_MS : IDLE_POLL_MS);
    };
    void poll();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [active]);

  useEffect(() => {
    let cancelled = false;
    getCaptureDisplays().then(
      (d) => {
        if (!cancelled) setDisplays(d);
      },
      (e: unknown) => {
        if (!cancelled) setError(toCoreApiError(e));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [generation]);

  useEffect(() => {
    if (!capturing) return;
    let cancelled = false;
    let painted = 0;
    const drawn: number[] = [];
    let scheduled: Scheduled | undefined;

    const tick = async () => {
      try {
        const frame = await getPreviewFrame(generation, painted);
        if (cancelled) return;
        const canvas = canvasRef.current;
        const context = canvas?.getContext("2d");
        if (frame && frame.generation === generation && canvas && context) {
          if (canvas.width !== frame.width) canvas.width = frame.width;
          if (canvas.height !== frame.height) canvas.height = frame.height;
          context.putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0);
          painted += 1;
          drawn.push(performance.now());
        }
      } catch (e) {
        if (!cancelled) setError(toCoreApiError(e));
        return;
      }
      // No await since the cancellation check above, so the loop is still live here.
      scheduled = nextFrame(() => void tick());
    };
    scheduled = nextFrame(() => void tick());

    const meter = setInterval(() => {
      const since = performance.now() - 1000;
      while ((drawn[0] ?? Infinity) < since) drawn.shift();
      setDrawnFps(drawn.length);
      setDrawnFrames(painted);
    }, 500);
    return () => {
      cancelled = true;
      scheduled?.cancel();
      clearInterval(meter);
    };
  }, [capturing, generation]);

  const start = useCallback(() => {
    setError(null);
    setDrawnFps(0);
    setDrawnFrames(0);
    startLocalPreview().then(setStatus, (e: unknown) => {
      setError(toCoreApiError(e));
    });
  }, []);

  const stop = useCallback(() => {
    stopLocalPreview().then(setStatus, (e: unknown) => {
      setError(toCoreApiError(e));
    });
  }, []);

  return {
    status,
    displays,
    error,
    drawnFps: capturing ? drawnFps : 0,
    drawnFrames: capturing ? drawnFrames : 0,
    start,
    stop,
    canvasRef,
  };
}
