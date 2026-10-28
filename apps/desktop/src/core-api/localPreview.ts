import { invoke } from "@tauri-apps/api/core";

import type { CaptureDisplays, LocalPreviewStatus } from "../bindings";
import { CoreApiError, toCoreApiError } from "./errors";
import { decodePreviewFrame, parseCaptureDisplays, parseLocalPreviewStatus, type PreviewFrame } from "./parseLocalPreview";

/**
 * CAP-02A local self-preview commands (ADR-007). None of them can name a capture target:
 * `start_local_preview` only opens the Windows capture picker, and capture starts only
 * when the local user selects something there.
 */
export const PREVIEW_COMMANDS = {
  displays: "get_capture_displays",
  status: "get_local_preview_status",
  start: "start_local_preview",
  stop: "stop_local_preview",
  frame: "get_local_preview_frame",
} as const;

async function call(command: string, args?: Record<string, unknown>): Promise<unknown> {
  try {
    return await invoke<unknown>(command, args);
  } catch (error) {
    throw toCoreApiError(error);
  }
}

export async function getCaptureDisplays(): Promise<CaptureDisplays> {
  return parseCaptureDisplays(await call(PREVIEW_COMMANDS.displays));
}

export async function getLocalPreviewStatus(): Promise<LocalPreviewStatus> {
  return parseLocalPreviewStatus(await call(PREVIEW_COMMANDS.status));
}

/** Opens the Windows capture picker. Capture starts only if the user selects a target. */
export async function startLocalPreview(): Promise<LocalPreviewStatus> {
  return parseLocalPreviewStatus(await call(PREVIEW_COMMANDS.start));
}

export async function stopLocalPreview(): Promise<LocalPreviewStatus> {
  return parseLocalPreviewStatus(await call(PREVIEW_COMMANDS.stop));
}

/**
 * The latest preview frame of `generation`, or `null` if there is no new one. `painted`
 * is the number of frames this UI has drawn (reported for preview-rate measurements).
 */
export async function getPreviewFrame(generation: number, painted: number): Promise<PreviewFrame | null> {
  const raw = await call(PREVIEW_COMMANDS.frame, { generation, painted });
  if (!(raw instanceof ArrayBuffer)) {
    throw new CoreApiError("invalidResponse", "The native core returned a preview frame that is not binary.");
  }
  return decodePreviewFrame(raw);
}

export type { PreviewFrame };
