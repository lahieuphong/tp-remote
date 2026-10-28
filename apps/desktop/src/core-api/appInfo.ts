import { invoke } from "@tauri-apps/api/core";

import type { AppInfo } from "../bindings";
import { toCoreApiError } from "./errors";
import { parseAppInfo } from "./parseAppInfo";

/** Name of the only IPC command BOOT-01 exposes. */
export const GET_APP_INFO = "get_app_info";

/** Asks the native Rust core what this build is. Rejects with a CoreApiError. */
export async function getAppInfo(): Promise<AppInfo> {
  let raw: unknown;
  try {
    raw = await invoke<unknown>(GET_APP_INFO);
  } catch (error) {
    throw toCoreApiError(error);
  }
  return parseAppInfo(raw);
}
