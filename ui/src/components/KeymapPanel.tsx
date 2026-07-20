// Keymap editor backed by Rynk in production and the frozen product-protocol
// backend in demo/legacy mode. The editor keeps VIA u16 keycodes as a
// transitional UI format and converts to typed Rynk actions at the boundary.

import { useCallback, useEffect, useMemo, useState } from "react";

import {
  GLOVE80_KEYS,
  GRID_TO_LED,
  KEYMAP_HOLES,
  LED_TO_GRID,
} from "../lib/glove80-layout";
import type { KeymapEntry } from "../lib/host-protocol";
import { formatKeycode, KeycodeError, parseKeycode, searchKeycodes } from "../lib/keycodes";
import type { RynkBrowserTransport } from "../lib/rynk-web-client";
import { Board, type BoardCell } from "./Board";
import type { StatusUpdate } from "./OverlayPanel";

const NO_CELLS: ReadonlyMap<number, BoardCell> = new Map();
const MAX_SEARCH_RESULTS = 24;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function hex4(code: number): string {
  return `0x${code.toString(16).toUpperCase().padStart(4, "0")}`;
}

interface LossyWrite {
  requested: number;
  stored: number;
}

interface KeymapPanelProps {
  client: {
    readKeymapLayer(layer: number): Promise<number[]>;
    writeKeymap(entries: KeymapEntry[]): Promise<number[]>;
  } | null;
  rows: number;
  cols: number;
  layerCount: number;
  sourceLabel: string | null;
  rynkConnecting: RynkBrowserTransport | null;
  onConnectRynk: (transport: RynkBrowserTransport) => void;
  onDisconnectRynk: (() => void) | null;
  onStatus: (status: StatusUpdate) => void;
}

export function KeymapPanel({
  client,
  rows,
  cols,
  layerCount,
  sourceLabel,
  rynkConnecting,
  onConnectRynk,
  onDisconnectRynk,
  onStatus,
}: KeymapPanelProps) {
  const [layer, setLayer] = useState(0);
  /** layer → keycodes in flat grid order, as last read from the keyboard. */
  const [layers, setLayers] = useState<Map<number, number[]>>(new Map());
  /** Staged edits, keyed "layer:gridKey" → requested keycode. */
  const [pending, setPending] = useState<Map<string, number>>(new Map());
  /** Lossy write results, keyed "layer:gridKey". */
  const [lossy, setLossy] = useState<Map<string, LossyWrite>>(new Map());
  const [selectedGrid, setSelectedGrid] = useState<number | null>(null);
  const [entryText, setEntryText] = useState("");
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState(false);

  const supported = client !== null && rows > 0 && cols > 0 && layerCount > 0;
  const gridSize = rows * cols;

  // A new connection means a new keymap: drop everything cached or staged.
  useEffect(() => {
    setLayers(new Map());
    setPending(new Map());
    setLossy(new Map());
    setSelectedGrid(null);
    setLayer(0);
  }, [client]);

  const loadLayer = useCallback(
    async (target: number, announce: boolean) => {
      if (!client || !supported) return;
      setBusy(true);
      try {
        const keycodes = await client.readKeymapLayer(target);
        setLayers((current) => new Map(current).set(target, keycodes));
        setPending((current) => {
          const next = new Map(current);
          for (const key of next.keys()) {
            if (key.startsWith(`${target}:`)) next.delete(key);
          }
          return next;
        });
        setLossy((current) => {
          const next = new Map(current);
          for (const key of next.keys()) {
            if (key.startsWith(`${target}:`)) next.delete(key);
          }
          return next;
        });
        if (announce) {
          onStatus({ tone: "ok", message: `Layer ${target} reloaded from the keyboard` });
        }
      } catch (error) {
        onStatus({ tone: "error", message: errorMessage(error) });
      } finally {
        setBusy(false);
      }
    },
    [client, onStatus, supported],
  );

  // Lazily read each layer the first time it is shown.
  useEffect(() => {
    if (client && supported && !layers.has(layer)) void loadLayer(layer, false);
  }, [client, supported, layer, layers, loadLayer]);

  const stored = layers.get(layer);

  /** The keycode the board should show at a grid position: staged edit if
   * any, else the last value read from the keyboard. */
  const shownKeycode = useCallback(
    (grid: number): number | undefined => pending.get(`${layer}:${grid}`) ?? stored?.[grid],
    [layer, pending, stored],
  );

  const keyLabels = useMemo(() => {
    const labels = new Map<number, string>();
    for (const [grid, led] of GRID_TO_LED) {
      const code = shownKeycode(grid);
      labels.set(led, code === undefined ? "…" : formatKeycode(code));
    }
    return labels;
  }, [shownKeycode]);

  const pendingLeds = useMemo(() => {
    const leds = new Set<number>();
    for (const key of pending.keys()) {
      const [l, grid] = key.split(":").map(Number);
      if (l === layer) {
        const led = GRID_TO_LED.get(grid);
        if (led !== undefined) leds.add(led);
      }
    }
    return leds;
  }, [layer, pending]);

  const lossyLeds = useMemo(() => {
    const leds = new Set<number>();
    for (const key of lossy.keys()) {
      const [l, grid] = key.split(":").map(Number);
      if (l === layer) {
        const led = GRID_TO_LED.get(grid);
        if (led !== undefined) leds.add(led);
      }
    }
    return leds;
  }, [layer, lossy]);

  const selectKey = useCallback(
    (led: number) => {
      const grid = LED_TO_GRID.get(led);
      if (grid === undefined) return;
      setSelectedGrid(grid);
      const code = pending.get(`${layer}:${grid}`) ?? layers.get(layer)?.[grid];
      setEntryText(code === undefined ? "" : formatKeycode(code));
    },
    [layer, layers, pending],
  );

  const selectedLed = selectedGrid === null ? null : (GRID_TO_LED.get(selectedGrid) ?? null);
  const selectedSpec =
    selectedLed === null ? undefined : GLOVE80_KEYS.find((k) => k.ledIndex === selectedLed);
  const selectedStored = selectedGrid === null ? undefined : stored?.[selectedGrid];
  const selectedPending =
    selectedGrid === null ? undefined : pending.get(`${layer}:${selectedGrid}`);

  const parsedEntry = useMemo(() => {
    if (entryText.trim() === "") return null;
    try {
      return { code: parseKeycode(entryText), error: null };
    } catch (error) {
      return { code: null, error: error instanceof KeycodeError ? error.message : errorMessage(error) };
    }
  }, [entryText]);

  const searchResults = useMemo(
    () => (search.trim() === "" ? [] : searchKeycodes(search).slice(0, MAX_SEARCH_RESULTS)),
    [search],
  );

  const stageEntry = (code: number) => {
    if (selectedGrid === null) return;
    const key = `${layer}:${selectedGrid}`;
    setPending((current) => {
      const next = new Map(current);
      if (stored !== undefined && stored[selectedGrid] === code) {
        next.delete(key); // staging the current value = no edit
      } else {
        next.set(key, code);
      }
      return next;
    });
    setEntryText(formatKeycode(code));
  };

  const discardEdit = () => {
    if (selectedGrid === null) return;
    setPending((current) => {
      const next = new Map(current);
      next.delete(`${layer}:${selectedGrid}`);
      return next;
    });
    setEntryText(selectedStored === undefined ? "" : formatKeycode(selectedStored));
  };

  const writePending = async () => {
    if (!client || pending.size === 0) return;
    const entries: KeymapEntry[] = [...pending.entries()].map(([key, keycode]) => {
      const [entryLayer, entryKey] = key.split(":").map(Number);
      return { layer: entryLayer, key: entryKey, keycode };
    });
    setBusy(true);
    try {
      const readback = await client.writeKeymap(entries);
      const lossyWrites = new Map<string, LossyWrite>();
      setLayers((current) => {
        const next = new Map(current);
        entries.forEach((entry, index) => {
          const codes = next.get(entry.layer);
          if (codes) {
            const updated = [...codes];
            updated[entry.key] = readback[index];
            next.set(entry.layer, updated);
          }
        });
        return next;
      });
      entries.forEach((entry, index) => {
        if (readback[index] !== entry.keycode) {
          lossyWrites.set(`${entry.layer}:${entry.key}`, {
            requested: entry.keycode,
            stored: readback[index],
          });
        }
      });
      setPending(new Map());
      setLossy(lossyWrites);
      if (selectedGrid !== null) {
        const index = entries.findIndex((e) => e.layer === layer && e.key === selectedGrid);
        if (index >= 0) setEntryText(formatKeycode(readback[index]));
      }
      if (lossyWrites.size > 0) {
        onStatus({
          tone: "warn",
          message:
            `Wrote ${entries.length} key(s); ${lossyWrites.size} stored differently (LOSSY) — ` +
            "the firmware has no exact representation for those keycodes",
        });
      } else {
        onStatus({
          tone: "ok",
          message: `Wrote ${entries.length} key(s) — live on the keyboard now, and persisted`,
        });
      }
    } catch (error) {
      // KEYMAP_WRITE batches are all-or-nothing on the device; a failed
      // batch wrote nothing, so the staged edits stay staged.
      onStatus({ tone: "error", message: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  };

  if (!client || !supported) {
    return (
      <section className="workspace">
        <div className="keymap-gate">
          <strong>Keymaps are owned by Rynk.</strong>
          <span>
            USB and Bluetooth use Rynk&apos;s WebHID collection. Bluetooth must already be paired.
          </span>
          <div className="connect-actions">
            <button
              className="button primary"
              disabled={rynkConnecting !== null}
              onClick={() => onConnectRynk("usb")}
            >
              {rynkConnecting === "usb" ? "Connecting…" : "Connect Rynk USB"}
            </button>
            <button
              className="button subtle"
              disabled={rynkConnecting !== null}
              onClick={() => onConnectRynk("ble")}
            >
              {rynkConnecting === "ble" ? "Connecting…" : "Connect Rynk Bluetooth"}
            </button>
          </div>
          <small>Demo mode still exercises the frozen legacy protocol without touching hardware.</small>
        </div>
      </section>
    );
  }

  const lossyEntries = [...lossy.entries()].filter(([key]) => key.startsWith(`${layer}:`));

  return (
    <section className="workspace">
      <aside className="tool-panel">
        <section>
          <div className="section-heading compact">
            <span className="step-number">01</span>
            <div>
              <h2>Layer</h2>
              <p>
                {rows}×{cols} grid · {layerCount} layers · {sourceLabel ?? "Rynk"}
              </p>
            </div>
          </div>
          <div className="layer-selector" role="tablist" aria-label="Keymap layers">
            {Array.from({ length: layerCount }, (_, index) => (
              <button
                key={index}
                role="tab"
                aria-selected={layer === index}
                className={layer === index ? "selected" : ""}
                onClick={() => {
                  setLayer(index);
                  setSelectedGrid(null);
                  setEntryText("");
                }}
              >
                {index}
              </button>
            ))}
          </div>
          <button
            className="button tool wide"
            disabled={busy}
            onClick={() => void loadLayer(layer, true)}
            title="Re-read this layer through Rynk (discards staged edits on it)"
          >
            Reload from keyboard
          </button>
          {onDisconnectRynk && (
            <button className="button subtle wide" disabled={busy} onClick={onDisconnectRynk}>
              Disconnect Rynk
            </button>
          )}
        </section>

        <section>
          <div className="section-heading compact">
            <span className="step-number">02</span>
            <div>
              <h2>Binding</h2>
              <p>Click a key on the board to edit it</p>
            </div>
          </div>
          {selectedGrid === null || !selectedSpec ? (
            <p className="keymap-hint">No key selected.</p>
          ) : (
            <div className="binding-editor">
              <div className="binding-position">
                <strong>{selectedSpec.label}</strong>
                <small>
                  key {selectedGrid} · r{Math.floor(selectedGrid / cols)},c{selectedGrid % cols}
                  {KEYMAP_HOLES.includes(selectedGrid) ? " · hole" : ""}
                </small>
              </div>
              <div className="binding-current">
                <span>On keyboard</span>
                <strong>
                  {selectedStored === undefined
                    ? "…"
                    : `${formatKeycode(selectedStored)} (${hex4(selectedStored)})`}
                </strong>
              </div>
              {selectedPending !== undefined && (
                <div className="binding-current staged">
                  <span>Staged</span>
                  <strong>
                    {formatKeycode(selectedPending)} ({hex4(selectedPending)})
                  </strong>
                </div>
              )}
              <label className="binding-input">
                <span>Keycode — a name, MO(2), LT(1, KC_A), or hex like 0x0004</span>
                <input
                  value={entryText}
                  onChange={(event) => setEntryText(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && parsedEntry?.code !== null && parsedEntry) {
                      stageEntry(parsedEntry.code);
                    }
                  }}
                  placeholder="KC_A"
                  spellCheck={false}
                />
              </label>
              {parsedEntry && parsedEntry.error !== null && (
                <p className="binding-error">{parsedEntry.error}</p>
              )}
              <div className="tool-grid">
                <button
                  className="button tool"
                  disabled={!parsedEntry || parsedEntry.code === null}
                  onClick={() => parsedEntry?.code !== null && parsedEntry && stageEntry(parsedEntry.code)}
                >
                  Stage edit
                </button>
                <button
                  className="button tool"
                  disabled={selectedPending === undefined}
                  onClick={discardEdit}
                >
                  Discard
                </button>
              </div>
              <label className="binding-input">
                <span>Search keycodes</span>
                <input
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder="play, shift, boot…"
                  spellCheck={false}
                />
              </label>
              {searchResults.length > 0 && (
                <ul className="keycode-results">
                  {searchResults.map((result) => (
                    <li key={result.code}>
                      <button onClick={() => stageEntry(result.code)}>
                        <strong>{result.name}</strong>
                        <small>
                          {hex4(result.code)}
                          {result.aliases.length > 0 ? ` · ${result.aliases.join(", ")}` : ""}
                        </small>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </section>

        <section className="scene-tools">
          <div className="section-heading compact">
            <span className="step-number">03</span>
            <div>
              <h2>Write</h2>
              <p>Rynk writes with canonical read-back</p>
            </div>
          </div>
          {lossyEntries.length > 0 && (
            <ul className="lossy-list">
              {lossyEntries.map(([key, write]) => (
                <li key={key}>
                  LOSSY · key {key.split(":")[1]}: wrote {formatKeycode(write.requested)}, stored{" "}
                  {formatKeycode(write.stored)} ({hex4(write.stored)})
                </li>
              ))}
            </ul>
          )}
          <button
            className="button apply"
            disabled={pending.size === 0 || busy}
            onClick={() => void writePending()}
            title="All-or-nothing per batch; the firmware echoes what it actually stored"
          >
            {pending.size === 0 ? "No staged edits" : `Write ${pending.size} change${pending.size === 1 ? "" : "s"}`}
          </button>
          <p className="keymap-hint">
            Writes change the live keymap immediately, persist through RMK storage, and are
            read back through Rynk before Lightbench reports success.
          </p>
        </section>
      </aside>

      <section className="keyboard-stage" aria-label="Keymap editor">
        <Board
          cells={NO_CELLS}
          keyLabels={keyLabels}
          selectedKey={selectedLed}
          pendingKeys={pendingLeds}
          flaggedKeys={lossyLeds}
          onPaintKey={selectKey}
          caption={`Layer ${layer} — Rynk actions rendered through the transitional VIA-keycode editor. Dashed = staged, red = lossy conversion. Grid holes are not shown; they always read KC_NO.`}
        />
      </section>
    </section>
  );
}
