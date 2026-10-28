import type { IpcErrorCode } from "../bindings";

/** Every way a call into the native core can fail, as seen by the UI. */
export type CoreApiErrorCode =
  | IpcErrorCode
  /** Tauri rejected the call itself, e.g. an unknown or unpermitted command. */
  | "ipcRejected"
  /** The core answered, but the data does not match the generated contract. */
  | "invalidResponse"
  | "unknown";

export class CoreApiError extends Error {
  readonly code: CoreApiErrorCode;

  constructor(code: CoreApiErrorCode, message: string) {
    super(message);
    this.name = "CoreApiError";
    this.code = code;
  }
}

// A Record over the generated union fails to compile if Rust adds or removes a code.
const IPC_ERROR_CODES: Record<IpcErrorCode, true> = {
  internal: true,
  injectedFault: true,
  invalidState: true,
  unavailable: true,
  forbidden: true,
};

function isIpcErrorCode(value: unknown): value is IpcErrorCode {
  return typeof value === "string" && Object.hasOwn(IPC_ERROR_CODES, value);
}

/** Normalises anything thrown by `invoke` into a CoreApiError. */
export function toCoreApiError(error: unknown): CoreApiError {
  if (error instanceof CoreApiError) {
    return error;
  }
  if (typeof error === "object" && error !== null) {
    const { code, message } = error as { code?: unknown; message?: unknown };
    if (isIpcErrorCode(code) && typeof message === "string") {
      return new CoreApiError(code, message);
    }
  }
  if (typeof error === "string") {
    return new CoreApiError("ipcRejected", error);
  }
  if (error instanceof Error) {
    return new CoreApiError("unknown", error.message);
  }
  return new CoreApiError("unknown", "The native core failed with an unrecognised error.");
}
