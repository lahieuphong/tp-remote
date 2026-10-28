import displays from "../../../../tests/fixtures/ipc/preview/capture-displays.dev-w1.json";
import capturing from "../../../../tests/fixtures/ipc/preview/local-preview-status.capturing.json";
import failed from "../../../../tests/fixtures/ipc/preview/local-preview-status.failed.json";
import ready from "../../../../tests/fixtures/ipc/preview/local-preview-status.ready.json";

/** Shared Rust/TypeScript fixtures for the CAP-02A preview commands (test-only). */
export const previewFixtures = { displays, capturing, failed, ready };

/** An empty preview-frame response ("no new frame"): the 24-byte header only. */
export function emptyFrame(): ArrayBuffer {
  const bytes = new Uint8Array(24);
  bytes.set([0x54, 0x50, 0x46, 0x31]);
  return bytes.buffer;
}

/** A width × height RGBA frame of one colour, with a valid header. */
export function solidFrame(generation: number, seq: number, width: number, height: number): ArrayBuffer {
  const buffer = new ArrayBuffer(24 + width * height * 4);
  const view = new DataView(buffer);
  [0x54, 0x50, 0x46, 0x31].forEach((b, i) => {
    view.setUint8(i, b);
  });
  view.setUint32(4, generation, true);
  view.setUint32(8, seq, true);
  view.setUint32(12, width, true);
  view.setUint32(16, height, true);
  new Uint8Array(buffer, 24).fill(128);
  return buffer;
}

/** Default answers for the preview commands, so other tests can ignore them. */
export function previewDefault(cmd: string): unknown {
  switch (cmd) {
    case "get_capture_displays":
      return displays;
    case "get_local_preview_frame":
      return emptyFrame();
    default:
      return ready;
  }
}
