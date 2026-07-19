// Host-side toggle metadata: names and explicitly-added toggle ids.
//
// The config blob has no room for names (PROTOCOL.md v1.1: records carry
// only activation + cells; toggles are bare ids 0–31), so names are a
// host-side convenience with two honest homes:
//
// - this browser's localStorage, keyed by keyboard identity (transport kind
//   + device label), with an "offline" bucket as the fallback base;
// - a small JSON sidecar written next to the .bin on export and read back on
//   import, so names travel with a shared blob.
//
// Neither ever reaches the keyboard.

import { CONFIG_TOGGLE_COUNT } from "./host-protocol";

const STORAGE_KEY = "glove80-lightbench-toggle-meta-v1";
export const OFFLINE_IDENTITY = "offline";

/** The sidecar's `format` field — refuse anything else on import. */
export const SIDECAR_FORMAT = "glove80-lightbench-names";
export const SIDECAR_VERSION = 1;

export interface ToggleMeta {
  /** Toggle id → user-given name. */
  names: Record<number, string>;
  /** Toggle ids shown in the switchboard even before any record uses them. */
  addedIds: number[];
}

export const EMPTY_META: ToggleMeta = { names: {}, addedIds: [] };

type StoredBuckets = Record<string, { names?: Record<string, string>; addedIds?: number[] }>;

function readBuckets(): StoredBuckets {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    return parsed !== null && typeof parsed === "object" ? (parsed as StoredBuckets) : {};
  } catch {
    return {};
  }
}

function sanitizeNames(names: Record<string, string> | undefined): Record<number, string> {
  const out: Record<number, string> = {};
  for (const [key, value] of Object.entries(names ?? {})) {
    const id = Number(key);
    if (Number.isInteger(id) && id >= 0 && id < CONFIG_TOGGLE_COUNT && typeof value === "string" && value.trim() !== "") {
      out[id] = value;
    }
  }
  return out;
}

function sanitizeIds(ids: number[] | undefined): number[] {
  return [...new Set((ids ?? []).filter((id) => Number.isInteger(id) && id >= 0 && id < CONFIG_TOGGLE_COUNT))].sort(
    (a, b) => a - b,
  );
}

/** Load the metadata for one keyboard identity. The offline bucket serves
 * as the base so names given while disconnected still show up connected;
 * the identity's own entries win. */
export function loadToggleMeta(identity: string): ToggleMeta {
  const buckets = readBuckets();
  const base = buckets[OFFLINE_IDENTITY];
  const own = buckets[identity];
  return {
    names: { ...sanitizeNames(base?.names), ...sanitizeNames(own?.names) },
    addedIds: sanitizeIds([...(base?.addedIds ?? []), ...(own?.addedIds ?? [])]),
  };
}

/** Persist metadata under one identity (best-effort; storage may be full). */
export function saveToggleMeta(identity: string, meta: ToggleMeta): void {
  try {
    const buckets = readBuckets();
    buckets[identity] = { names: sanitizeNames(meta.names), addedIds: sanitizeIds(meta.addedIds) };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(buckets));
  } catch {
    // Never block the UI on storage.
  }
}

/** The smallest free toggle id given everything already visible, or null
 * when all 32 are taken. */
export function nextFreeToggleId(usedIds: Iterable<number>): number | null {
  const used = new Set(usedIds);
  for (let id = 0; id < CONFIG_TOGGLE_COUNT; id++) {
    if (!used.has(id)) return id;
  }
  return null;
}

/** Serialize names as the export sidecar (pretty-printed for humans). */
export function encodeNamesSidecar(names: Record<number, string>): string {
  const entries = Object.entries(names)
    .filter(([, name]) => name.trim() !== "")
    .sort(([a], [b]) => Number(a) - Number(b));
  return JSON.stringify(
    {
      format: SIDECAR_FORMAT,
      version: SIDECAR_VERSION,
      toggleNames: Object.fromEntries(entries),
    },
    null,
    2,
  );
}

/** Parse an imported sidecar; throws with a human-readable message on
 * anything that is not a well-formed names file. */
export function decodeNamesSidecar(text: string): Record<number, string> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new Error("not valid JSON");
  }
  if (parsed === null || typeof parsed !== "object") throw new Error("not a names sidecar");
  const obj = parsed as { format?: unknown; version?: unknown; toggleNames?: unknown };
  if (obj.format !== SIDECAR_FORMAT) {
    throw new Error(`not a Lightbench names sidecar (format ${JSON.stringify(obj.format ?? null)})`);
  }
  if (obj.version !== SIDECAR_VERSION) {
    throw new Error(`unsupported sidecar version ${String(obj.version)}`);
  }
  if (obj.toggleNames === null || typeof obj.toggleNames !== "object") {
    throw new Error("sidecar has no toggleNames object");
  }
  const names: Record<number, string> = {};
  for (const [key, value] of Object.entries(obj.toggleNames as Record<string, unknown>)) {
    const id = Number(key);
    if (!Number.isInteger(id) || id < 0 || id >= CONFIG_TOGGLE_COUNT) {
      throw new Error(`toggle id ${key} out of range (0–${CONFIG_TOGGLE_COUNT - 1})`);
    }
    if (typeof value !== "string") throw new Error(`name for toggle ${key} is not a string`);
    if (value.trim() !== "") names[id] = value;
  }
  return names;
}
