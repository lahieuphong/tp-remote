import { describe, expect, it } from "vitest";

import headerFixture from "../../../../tests/fixtures/ipc/preview/preview-frame-header.json";
import type { LocalPreviewStatus } from "../bindings";
import { CoreApiError } from "../core-api/errors";
import { decodePreviewFrame, parseCaptureDisplays, parseLocalPreviewStatus } from "../core-api/parseLocalPreview";
import { emptyFrame, previewFixtures } from "../test/previewFixtures";

// Same fixture files as crates/tp-contracts/tests/preview_fixtures.rs.
const invalidFixtures = import.meta.glob<unknown>("../../../../tests/fixtures/ipc/preview/invalid/*.json", {
  eager: true,
  import: "default",
});

function expectInvalid(run: () => unknown): void {
  try {
    run();
  } catch (error) {
    expect(error).toBeInstanceOf(CoreApiError);
    expect((error as CoreApiError).code).toBe("invalidResponse");
    return;
  }
  throw new Error("expected an invalidResponse error");
}

describe("Local preview contract (Rust ↔ TypeScript)", () => {
  it("accepts every status fixture that Rust serializes, unchanged", () => {
    for (const fixture of [previewFixtures.capturing, previewFixtures.failed, previewFixtures.ready]) {
      const parsed: LocalPreviewStatus = parseLocalPreviewStatus(fixture);
      expect(parsed).toEqual(fixture);
    }
  });

  it("accepts the DEV-W1 display layout, including negative coordinates and nulls", () => {
    const parsed = parseCaptureDisplays(previewFixtures.displays);
    expect(parsed).toEqual(previewFixtures.displays);
    expect(parsed.displays[1]?.x).toBe(-3840);
    expect(parsed.displays[2]?.scalePercent).toBeNull();
  });

  it("rejects every invalid fixture that Rust also rejects", () => {
    const names = Object.keys(invalidFixtures).sort();
    expect(names.map((n) => n.split("/").pop())).toEqual([
      "bad-nested-enum.json",
      "bad-state.json",
      "missing-field.json",
      "negative-count.json",
      "unknown-field.json",
      "wrong-type.json",
    ]);
    for (const name of names) {
      expectInvalid(() => parseLocalPreviewStatus(invalidFixtures[name]));
    }
    for (const raw of [null, [], "status", 1]) {
      expectInvalid(() => parseLocalPreviewStatus(raw));
      expectInvalid(() => parseCaptureDisplays(raw));
    }
  });

  it("decodes the frame header fixture exactly as Rust encodes it", () => {
    const { width, height } = headerFixture;
    const buffer = new Uint8Array(24 + width * height * 4);
    buffer.set(headerFixture.bytes);
    const frame = decodePreviewFrame(buffer.buffer);
    expect(frame).not.toBeNull();
    expect(frame?.generation).toBe(headerFixture.generation);
    expect(frame?.seq).toBe(headerFixture.seq);
    expect(frame?.width).toBe(1280);
    expect(frame?.height).toBe(720);
    expect(frame?.pixels.length).toBe(1280 * 720 * 4);
  });

  it("treats an empty header as no frame and rejects malformed frames", () => {
    expect(decodePreviewFrame(emptyFrame())).toBeNull();
    const valid = new Uint8Array(24 + 4);
    valid.set(headerFixture.bytes.slice(0, 12));
    new DataView(valid.buffer).setUint32(12, 1, true);
    new DataView(valid.buffer).setUint32(16, 1, true);
    expect(decodePreviewFrame(valid.buffer)?.pixels.length).toBe(4);

    expectInvalid(() => decodePreviewFrame(new ArrayBuffer(10)));
    const badMagic = new Uint8Array(valid);
    badMagic[0] = 0x58;
    expectInvalid(() => decodePreviewFrame(badMagic.buffer));
    expectInvalid(() => decodePreviewFrame(new Uint8Array([...valid, 0]).buffer));
    const emptyWithData = new Uint8Array(28);
    emptyWithData.set(headerFixture.bytes.slice(0, 4));
    expectInvalid(() => decodePreviewFrame(emptyWithData.buffer));
  });
});
