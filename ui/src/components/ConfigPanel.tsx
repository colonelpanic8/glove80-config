// Persistent-config editor: the ordered lighting records the keyboard boots
// with (docs/lighting-design.md), edited offline and applied through the
// v1.1 transactional session (CONFIG_BEGIN → DATA… → COMMIT). The old config
// stays active unless a commit fully succeeds; CONFIG_READ loads the active
// blob back byte-stable.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { brushToEffect, type Brush } from "../lib/brush";
import {
  CONFIG_LAYER_COUNT,
  CONFIG_TOGGLE_COUNT,
  encodeLightingConfig,
  decodeLightingConfig,
  FEATURE_PERSISTENT_CONFIG,
  FEATURE_TOGGLES,
  MAX_CELLS_PER_RECORD,
  MAX_CONFIG_RECORDS,
  type Capabilities,
  type ConfigActivation,
  type ConfigRecord,
  type LightingConfig,
} from "../lib/host-protocol";
import type { ProtocolClient } from "../lib/protocol-client";
import { Board, type BoardCell } from "./Board";
import { BrushControls } from "./BrushControls";
import type { StatusUpdate } from "./OverlayPanel";

const DRAFT_STORAGE_KEY = "glove80-lightbench-config-draft-v1";

export const EMPTY_CONFIG: LightingConfig = {
  togglePersistMask: 0,
  toggleInitialState: 0,
  records: [],
};

function cloneConfig(config: LightingConfig): LightingConfig {
  return {
    togglePersistMask: config.togglePersistMask,
    toggleInitialState: config.toggleInitialState,
    records: config.records.map((record) => ({
      activation: { ...record.activation },
      cells: record.cells.map((cell) => ({ key: cell.key, effect: { ...cell.effect } })),
    })),
  };
}

/** Restore the saved draft; anything that does not survive a full encode
 * round-trip (the codec's own validation) is discarded. */
function loadDraft(): LightingConfig {
  try {
    const raw = localStorage.getItem(DRAFT_STORAGE_KEY);
    if (!raw) return cloneConfig(EMPTY_CONFIG);
    return decodeLightingConfig(encodeLightingConfig(JSON.parse(raw) as LightingConfig));
  } catch {
    return cloneConfig(EMPTY_CONFIG);
  }
}

function bytesEqual(a: Uint8Array | null, b: Uint8Array | null): boolean {
  if (a === null || b === null) return a === b;
  return a.length === b.length && a.every((byte, index) => byte === b[index]);
}

function activationLabel(activation: ConfigActivation): string {
  switch (activation.kind) {
    case "always":
      return "Always on";
    case "layerActive":
      return `Layer ${activation.layer}`;
    case "toggle":
      return `Toggle ${activation.id}`;
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

interface ConfigPanelProps {
  client: ProtocolClient | null;
  capabilities: Capabilities | null;
  brush: Brush;
  onBrushChange: (brush: Brush) => void;
  onStatus: (status: StatusUpdate) => void;
}

export function ConfigPanel({ client, capabilities, brush, onBrushChange, onStatus }: ConfigPanelProps) {
  const [config, setConfig] = useState(loadDraft);
  const [selected, setSelected] = useState(0);
  const [deviceBlob, setDeviceBlob] = useState<Uint8Array | null>(null);
  const [busy, setBusy] = useState(false);
  const importInput = useRef<HTMLInputElement | null>(null);

  const supportsConfig = !capabilities || (capabilities.featureBits & FEATURE_PERSISTENT_CONFIG) !== 0;
  const supportsToggles = !!capabilities && (capabilities.featureBits & FEATURE_TOGGLES) !== 0;
  const layerCount = Math.min(capabilities?.layerCapacity ?? CONFIG_LAYER_COUNT, CONFIG_LAYER_COUNT);

  useEffect(() => {
    try {
      localStorage.setItem(DRAFT_STORAGE_KEY, JSON.stringify(config));
    } catch {
      // Draft persistence is best-effort; never block editing on storage.
    }
  }, [config]);

  // The device's active blob is unknown again after a connection change.
  useEffect(() => {
    setDeviceBlob(null);
  }, [client]);

  const validation = useMemo(() => {
    try {
      return { blob: encodeLightingConfig(config), error: null };
    } catch (error) {
      return { blob: null, error: errorMessage(error) };
    }
  }, [config]);

  const dirtyLabel = !validation.blob
    ? { tone: "error" as const, text: `Invalid: ${validation.error}` }
    : deviceBlob === null
      ? { tone: "unknown" as const, text: "Not compared with a keyboard yet" }
      : bytesEqual(validation.blob, deviceBlob)
        ? { tone: "clean" as const, text: "In sync with the keyboard" }
        : { tone: "dirty" as const, text: "Differs from the keyboard — Apply to persist" };

  const record: ConfigRecord | undefined = config.records[selected];

  const updateRecord = useCallback((index: number, update: (record: ConfigRecord) => ConfigRecord) => {
    setConfig((current) => ({
      ...current,
      records: current.records.map((r, i) => (i === index ? update(r) : r)),
    }));
  }, []);

  const paintKey = useCallback(
    (key: number) => {
      if (!record) return;
      updateRecord(selected, (r) => {
        const cells = r.cells.filter((cell) => cell.key !== key);
        if (brush.mode === "erase") return { ...r, cells };
        if (cells.length >= MAX_CELLS_PER_RECORD) {
          onStatus({
            tone: "warn",
            message: `A record holds at most ${MAX_CELLS_PER_RECORD} cells — erase some or add another record`,
          });
          return r;
        }
        return { ...r, cells: [...cells, { key, effect: brushToEffect(brush) }] };
      });
    },
    [brush, onStatus, record, selected, updateRecord],
  );

  const addRecord = (activation: ConfigActivation) => {
    if (config.records.length >= MAX_CONFIG_RECORDS) {
      onStatus({ tone: "warn", message: `A config holds at most ${MAX_CONFIG_RECORDS} records` });
      return;
    }
    setConfig((current) => ({ ...current, records: [...current.records, { activation, cells: [] }] }));
    setSelected(config.records.length);
  };

  const removeRecord = (index: number) => {
    setConfig((current) => ({ ...current, records: current.records.filter((_, i) => i !== index) }));
    setSelected((current) => Math.max(0, current > index ? current - 1 : Math.min(current, config.records.length - 2)));
  };

  const moveRecord = (index: number, delta: -1 | 1) => {
    const target = index + delta;
    if (target < 0 || target >= config.records.length) return;
    setConfig((current) => {
      const records = [...current.records];
      [records[index], records[target]] = [records[target], records[index]];
      return { ...current, records };
    });
    setSelected((current) => (current === index ? target : current === target ? index : current));
  };

  const setActivation = (index: number, activation: ConfigActivation) => {
    updateRecord(index, (r) => ({ ...r, activation }));
  };

  const toggleIdsInUse = useMemo(() => {
    const ids = new Set<number>();
    for (const r of config.records) {
      if (r.activation.kind === "toggle") ids.add(r.activation.id);
    }
    return [...ids].sort((a, b) => a - b);
  }, [config.records]);

  const setToggleBit = (field: "togglePersistMask" | "toggleInitialState", id: number, on: boolean) => {
    setConfig((current) => ({
      ...current,
      [field]: on ? current[field] | (1 << id) : current[field] & ~(1 << id),
    }));
  };

  const flipRuntimeToggle = async (id: number, state: boolean) => {
    if (!client) return;
    try {
      const applied = await client.setToggle(id, state);
      onStatus({ tone: "ok", message: `Toggle ${id} is now ${applied ? "on" : "off"} on the keyboard` });
    } catch (error) {
      onStatus({ tone: "error", message: errorMessage(error) });
    }
  };

  const applyToKeyboard = async () => {
    if (!client || !validation.blob) return;
    setBusy(true);
    try {
      const blob = validation.blob;
      await client.applyConfigBlob(blob, (stage) => {
        switch (stage.stage) {
          case "begin":
            onStatus({ tone: "busy", message: `Opening config session · ${blob.length} bytes` });
            break;
          case "transfer":
            onStatus({ tone: "busy", message: `Transferring config · ${stage.sent}/${stage.total} bytes` });
            break;
          case "commit":
            onStatus({ tone: "busy", message: "Committing — the keyboard validates, activates and persists" });
            break;
          case "done":
            break;
        }
      });
      setDeviceBlob(blob);
      onStatus({ tone: "ok", message: "Config applied and persisted — it survives reboots" });
    } catch (error) {
      // StatusError already names CRC_MISMATCH / INVALID_CONFIG /
      // CONFIG_INCOMPLETE and says the old config is untouched.
      onStatus({ tone: "error", message: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  };

  const loadFromKeyboard = async () => {
    if (!client) return;
    setBusy(true);
    try {
      const blob = await client.readConfigBlob(({ read, total }) =>
        onStatus({ tone: "busy", message: `Reading config · ${read}/${total} bytes` }),
      );
      if (blob === null) {
        setDeviceBlob(null);
        onStatus({ tone: "warn", message: "The keyboard has no stored config" });
        return;
      }
      const loaded = decodeLightingConfig(blob);
      setConfig(loaded);
      setDeviceBlob(blob);
      setSelected(0);
      onStatus({ tone: "ok", message: `Loaded the active config · ${loaded.records.length} records` });
    } catch (error) {
      onStatus({ tone: "error", message: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  };

  const exportBlob = () => {
    if (!validation.blob) return;
    const blob = new Blob([validation.blob.slice().buffer as ArrayBuffer], {
      type: "application/octet-stream",
    });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = "glove80-lighting.bin";
    anchor.click();
    URL.revokeObjectURL(url);
    onStatus({ tone: "ok", message: `Exported glove80-lighting.bin · ${validation.blob.length} bytes` });
  };

  const importBlob = async (file: File) => {
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      const imported = decodeLightingConfig(bytes); // full validation
      setConfig(imported);
      setSelected(0);
      onStatus({ tone: "ok", message: `Imported ${file.name} · ${imported.records.length} records` });
    } catch (error) {
      onStatus({ tone: "error", message: `${file.name}: ${errorMessage(error)}` });
    }
  };

  const boardCells = new Map<number, BoardCell>(
    (record?.cells ?? []).map((cell) => [cell.key, { effect: cell.effect }]),
  );

  return (
    <section className="workspace">
      <aside className="tool-panel">
        <BrushControls brush={brush} onChange={onBrushChange} capabilities={capabilities} />

        <section>
          <div className="section-heading compact">
            <span className="step-number">03</span>
            <div>
              <h2>Records</h2>
              <p>Composited top to bottom in this order</p>
            </div>
          </div>
          <ul className="record-list">
            {config.records.map((r, index) => (
              <li key={index} className={index === selected ? "selected" : ""}>
                <button className="record-select" onClick={() => setSelected(index)}>
                  <strong>{activationLabel(r.activation)}</strong>
                  <small>
                    {r.cells.length}/{MAX_CELLS_PER_RECORD} cells
                  </small>
                </button>
                <span className="record-actions">
                  <button onClick={() => moveRecord(index, -1)} disabled={index === 0} aria-label="Move record up">↑</button>
                  <button
                    onClick={() => moveRecord(index, 1)}
                    disabled={index === config.records.length - 1}
                    aria-label="Move record down"
                  >
                    ↓
                  </button>
                  <button onClick={() => removeRecord(index)} aria-label="Delete record">✕</button>
                </span>
              </li>
            ))}
            {config.records.length === 0 && <li className="record-empty">No records — add one below</li>}
          </ul>
          <div className="tool-grid three">
            <button className="button tool" onClick={() => addRecord({ kind: "always" })}>+ Always</button>
            <button className="button tool" onClick={() => addRecord({ kind: "layerActive", layer: 1 })}>+ Layer</button>
            <button className="button tool" onClick={() => addRecord({ kind: "toggle", id: 0 })}>+ Toggle</button>
          </div>
          {record && record.activation.kind === "layerActive" && (
            <label className="inline-field">
              <span>Active on layer</span>
              <select
                value={record.activation.layer}
                onChange={(event) => setActivation(selected, { kind: "layerActive", layer: Number(event.target.value) })}
              >
                {Array.from({ length: layerCount }, (_, layer) => (
                  <option key={layer} value={layer}>Layer {layer}</option>
                ))}
              </select>
            </label>
          )}
          {record && record.activation.kind === "toggle" && (
            <label className="inline-field">
              <span>Toggle id</span>
              <select
                value={record.activation.id}
                onChange={(event) => setActivation(selected, { kind: "toggle", id: Number(event.target.value) })}
              >
                {Array.from({ length: CONFIG_TOGGLE_COUNT }, (_, id) => (
                  <option key={id} value={id}>Toggle {id}</option>
                ))}
              </select>
            </label>
          )}
        </section>

        {toggleIdsInUse.length > 0 && (
          <section>
            <div className="section-heading compact">
              <span className="step-number">04</span>
              <div>
                <h2>Toggles</h2>
                <p>Boot state, persistence, and live control</p>
              </div>
            </div>
            <ul className="toggle-list">
              {toggleIdsInUse.map((id) => (
                <li key={id}>
                  <strong>Toggle {id}</strong>
                  <label>
                    <input
                      type="checkbox"
                      checked={(config.toggleInitialState & (1 << id)) !== 0}
                      onChange={(event) => setToggleBit("toggleInitialState", id, event.target.checked)}
                    />
                    on at boot
                  </label>
                  <label>
                    <input
                      type="checkbox"
                      checked={(config.togglePersistMask & (1 << id)) !== 0}
                      onChange={(event) => setToggleBit("togglePersistMask", id, event.target.checked)}
                    />
                    persist state
                  </label>
                  {client && supportsToggles && (
                    <span className="toggle-live">
                      <button className="button tool" onClick={() => void flipRuntimeToggle(id, true)}>On</button>
                      <button className="button tool" onClick={() => void flipRuntimeToggle(id, false)}>Off</button>
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </section>
        )}

        <section className="scene-tools">
          <div className="section-heading compact">
            <span className="step-number">{toggleIdsInUse.length > 0 ? "05" : "04"}</span>
            <div>
              <h2>Keyboard & files</h2>
              <p>Transactional apply · byte-stable export</p>
            </div>
          </div>
          <div className={`dirty-indicator ${dirtyLabel.tone}`} role="status">
            {dirtyLabel.text}
          </div>
          <div className="tool-grid">
            <button
              className="button tool"
              disabled={!client || !supportsConfig || busy}
              onClick={() => void loadFromKeyboard()}
              title="CONFIG_READ: load the keyboard's active config into the editor"
            >
              Load from keyboard
            </button>
            <button
              className="button tool"
              disabled={!validation.blob}
              onClick={exportBlob}
              title="Save the encoded config blob as a .bin file"
            >
              Export .bin
            </button>
            <button className="button tool" onClick={() => importInput.current?.click()}>
              Import .bin
            </button>
            <button
              className="button tool"
              onClick={() => {
                setConfig(cloneConfig(EMPTY_CONFIG));
                setSelected(0);
              }}
            >
              New empty config
            </button>
          </div>
          <input
            ref={importInput}
            type="file"
            accept=".bin,application/octet-stream"
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0];
              event.target.value = "";
              if (file) void importBlob(file);
            }}
          />
          <button
            className="button apply"
            disabled={!client || !supportsConfig || !validation.blob || busy}
            onClick={() => void applyToKeyboard()}
            title="CONFIG_BEGIN → DATA → COMMIT: all-or-nothing; the old config stays if anything fails"
          >
            Apply to keyboard
          </button>
        </section>
      </aside>

      <section className="keyboard-stage" aria-label="Persistent config canvas">
        <Board
          cells={boardCells}
          onPaintKey={record ? paintKey : undefined}
          caption={
            record
              ? `Editing “${activationLabel(record.activation)}” — unpainted keys stay transparent and reveal the records below.`
              : "Add or select a record to paint its cells."
          }
        />
      </section>
    </section>
  );
}
