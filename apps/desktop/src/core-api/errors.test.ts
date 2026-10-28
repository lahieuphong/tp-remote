import { describe, expect, it } from "vitest";

import { CoreApiError, toCoreApiError } from "./errors";

describe("toCoreApiError", () => {
  it("keeps codes and messages of IpcError values from Rust", () => {
    const e = toCoreApiError({ code: "injectedFault", message: "boom" });
    expect(e).toBeInstanceOf(CoreApiError);
    expect(e.code).toBe("injectedFault");
    expect(e.message).toBe("boom");
  });

  it("treats string rejections as Tauri rejecting the call", () => {
    const e = toCoreApiError("command launch_session not found");
    expect(e.code).toBe("ipcRejected");
    expect(e.message).toBe("command launch_session not found");
  });

  it("does not trust codes outside the generated IpcErrorCode union", () => {
    const e = toCoreApiError({ code: "admin", message: "granted" });
    expect(e.code).toBe("unknown");
    expect(e.message).not.toBe("granted");
  });

  it("does not accept inherited properties as error codes", () => {
    const e = toCoreApiError({ code: "toString", message: "x" });
    expect(e.code).toBe("unknown");
  });

  it("maps thrown Errors and other values to unknown", () => {
    expect(toCoreApiError(new TypeError("no IPC")).code).toBe("unknown");
    expect(toCoreApiError(42).code).toBe("unknown");
  });

  it("passes CoreApiError through unchanged", () => {
    const original = new CoreApiError("invalidResponse", "bad");
    expect(toCoreApiError(original)).toBe(original);
  });
});
