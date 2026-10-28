import { mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { previewFixtures, solidFrame } from "../test/previewFixtures";
import { LocalPreviewCard } from "./LocalPreviewCard";

/** Tauri rejects failed commands with the serialized error value, not an Error. */
function tauriRejection(value: unknown): Promise<never> {
  // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- reproduces Tauri's behaviour
  return Promise.reject(value);
}

interface Backend {
  status: unknown;
  calls: { cmd: string; args: unknown }[];
  onStart?: () => unknown;
  onStop?: () => unknown;
}

function backend(status: unknown): Backend {
  const state: Backend = { status, calls: [] };
  mockIPC((cmd, args) => {
    state.calls.push({ cmd, args });
    switch (cmd) {
      case "get_capture_displays":
        return previewFixtures.displays;
      case "start_local_preview":
        return state.onStart ? state.onStart() : state.status;
      case "stop_local_preview":
        return state.onStop ? state.onStop() : state.status;
      case "get_local_preview_frame":
        return solidFrame(3, 1, 4, 2);
      default:
        return state.status;
    }
  });
  return state;
}

function stateBadge(): HTMLElement {
  return screen.getByTestId("preview-state");
}

const putImageData = vi.fn();

beforeEach(() => {
  putImageData.mockReset();
  // jsdom has no canvas; a minimal 2D context records what the preview draws.
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(
    () => ({ putImageData }) as unknown as CanvasRenderingContext2D,
  );
  vi.stubGlobal(
    "ImageData",
    class {
      constructor(
        readonly data: Uint8ClampedArray,
        readonly width: number,
        readonly height: number,
      ) {}
    },
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("LocalPreviewCard", () => {
  it("shows Not available and disables Start when capture is unsupported", async () => {
    backend({ ...previewFixtures.ready, availability: "unsupported", state: "notAvailable" });
    render(<LocalPreviewCard />);
    expect(await screen.findByText("Screen capture is not available on this system.")).toBeDefined();
    expect(stateBadge().textContent).toBe("Not available");
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Start local preview" }).disabled).toBe(true);
  });

  it("Start only opens the picker: nothing is captured or drawn until the user selects", async () => {
    const api = backend(previewFixtures.ready);
    api.onStart = () => {
      api.status = { ...previewFixtures.ready, state: "selectingTarget", generation: 1 };
      return api.status;
    };
    render(<LocalPreviewCard />);
    expect(await screen.findByText("No capture is active.")).toBeDefined();

    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Start local preview" }));
    });
    expect(await screen.findByText(/Choose a display or window in the Windows picker/)).toBeDefined();
    expect(stateBadge().textContent).toBe("Selecting target");
    expect(api.calls.map((c) => c.cmd)).toContain("start_local_preview");
    expect(api.calls.some((c) => c.cmd === "get_local_preview_frame")).toBe(false);
    expect(screen.queryByTestId("preview-canvas")).toBeNull();
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Stop capture" }).disabled).toBe(false);
  });

  it("while capturing, shows the target and keeps capture FPS and preview FPS apart", async () => {
    const api = backend(previewFixtures.capturing);
    render(<LocalPreviewCard />);
    expect(await screen.findByTestId("preview-canvas")).toBeDefined();
    expect(stateBadge().textContent).toBe("Capturing");
    expect(screen.getByText(/Windows draws a yellow border/)).toBeDefined();
    expect(await screen.findByText("Display 2 (DISPLAY1) · 1920×1080", { exact: false })).toBeDefined();

    const stats = screen.getByTestId("preview-stats");
    const captureFps = within(stats).getByText("Capture FPS").parentElement;
    const previewFps = within(stats).getByText("Preview FPS").parentElement;
    expect(captureFps?.textContent).toContain("59.9");
    expect(captureFps?.textContent).toContain("frames from Windows");
    expect(previewFps?.textContent).toContain("frames drawn here");
    expect(previewFps?.textContent).not.toContain("59.9");
    expect(within(stats).getByText("1920×1080")).toBeDefined();
    expect(within(stats).getByText("00:30")).toBeDefined();

    await vi.waitFor(() => {
      expect(putImageData).toHaveBeenCalled();
    });
    const frameCalls = api.calls.filter((c) => c.cmd === "get_local_preview_frame");
    expect(frameCalls[0]?.args).toEqual({ generation: 3, painted: 0 });
  });

  it("Stop capture stops and then states that no capture is active", async () => {
    const api = backend(previewFixtures.capturing);
    api.onStop = () => {
      api.status = { ...previewFixtures.ready, state: "stopped", generation: 3, lastOutcome: "stoppedByUser" };
      return api.status;
    };
    render(<LocalPreviewCard />);
    expect(await screen.findByTestId("preview-canvas")).toBeDefined();
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Stop capture" }));
    });
    expect(await screen.findByText("No capture is active.")).toBeDefined();
    expect(screen.getByText("Capture stopped.")).toBeDefined();
    expect(screen.queryByTestId("preview-canvas")).toBeNull();
    expect(stateBadge().textContent).toBe("Stopped");
    expect(api.calls.map((c) => c.cmd)).toContain("stop_local_preview");
  });

  it("reports a cancelled picker as nothing captured", async () => {
    backend({ ...previewFixtures.ready, generation: 2, lastOutcome: "cancelled" });
    render(<LocalPreviewCard />);
    expect(await screen.findByText(/Nothing was captured/)).toBeDefined();
  });

  it("shows the failure message and the recovery step", async () => {
    backend(previewFixtures.failed);
    render(<LocalPreviewCard />);
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("The graphics device was reset, so capture stopped.")).toBeDefined();
    expect(within(alert).getByText("Start the preview again.")).toBeDefined();
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Start local preview" }).disabled).toBe(false);
  });

  it("shows a rejected start with its error code", async () => {
    const api = backend(previewFixtures.ready);
    api.onStart = () => tauriRejection({ code: "invalidState", message: "A preview is already being started or is running." });
    render(<LocalPreviewCard />);
    expect(await screen.findByText("No capture is active.")).toBeDefined();
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Start local preview" }));
    });
    expect(await screen.findByText("code: invalidState")).toBeDefined();
  });

  it("lists the displays Windows reports, including negative coordinates", async () => {
    backend(previewFixtures.ready);
    render(<LocalPreviewCard />);
    const table = await screen.findByTestId("capture-displays");
    expect(within(table).getByText("-3840, 71")).toBeDefined();
    expect(within(table).getAllByRole("row")).toHaveLength(4);
  });
});
