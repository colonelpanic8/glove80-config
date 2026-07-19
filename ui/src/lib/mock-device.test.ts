import { describe, expect, it } from "vitest";

import {
  crc32,
  encodeLightingConfig,
  type Effect,
  type LightingConfig,
  type Request,
} from "./host-protocol";
import { MockKeyboard, MOCK_CAPABILITIES } from "./mock-device";

const SOLID_RED: Effect = { kind: "solid", r: 255, g: 0, b: 0, periodMs: 0, phaseMs: 0, dutyPercent: 0 };
const BLINK_BLUE: Effect = { kind: "blink", r: 0, g: 0, b: 255, periodMs: 1000, phaseMs: 0, dutyPercent: 50 };

function makeClock(start = 1_000) {
  let now = start;
  return { now: () => now, advance: (ms: number) => (now += ms) };
}

function send(keyboard: MockKeyboard, request: Request) {
  return keyboard.handle(7, request);
}

const SAMPLE_CONFIG: LightingConfig = {
  togglePersistMask: 0,
  toggleInitialState: 1 << 3,
  records: [
    { activation: { kind: "always" }, cells: [{ key: 0, effect: SOLID_RED }] },
    { activation: { kind: "toggle", id: 3 }, cells: [{ key: 79, effect: BLINK_BLUE }] },
  ],
};

function applyBlob(keyboard: MockKeyboard, blob: Uint8Array, blobCrc = crc32(blob)) {
  expect(send(keyboard, { command: "configBegin", totalLen: blob.length, blobCrc32: blobCrc }).status).toBe("ok");
  const chunk = 100;
  for (let offset = 0; offset < blob.length; offset += chunk) {
    const status = send(keyboard, {
      command: "configData",
      offset,
      data: blob.subarray(offset, Math.min(offset + chunk, blob.length)),
    }).status;
    expect(status).toBe("ok");
  }
  return send(keyboard, { command: "configCommit" });
}

describe("MockKeyboard capabilities", () => {
  it("answers GET_CAPABILITIES with the advertised capabilities", () => {
    const response = send(new MockKeyboard(), { command: "getCapabilities", clientMajor: 1, clientMinor: 1 });
    expect(response.status).toBe("ok");
    expect(response.payload).toMatchObject({ type: "capabilities", ...MOCK_CAPABILITIES });
  });

  it("rejects an unsupported client major version", () => {
    const response = send(new MockKeyboard(), { command: "getCapabilities", clientMajor: 2, clientMinor: 0 });
    expect(response.status).toBe("unsupportedVersion");
  });
});

describe("MockKeyboard overlay", () => {
  it("merges SET_CELLS and reads them back", () => {
    const keyboard = new MockKeyboard();
    send(keyboard, { command: "setCells", ttlMs: 0, cells: [{ key: 4, effect: SOLID_RED }] });
    send(keyboard, { command: "setCells", ttlMs: 0, cells: [{ key: 44, effect: BLINK_BLUE }] });
    const read = send(keyboard, { command: "readOverlay" });
    expect(read.payload).toEqual({
      type: "overlayState",
      cells: [
        { key: 4, effect: SOLID_RED, remainingTtlMs: 0 },
        { key: 44, effect: BLINK_BLUE, remainingTtlMs: 0 },
      ],
    });
  });

  it("rejects out-of-range keys and oversized batches", () => {
    const keyboard = new MockKeyboard();
    expect(send(keyboard, { command: "setCells", ttlMs: 0, cells: [{ key: 80, effect: SOLID_RED }] }).status).toBe("outOfRange");
    const big = Array.from({ length: 41 }, (_, key) => ({ key, effect: SOLID_RED }));
    expect(send(keyboard, { command: "setCells", ttlMs: 0, cells: big }).status).toBe("capacityExceeded");
    expect(keyboard.overlaySize()).toBe(0);
  });

  it("expires TTL cells on the injected clock", () => {
    const clock = makeClock();
    const keyboard = new MockKeyboard({ now: clock.now });
    send(keyboard, { command: "setCells", ttlMs: 500, cells: [{ key: 1, effect: SOLID_RED }] });
    send(keyboard, { command: "setCells", ttlMs: 0, cells: [{ key: 2, effect: SOLID_RED }] });
    clock.advance(400);
    let read = send(keyboard, { command: "readOverlay" });
    expect(read.payload).toMatchObject({
      cells: [
        { key: 1, remainingTtlMs: 100 },
        { key: 2, remainingTtlMs: 0 },
      ],
    });
    clock.advance(200);
    read = send(keyboard, { command: "readOverlay" });
    expect(read.payload).toMatchObject({ cells: [{ key: 2 }] });
  });

  it("REPLACE_OVERLAY atomically replaces; UNSET and CLEAR remove", () => {
    const keyboard = new MockKeyboard();
    send(keyboard, { command: "setCells", ttlMs: 0, cells: [{ key: 1, effect: SOLID_RED }, { key: 2, effect: SOLID_RED }] });
    send(keyboard, { command: "replaceOverlay", ttlMs: 0, cells: [{ key: 9, effect: BLINK_BLUE }] });
    expect(send(keyboard, { command: "readOverlay" }).payload).toMatchObject({ cells: [{ key: 9 }] });
    send(keyboard, { command: "unsetCells", keys: [9] });
    expect(keyboard.overlaySize()).toBe(0);
    send(keyboard, { command: "setCells", ttlMs: 0, cells: [{ key: 3, effect: SOLID_RED }] });
    send(keyboard, { command: "clearOverlay" });
    expect(keyboard.overlaySize()).toBe(0);
  });

  it("reports PARTIAL_APPLY for right-half writes while the peripheral is offline", () => {
    const keyboard = new MockKeyboard({ peripheralOffline: true });
    const response = send(keyboard, {
      command: "setCells",
      ttlMs: 0,
      cells: [{ key: 5, effect: SOLID_RED }, { key: 45, effect: SOLID_RED }, { key: 41, effect: SOLID_RED }],
    });
    expect(response.status).toBe("partialApply");
    expect(response.payload).toEqual({ type: "overlayAck", pendingKeys: [41, 45] });
    // A bare right-half clear: PARTIAL_APPLY with an empty pending list.
    const clear = send(keyboard, { command: "clearOverlay" });
    expect(clear.status).toBe("partialApply");
    expect(clear.payload).toEqual({ type: "overlayAck", pendingKeys: [] });
  });
});

describe("MockKeyboard brightness and toggles", () => {
  it("stores brightness and echoes the applied level", () => {
    const keyboard = new MockKeyboard();
    expect(send(keyboard, { command: "setBrightness", level: 90 }).payload).toEqual({ type: "brightness", level: 90 });
    expect(send(keyboard, { command: "getBrightness" }).payload).toEqual({ type: "brightness", level: 90 });
  });

  it("only knows toggles the active config defines", () => {
    const keyboard = new MockKeyboard({ initialConfig: SAMPLE_CONFIG });
    expect(send(keyboard, { command: "getToggle", id: 3 }).payload).toEqual({ type: "toggle", id: 3, state: true });
    expect(send(keyboard, { command: "setToggle", id: 3, state: false }).payload).toEqual({ type: "toggle", id: 3, state: false });
    expect(send(keyboard, { command: "getToggle", id: 9 }).status).toBe("unknownToggle");
  });
});

describe("MockKeyboard config session", () => {
  it("applies a valid blob transactionally and serves it back byte-stable", () => {
    const keyboard = new MockKeyboard();
    const blob = encodeLightingConfig(SAMPLE_CONFIG);
    expect(applyBlob(keyboard, blob).status).toBe("ok");
    expect(keyboard.activeConfigBlob()).toEqual(blob);
    // Chunked CONFIG_READ reproduces the committed bytes.
    const parts: Uint8Array[] = [];
    let offset = 0;
    for (;;) {
      const response = send(keyboard, { command: "configRead", offset, maxLen: 33 });
      expect(response.status).toBe("ok");
      const payload = response.payload as { type: "configData"; totalLen: number; data: Uint8Array };
      expect(payload.totalLen).toBe(blob.length);
      if (payload.data.length === 0) break;
      parts.push(payload.data);
      offset += payload.data.length;
      if (offset === blob.length) break;
    }
    expect(Uint8Array.from(parts.flatMap((part) => [...part]))).toEqual(blob);
  });

  it("reports an empty config as total_len 0", () => {
    const response = send(new MockKeyboard(), { command: "configRead", offset: 0, maxLen: 100 });
    expect(response.payload).toEqual({ type: "configData", totalLen: 0, data: new Uint8Array(0) });
  });

  it("answers NO_SESSION without a BEGIN", () => {
    const keyboard = new MockKeyboard();
    expect(send(keyboard, { command: "configData", offset: 0, data: new Uint8Array(4) }).status).toBe("noSession");
    expect(send(keyboard, { command: "configCommit" }).status).toBe("noSession");
    expect(send(keyboard, { command: "configAbort" }).status).toBe("ok"); // idempotent
  });

  it("aborts the session on a non-contiguous offset", () => {
    const keyboard = new MockKeyboard();
    const blob = encodeLightingConfig(SAMPLE_CONFIG);
    send(keyboard, { command: "configBegin", totalLen: blob.length, blobCrc32: crc32(blob) });
    expect(send(keyboard, { command: "configData", offset: 4, data: blob.subarray(4, 8) }).status).toBe("badOffset");
    expect(send(keyboard, { command: "configData", offset: 0, data: blob.subarray(0, 4) }).status).toBe("noSession");
  });

  it("rejects an early commit with CONFIG_INCOMPLETE", () => {
    const keyboard = new MockKeyboard();
    const blob = encodeLightingConfig(SAMPLE_CONFIG);
    send(keyboard, { command: "configBegin", totalLen: blob.length, blobCrc32: crc32(blob) });
    send(keyboard, { command: "configData", offset: 0, data: blob.subarray(0, 8) });
    expect(send(keyboard, { command: "configCommit" }).status).toBe("configIncomplete");
  });

  it("rejects a wrong announced CRC with CRC_MISMATCH and keeps the old config", () => {
    const keyboard = new MockKeyboard({ initialConfig: SAMPLE_CONFIG });
    const before = keyboard.activeConfigBlob();
    const blob = encodeLightingConfig({ togglePersistMask: 0, toggleInitialState: 0, records: [] });
    expect(applyBlob(keyboard, blob, crc32(blob) ^ 1).status).toBe("crcMismatch");
    expect(keyboard.activeConfigBlob()).toEqual(before);
  });

  it("rejects a structurally invalid blob with INVALID_CONFIG and keeps the old config", () => {
    const keyboard = new MockKeyboard({ initialConfig: SAMPLE_CONFIG });
    const before = keyboard.activeConfigBlob();
    const blob = encodeLightingConfig(SAMPLE_CONFIG);
    // Corrupt a record's activation kind, then re-seal the CRCs so only
    // structural validation can catch it.
    const bad = blob.slice();
    bad[28] = 9; // record 0 activation kind → unknown
    const view = new DataView(bad.buffer);
    view.setUint32(12, crc32(bad.subarray(16)), true);
    expect(applyBlob(keyboard, bad).status).toBe("invalidConfig");
    expect(keyboard.activeConfigBlob()).toEqual(before);
  });

  it("rejects a blob beyond max_config_blob_len at BEGIN", () => {
    const keyboard = new MockKeyboard({ capabilities: { maxConfigBlobLen: 64 } });
    expect(send(keyboard, { command: "configBegin", totalLen: 65, blobCrc32: 0 }).status).toBe("capacityExceeded");
  });
});
