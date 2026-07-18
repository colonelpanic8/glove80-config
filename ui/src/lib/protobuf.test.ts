import { describe, expect, it } from "vitest";

import {
  ApplyResult,
  decodeStudioResponse,
  encodeClear,
  encodeGetCapabilities,
  encodeSetEffects,
  encodeSetPixels,
  EffectType,
} from "./protobuf";

describe("host-lighting protobuf codec", () => {
  it("encodes the Studio envelope and custom subsystem field", () => {
    expect([...encodeGetCapabilities(7)]).toEqual([0x08, 0x07, 0x32, 0x02, 0x08, 0x01]);
    expect([...encodeClear(9)]).toEqual([0x08, 0x09, 0x32, 0x02, 0x18, 0x01]);
  });

  it("encodes bounded pixel updates", () => {
    expect([...encodeSetPixels(3, [{ index: 40, rgb: 0x12ab34 }], true, 5000)]).toEqual([
      0x08, 0x03, 0x32, 0x0f, 0x12, 0x0d, 0x0a, 0x06, 0x08, 0x28, 0x10, 0xb4,
      0xd6, 0x4a, 0x10, 0x01, 0x18, 0x88, 0x27,
    ]);
  });

  it("encodes per-key effects", () => {
    expect([
      ...encodeSetEffects(
        4,
        [
          {
            index: 40,
            rgb: 0x12ab34,
            type: EffectType.Breathe,
            periodMs: 1500,
            phaseMs: 100,
            dutyPercent: 50,
          },
        ],
        true,
        5000,
      ),
    ]).toEqual([
      0x08, 0x04, 0x32, 0x18, 0x22, 0x16, 0x0a, 0x0f, 0x08, 0x28, 0x10, 0xb4,
      0xd6, 0x4a, 0x18, 0x02, 0x20, 0xdc, 0x0b, 0x28, 0x64, 0x30, 0x32, 0x10,
      0x01, 0x18, 0x88, 0x27,
    ]);
  });

  it("decodes capability and apply responses", () => {
    const capabilities = Uint8Array.from([
      0x0a, 0x1d,
      0x08, 0x02, 0x10, 0x50, 0x18, 0x28, 0x20, 0x08, 0x28, 0x14,
      0x30, 0x88, 0x27, 0x38, 0xb0, 0xea, 0x01, 0x40, 0x60,
      0x58, 0x01, 0x60, 0xc8, 0x01, 0x68, 0x90, 0x4e, 0x70, 0x32,
    ]);
    const envelope = Uint8Array.from([
      0x0a, capabilities.length + 4, 0x08, 0x2a, 0x32, capabilities.length, ...capabilities,
    ]);
    const decoded = decodeStudioResponse(envelope);
    expect(decoded.kind).toBe("capabilities");
    if (decoded.kind === "capabilities") {
      expect(decoded.requestId).toBe(42);
      expect(decoded.capabilities).toMatchObject({
        protocolVersion: 2,
        pixelCount: 80,
        pixelsPerHalf: 40,
        maxUpdatesPerRequest: 8,
        maxUpdateHz: 20,
        maxChannelValue: 96,
        supportsEffects: true,
        minEffectPeriodMs: 200,
        maxEffectPeriodMs: 10000,
        effectTimeQuantumMs: 50,
      });
    }

    expect(decodeStudioResponse(Uint8Array.from([0x0a, 0x06, 0x08, 0x05, 0x32, 0x02, 0x10, 0x02]))).toEqual({
      requestId: 5,
      kind: "setPixels",
      result: ApplyResult.Partial,
    });
    expect(
      decodeStudioResponse(
        Uint8Array.from([0x0a, 0x06, 0x08, 0x06, 0x32, 0x02, 0x20, 0x05]),
      ),
    ).toEqual({
      requestId: 6,
      kind: "setEffects",
      result: ApplyResult.InvalidEffect,
    });
  });
});
