import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  decodeNamesSidecar,
  encodeNamesSidecar,
  loadToggleMeta,
  nextFreeToggleId,
  OFFLINE_IDENTITY,
  saveToggleMeta,
  SIDECAR_FORMAT,
} from "./toggle-meta";

// toggle-meta reads/writes window.localStorage; vitest runs in node here,
// so provide a minimal in-memory stand-in.
const store = new Map<string, string>();
beforeEach(() => {
  store.clear();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
  });
});

describe("toggle metadata storage", () => {
  it("round-trips names and added ids per identity", () => {
    saveToggleMeta("usb:Glove80", { names: { 2: "Gaming", 5: "Meeting" }, addedIds: [5, 2] });
    const meta = loadToggleMeta("usb:Glove80");
    expect(meta.names).toEqual({ 2: "Gaming", 5: "Meeting" });
    expect(meta.addedIds).toEqual([2, 5]); // sorted, deduped
  });

  it("uses the offline bucket as fallback base; the identity's own wins", () => {
    saveToggleMeta(OFFLINE_IDENTITY, { names: { 1: "Draft name", 2: "Offline two" }, addedIds: [1] });
    saveToggleMeta("ble:Glove80", { names: { 2: "Real two" }, addedIds: [3] });
    const meta = loadToggleMeta("ble:Glove80");
    expect(meta.names).toEqual({ 1: "Draft name", 2: "Real two" });
    expect(meta.addedIds).toEqual([1, 3]);
  });

  it("drops out-of-range ids and empty names, and survives corrupt storage", () => {
    saveToggleMeta("x", { names: { 40: "nope", 3: "  " } as never, addedIds: [-1, 32, 7] });
    expect(loadToggleMeta("x")).toEqual({ names: {}, addedIds: [7] });
    store.set("glove80-lightbench-toggle-meta-v1", "{not json");
    expect(loadToggleMeta("x")).toEqual({ names: {}, addedIds: [] });
  });
});

describe("nextFreeToggleId", () => {
  it("assigns the smallest unused id and null when all 32 are taken", () => {
    expect(nextFreeToggleId([])).toBe(0);
    expect(nextFreeToggleId([0, 1, 3])).toBe(2);
    expect(nextFreeToggleId(Array.from({ length: 32 }, (_, i) => i))).toBeNull();
  });
});

describe("names sidecar", () => {
  it("round-trips through encode/decode", () => {
    const text = encodeNamesSidecar({ 2: "Gaming", 17: "Do not disturb" });
    const parsed = JSON.parse(text) as { format: string; toggleNames: Record<string, string> };
    expect(parsed.format).toBe(SIDECAR_FORMAT);
    expect(decodeNamesSidecar(text)).toEqual({ 2: "Gaming", 17: "Do not disturb" });
  });

  it("rejects wrong format, bad ids and non-string names", () => {
    expect(() => decodeNamesSidecar("{oops")).toThrow(/JSON/);
    expect(() => decodeNamesSidecar(JSON.stringify({ format: "other", version: 1, toggleNames: {} }))).toThrow(
      /not a Lightbench/,
    );
    expect(() =>
      decodeNamesSidecar(JSON.stringify({ format: SIDECAR_FORMAT, version: 1, toggleNames: { 99: "x" } })),
    ).toThrow(/out of range/);
    expect(() =>
      decodeNamesSidecar(JSON.stringify({ format: SIDECAR_FORMAT, version: 1, toggleNames: { 3: 7 } })),
    ).toThrow(/not a string/);
    expect(() =>
      decodeNamesSidecar(JSON.stringify({ format: SIDECAR_FORMAT, version: 2, toggleNames: {} })),
    ).toThrow(/version/);
  });
});
