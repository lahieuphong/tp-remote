import { mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import validFixture from "../../../../tests/fixtures/ipc/app-info.valid.json";
import { previewDefault } from "../test/previewFixtures";
import { App } from "./App";

/**
 * Records every IPC command the UI sends. `handler` answers get_app_info; the CAP-02A
 * preview commands get their ready-state fixtures (LocalPreviewCard.test.tsx covers them).
 */
function recordCommands(handler: (cmd: string) => unknown): string[] {
  const calls: string[] = [];
  mockIPC((cmd) => {
    calls.push(cmd);
    return cmd === "get_app_info" ? handler(cmd) : previewDefault(cmd);
  });
  return calls;
}

/** Commands other than the preview card's own (start, stop and frames need a click). */
const PREVIEW_MOUNT = new Set(["get_local_preview_status", "get_capture_displays"]);

function appInfoCalls(calls: string[]): string[] {
  expect(calls.filter((c) => c !== "get_app_info" && !PREVIEW_MOUNT.has(c))).toEqual([]);
  return calls.filter((c) => c === "get_app_info");
}

/** Tauri rejects failed commands with the serialized error value, not an Error. */
function tauriRejection(value: unknown): Promise<never> {
  // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- reproduces Tauri's behaviour
  return Promise.reject(value);
}

/** The core status card. Since CAP-02A the preview card shows its own Ready badge too. */
function coreStatus(): HTMLElement {
  return screen.getByRole("status");
}

function statusLabel(): string {
  return screen.getByTestId("core-status-label").textContent;
}

describe("App: native core status", () => {
  it("shows Loading while get_app_info is pending, and no data", () => {
    recordCommands(() => new Promise(() => undefined));
    render(<App />);

    expect(statusLabel()).toBe("Loading");
    expect(screen.getByRole("status").dataset.status).toBe("loading");
    expect(within(coreStatus()).queryByText("Ready")).toBeNull();
    expect(screen.queryByTestId("diagnostics")).toBeNull();
    expect(screen.queryByTestId("engine-modules")).toBeNull();
  });

  it("shows Ready with the values returned by Rust", async () => {
    const calls = recordCommands(() => validFixture);
    render(<App />);

    expect(await within(coreStatus()).findByText("Ready")).toBeDefined();
    const diagnostics = screen.getByTestId("diagnostics");
    expect(within(diagnostics).getByText("0.1.0")).toBeDefined();
    expect(within(diagnostics).getByText("windows (windows)")).toBeDefined();
    expect(within(diagnostics).getByText("x86_64")).toBeDefined();
    expect(within(diagnostics).getByText("v1 (supports v1–v1)")).toBeDefined();
    expect(within(diagnostics).getByText("155.0.4283.62")).toBeDefined();
    const modules = screen.getByTestId("engine-modules");
    expect(within(modules).getByText("tp-capture")).toBeDefined();
    expect(within(modules).getByText("CAP-02")).toBeDefined();
    expect(screen.getByText("reported by Rust core: remoteFeatures = notImplemented")).toBeDefined();
    expect(appInfoCalls(calls)).toEqual(["get_app_info"]);
  });

  it("shows Failed with the error from Rust when the command rejects", async () => {
    recordCommands(() =>
      tauriRejection({ code: "injectedFault", message: "get_app_info failed: fault injected for testing" }),
    );
    render(<App />);

    expect(await within(coreStatus()).findByText("Failed")).toBeDefined();
    expect(screen.getByTestId("core-error-message").textContent).toBe(
      "get_app_info failed: fault injected for testing",
    );
    expect(screen.getByText("code: injectedFault")).toBeDefined();
    expect(within(coreStatus()).queryByText("Ready")).toBeNull();
    expect(screen.queryByTestId("diagnostics")).toBeNull();
  });

  it("shows Failed when Tauri rejects the call itself", async () => {
    recordCommands(() => tauriRejection("Command get_app_info not allowed by ACL"));
    render(<App />);

    expect(await within(coreStatus()).findByText("Failed")).toBeDefined();
    expect(screen.getByText("code: ipcRejected")).toBeDefined();
  });

  it("shows Failed, not Ready, when the response violates the contract", async () => {
    recordCommands(() => ({ ...validFixture, protocol: { current: "1", minSupported: 1, maxSupported: 1 } }));
    render(<App />);

    expect(await within(coreStatus()).findByText("Failed")).toBeDefined();
    expect(screen.getByText("code: invalidResponse")).toBeDefined();
    expect(within(coreStatus()).queryByText("Ready")).toBeNull();
    expect(screen.queryByTestId("diagnostics")).toBeNull();
  });

  it("Retry calls the core again and can reach Ready", async () => {
    let attempt = 0;
    const calls = recordCommands(() => {
      attempt += 1;
      return attempt === 1
        ? tauriRejection({ code: "internal", message: "first attempt failed" })
        : validFixture;
    });
    render(<App />);

    expect(await within(coreStatus()).findByText("Failed")).toBeDefined();
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    });
    expect(statusLabel()).toBe("Loading");
    expect(await within(coreStatus()).findByText("Ready")).toBeDefined();
    expect(appInfoCalls(calls)).toEqual(["get_app_info", "get_app_info"]);
  });

  it("always states that remote features are not implemented", () => {
    recordCommands(() => new Promise(() => undefined));
    render(<App />);
    expect(screen.getByText("Remote features not implemented")).toBeDefined();
  });
});
