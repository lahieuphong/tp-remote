import type {
  CaptureAvailability,
  CaptureDisplay,
  CaptureDisplays,
  LocalPreviewState,
  LocalPreviewStatus,
  PreviewError,
  PreviewOutcome,
  PreviewStats,
  PreviewTarget,
  PreviewTargetKind,
} from "../bindings";
import { CoreApiError } from "./errors";
import {
  ContractViolation,
  array,
  boolean,
  exact,
  i32,
  nullableOneOf,
  nullableU32,
  object,
  oneOf,
  rate,
  string,
  u32,
} from "./validate";

/** Runtime validation of the CAP-02A local-preview responses (same rules as AppInfo). */

const AVAILABILITY: Record<CaptureAvailability, true> = {
  available: true,
  notImplemented: true,
  unsupported: true,
};
const STATES: Record<LocalPreviewState, true> = {
  notAvailable: true,
  ready: true,
  selectingTarget: true,
  initializing: true,
  capturing: true,
  stopping: true,
  stopped: true,
  failed: true,
};
const TARGET_KINDS: Record<PreviewTargetKind, true> = { display: true, unknown: true };
const OUTCOMES: Record<PreviewOutcome, true> = {
  cancelled: true,
  stoppedByUser: true,
  targetClosed: true,
  deviceLost: true,
  failed: true,
};

function contractError(contract: string, error: unknown): never {
  if (error instanceof ContractViolation) {
    throw new CoreApiError(
      "invalidResponse",
      `The native core returned data that does not match the ${contract} contract: ${error.message}.`,
    );
  }
  throw error;
}

function target(value: unknown, path: string): PreviewTarget | null {
  if (value === null) return null;
  const o = object(value, path);
  return exact(
    o,
    {
      kind: oneOf(o, "kind", path, TARGET_KINDS),
      label: string(o, "label", path),
      width: u32(o, "width", path),
      height: u32(o, "height", path),
      displayIndex: nullableU32(o, "displayIndex", path),
    },
    path,
  );
}

function stats(value: unknown, path: string): PreviewStats {
  const o = object(value, path);
  return exact(
    o,
    {
      capturedFrames: u32(o, "capturedFrames", path),
      previewedFrames: u32(o, "previewedFrames", path),
      skippedFrames: u32(o, "skippedFrames", path),
      droppedFrames: u32(o, "droppedFrames", path),
      captureFps: rate(o, "captureFps", path),
      previewFps: rate(o, "previewFps", path),
      elapsedMs: u32(o, "elapsedMs", path),
      firstFrameMs: nullableU32(o, "firstFrameMs", path),
      frameWidth: u32(o, "frameWidth", path),
      frameHeight: u32(o, "frameHeight", path),
      previewWidth: u32(o, "previewWidth", path),
      previewHeight: u32(o, "previewHeight", path),
    },
    path,
  );
}

function previewError(value: unknown, path: string): PreviewError | null {
  if (value === null) return null;
  const o = object(value, path);
  return exact(o, { message: string(o, "message", path), recovery: string(o, "recovery", path) }, path);
}

export function parseLocalPreviewStatus(raw: unknown): LocalPreviewStatus {
  const path = "localPreviewStatus";
  try {
    const o = object(raw, path);
    const status: LocalPreviewStatus = {
      availability: oneOf(o, "availability", path, AVAILABILITY),
      state: oneOf(o, "state", path, STATES),
      generation: u32(o, "generation", path),
      target: target(o.target, `${path}.target`),
      stats: stats(o.stats, `${path}.stats`),
      lastOutcome: nullableOneOf(o, "lastOutcome", path, OUTCOMES),
      error: previewError(o.error, `${path}.error`),
    };
    return exact(o, status, path);
  } catch (error) {
    return contractError("LocalPreviewStatus", error);
  }
}

function display(value: unknown, path: string): CaptureDisplay {
  const o = object(value, path);
  return exact(
    o,
    {
      index: u32(o, "index", path),
      name: string(o, "name", path),
      x: i32(o, "x", path),
      y: i32(o, "y", path),
      width: u32(o, "width", path),
      height: u32(o, "height", path),
      scalePercent: nullableU32(o, "scalePercent", path),
      refreshHz: nullableU32(o, "refreshHz", path),
      primary: boolean(o, "primary", path),
    },
    path,
  );
}

export function parseCaptureDisplays(raw: unknown): CaptureDisplays {
  const path = "captureDisplays";
  try {
    const o = object(raw, path);
    const displays: CaptureDisplays = {
      epoch: u32(o, "epoch", path),
      displays: array(o, "displays", path).map((d, i) => display(d, `${path}.displays[${String(i)}]`)),
    };
    return exact(o, displays, path);
  } catch (error) {
    return contractError("CaptureDisplays", error);
  }
}

/** Binary frame layout (tp_contracts::ipc::preview_frame). */
export const FRAME_MAGIC = [0x54, 0x50, 0x46, 0x31] as const; // "TPF1"
export const FRAME_HEADER_LEN = 24;

export interface PreviewFrame {
  generation: number;
  seq: number;
  width: number;
  height: number;
  /** RGBA8, row-major, width × height × 4 bytes. Never stored or sent anywhere. */
  pixels: Uint8ClampedArray<ArrayBuffer>;
}

/** Decodes a frame response. Returns `null` for "no new frame"; throws on malformed data. */
export function decodePreviewFrame(buffer: ArrayBuffer): PreviewFrame | null {
  const invalid = (reason: string): never => {
    throw new CoreApiError("invalidResponse", `The native core returned an invalid preview frame: ${reason}.`);
  };
  if (buffer.byteLength < FRAME_HEADER_LEN) invalid("too short");
  const view = new DataView(buffer);
  FRAME_MAGIC.forEach((byte, i) => {
    if (view.getUint8(i) !== byte) invalid("bad magic");
  });
  const generation = view.getUint32(4, true);
  const seq = view.getUint32(8, true);
  const width = view.getUint32(12, true);
  const height = view.getUint32(16, true);
  if (width === 0 || height === 0) {
    if (buffer.byteLength !== FRAME_HEADER_LEN) invalid("empty frame with pixel data");
    return null;
  }
  if (buffer.byteLength !== FRAME_HEADER_LEN + width * height * 4) invalid("length does not match size");
  return { generation, seq, width, height, pixels: new Uint8ClampedArray(buffer, FRAME_HEADER_LEN) };
}
