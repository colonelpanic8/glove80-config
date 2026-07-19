// Persistent-config editor: the ordered lighting records the keyboard boots
// with (docs/lighting-design.md), edited offline and applied through the
// v1.1 transactional session (CONFIG_BEGIN → DATA… → COMMIT). The old config
// stays active unless a commit fully succeeds; CONFIG_READ loads the active
// blob back byte-stable.
//
// The board has two modes: EDIT paints the selected record's cells (and only
// them — records are sparse layers), PREVIEW runs the client-side
// mini-compositor (lib/compositor-preview.ts) to show the composed result
// for a chosen layer/toggle state, with blink/breathe animated by the same
// phase math the firmware uses.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { brushToEffect, type Brush } from "../lib/brush";
import { cloneRecord, referencedToggleIds, type ConfigDraft } from "../lib/config-draft";
import {
  CHANNEL_CEILING,
  composePreview,
  previewAnimated,
  type PreviewState,
} from "../lib/compositor-preview";
import {
  CONFIG_LAYER_COUNT,
  CONFIG_TOGGLE_COUNT,
  encodeLightingConfig,
  decodeLightingConfig,
  FEATURE_PERSISTENT_CONFIG,
  MAX_CELLS_PER_RECORD,
  MAX_CONFIG_RECORDS,
  type Capabilities,
  type CellWrite,
  type ConfigActivation,
  type ConfigRecord,
  type Effect,
} from "../lib/host-protocol";
import type { ProtocolClient } from "../lib/protocol-client";
import { decodeNamesSidecar, encodeNamesSidecar } from "../lib/toggle-meta";
import { Board, type BoardCell } from "./Board";
import { BrushControls } from "./BrushControls";
import { CellEditor } from "./CellEditor";
import type { StatusUpdate } from "./OverlayPanel";

function bytesEqual(a: Uint8Array | null, b: Uint8Array | null): boolean {
  if (a === null || b === null) return a === b;
  return a.length === b.length && a.every((byte, index) => byte === b[index]);
}

function activationLabel(activation: ConfigActivation, toggleNames: Record<number, string>): string {
  switch (activation.kind) {
    case "always":
      return "Always on";
    case "layerActive":
      return `Layer ${activation.layer}`;
    case "toggle":
      return toggleNames[activation.id] ?? `Toggle ${activation.id}`;
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function downloadFile(name: string, bytes: Uint8Array | string, type: string): void {
  const blob =
    typeof bytes === "string"
      ? new Blob([bytes], { type })
      : new Blob([bytes.slice().buffer as ArrayBuffer], { type });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  URL.revokeObjectURL(url);
}

const solidEffect = (r: number, g: number, b: number): Effect => ({
  kind: "solid", r, g, b, periodMs: 0, phaseMs: 0, dutyPercent: 0,
});

/** Sample host-overlay cells for the preview: what a daemon/CLI might paint
 * live on top of everything (mirrored thumb keys solid, two blinking). */
const SAMPLE_HOST_CELLS: readonly CellWrite[] = [
  { key: 0, effect: solidEffect(0xe9, 0xed, 0xe8) },
  { key: 40, effect: solidEffect(0xe9, 0xed, 0xe8) },
  { key: 21, effect: { kind: "blink", r: 0xf5, g: 0xa5, b: 0x24, periodMs: 800, phaseMs: 0, dutyPercent: 50 } },
  { key: 61, effect: { kind: "blink", r: 0xf5, g: 0xa5, b: 0x24, periodMs: 800, phaseMs: 400, dutyPercent: 50 } },
];

type BoardMode = "edit" | "preview";

interface ConfigPanelProps {
  client: ProtocolClient | null;
  capabilities: Capabilities | null;
  brush: Brush;
  onBrushChange: (brush: Brush) => void;
  onStatus: (status: StatusUpdate) => void;
  draft: ConfigDraft;
  /** Host-side toggle names (browser storage + export sidecar). */
  toggleNames: Record<number, string>;
  onImportNames: (names: Record<number, string>) => void;
}

export function ConfigPanel({
  client,
  capabilities,
  brush,
  onBrushChange,
  onStatus,
  draft,
  toggleNames,
  onImportNames,
}: ConfigPanelProps) {
  const { config, setConfig, selected, setSelected } = draft;
  const [deviceBlob, setDeviceBlob] = useState<Uint8Array | null>(null);
  const [busy, setBusy] = useState(false);
  const [boardMode, setBoardMode] = useState<BoardMode>("edit");
  const [soloRecord, setSoloRecord] = useState<number | null>(null);
  const [selectedCellKey, setSelectedCellKey] = useState<number | null>(null);
  const [previewLayer, setPreviewLayer] = useState(0);
  const [previewToggleMask, setPreviewToggleMask] = useState(() => config.toggleInitialState);
  const [includeHostSample, setIncludeHostSample] = useState(false);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const importInput = useRef<HTMLInputElement | null>(null);

  const supportsConfig = !capabilities || (capabilities.featureBits & FEATURE_PERSISTENT_CONFIG) !== 0;
  const layerCount = Math.min(capabilities?.layerCapacity ?? CONFIG_LAYER_COUNT, CONFIG_LAYER_COUNT);

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

  const updateRecord = useCallback(
    (index: number, update: (record: ConfigRecord) => ConfigRecord) => {
      setConfig((current) => ({
        ...current,
        records: current.records.map((r, i) => (i === index ? update(r) : r)),
      }));
    },
    [setConfig],
  );

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
      if (brush.mode === "erase") {
        setSelectedCellKey((current) => (current === key ? null : current));
      } else {
        setSelectedCellKey(key); // painting selects, so tweaking is one click away
      }
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
    setSelectedCellKey(null);
  };

  const removeRecord = (index: number) => {
    setConfig((current) => ({ ...current, records: current.records.filter((_, i) => i !== index) }));
    setSelected((current) => Math.max(0, current > index ? current - 1 : Math.min(current, config.records.length - 2)));
    setSoloRecord((current) => (current === null ? null : current === index ? null : current > index ? current - 1 : current));
    setSelectedCellKey(null);
  };

  const duplicateRecord = (index: number) => {
    if (config.records.length >= MAX_CONFIG_RECORDS) {
      onStatus({ tone: "warn", message: `A config holds at most ${MAX_CONFIG_RECORDS} records` });
      return;
    }
    setConfig((current) => {
      const records = [...current.records];
      records.splice(index + 1, 0, cloneRecord(records[index]));
      return { ...current, records };
    });
    setSelected(index + 1);
  };

  /** Move a record from one list position to another (drag or arrows),
   * keeping selection and solo pinned to the records they pointed at. */
  const moveRecordTo = (from: number, to: number) => {
    if (from === to || from < 0 || to < 0 || from >= config.records.length || to >= config.records.length) return;
    setConfig((current) => {
      const records = [...current.records];
      const [moved] = records.splice(from, 1);
      records.splice(to, 0, moved);
      return { ...current, records };
    });
    const remap = (index: number): number => {
      if (index === from) return to;
      if (from < to) return index > from && index <= to ? index - 1 : index;
      return index >= to && index < from ? index + 1 : index;
    };
    setSelected(remap);
    setSoloRecord((current) => (current === null ? null : remap(current)));
  };

  const setActivation = (index: number, activation: ConfigActivation) => {
    updateRecord(index, (r) => ({ ...r, activation }));
  };

  const toggleIdsInUse = useMemo(() => referencedToggleIds(config), [config]);

  // --- composed preview ---------------------------------------------------

  const previewState: PreviewState = useMemo(
    () => ({
      records: config.records,
      activeLayer: previewLayer,
      togglesMask: previewToggleMask,
      hostCells: includeHostSample ? SAMPLE_HOST_CELLS : undefined,
      soloRecord,
    }),
    [config.records, previewLayer, previewToggleMask, includeHostSample, soloRecord],
  );

  const [previewNow, setPreviewNow] = useState(0);
  useEffect(() => {
    if (boardMode !== "preview" || !previewAnimated(previewState)) return;
    let raf = 0;
    let last = 0;
    const tick = (t: number) => {
      if (t - last >= 33) {
        last = t;
        setPreviewNow(t); // ~30fps is plenty for blink edges and breathe
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [boardMode, previewState]);

  const enterSolo = (index: number) => {
    if (soloRecord === index && boardMode === "preview") {
      setSoloRecord(null);
      setBoardMode("edit");
    } else {
      setSoloRecord(index);
      setBoardMode("preview");
    }
  };

  const setMode = (mode: BoardMode) => {
    setBoardMode(mode);
    if (mode === "edit") setSoloRecord(null);
  };

  // --- device & files -----------------------------------------------------

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
      setSelectedCellKey(null);
      onStatus({ tone: "ok", message: `Loaded the active config · ${loaded.records.length} records` });
    } catch (error) {
      onStatus({ tone: "error", message: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  };

  const exportBlob = () => {
    if (!validation.blob) return;
    downloadFile("glove80-lighting.bin", validation.blob, "application/octet-stream");
    const namesInUse = Object.fromEntries(
      Object.entries(toggleNames).filter(([id]) => toggleIdsInUse.includes(Number(id))),
    ) as Record<number, string>;
    if (Object.keys(namesInUse).length > 0) {
      // Names cannot live in the blob (no field for them) — write the
      // sidecar next to it so they travel with the export.
      downloadFile("glove80-lighting.names.json", encodeNamesSidecar(namesInUse), "application/json");
      onStatus({
        tone: "ok",
        message: `Exported glove80-lighting.bin · ${validation.blob.length} bytes + names sidecar (names are host-side only)`,
      });
    } else {
      onStatus({ tone: "ok", message: `Exported glove80-lighting.bin · ${validation.blob.length} bytes` });
    }
  };

  const importFile = async (file: File) => {
    try {
      if (file.name.endsWith(".json")) {
        const names = decodeNamesSidecar(await file.text());
        onImportNames(names);
        onStatus({
          tone: "ok",
          message: `Imported ${Object.keys(names).length} toggle names from ${file.name} (stored in this browser)`,
        });
        return;
      }
      const bytes = new Uint8Array(await file.arrayBuffer());
      const imported = decodeLightingConfig(bytes); // full validation
      setConfig(imported);
      setSelected(0);
      setSelectedCellKey(null);
      onStatus({ tone: "ok", message: `Imported ${file.name} · ${imported.records.length} records` });
    } catch (error) {
      onStatus({ tone: "error", message: `${file.name}: ${errorMessage(error)}` });
    }
  };

  // --- board content ------------------------------------------------------

  const boardCells = useMemo(() => {
    if (boardMode === "preview") {
      const frame = composePreview(previewState, previewNow);
      const cells = new Map<number, BoardCell>();
      frame.forEach((rgb, key) => {
        if (rgb !== null) cells.set(key, { effect: solidEffect(rgb.r, rgb.g, rgb.b) });
      });
      return cells;
    }
    return new Map<number, BoardCell>((record?.cells ?? []).map((cell) => [cell.key, { effect: cell.effect }]));
  }, [boardMode, previewState, previewNow, record]);

  const previewIsAnimated = boardMode === "preview" && previewAnimated(previewState);
  const soloLabel = soloRecord !== null && config.records[soloRecord]
    ? activationLabel(config.records[soloRecord].activation, toggleNames)
    : null;

  return (
    <section className="workspace">
      <aside className="tool-panel">
        <BrushControls brush={brush} onChange={onBrushChange} capabilities={capabilities} />

        <section>
          <div className="section-heading compact">
            <span className="step-number">03</span>
            <div>
              <h2>Records</h2>
              <p>Later records win within a class</p>
            </div>
          </div>
          <ul className="record-list">
            {config.records.map((r, index) => (
              <li
                key={index}
                className={[
                  index === selected ? "selected" : "",
                  dragIndex === index ? "dragging" : "",
                  soloRecord === index ? "soloed" : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
                draggable
                onDragStart={(event) => {
                  setDragIndex(index);
                  event.dataTransfer.effectAllowed = "move";
                }}
                onDragEnd={() => setDragIndex(null)}
                onDragOver={(event) => {
                  event.preventDefault();
                  event.dataTransfer.dropEffect = "move";
                }}
                onDrop={(event) => {
                  event.preventDefault();
                  if (dragIndex !== null) moveRecordTo(dragIndex, index);
                  setDragIndex(null);
                }}
              >
                <span className="drag-handle" aria-hidden="true" title="Drag to reorder">
                  ⠿
                </span>
                <button
                  className="record-select"
                  onClick={() => {
                    setSelected(index);
                    setSelectedCellKey(null);
                  }}
                >
                  <strong>{activationLabel(r.activation, toggleNames)}</strong>
                  <small>
                    {r.activation.kind === "toggle" && toggleNames[r.activation.id] ? `toggle ${r.activation.id} · ` : ""}
                    {r.cells.length}/{MAX_CELLS_PER_RECORD} cells
                  </small>
                </button>
                <span className="record-actions">
                  <button onClick={() => moveRecordTo(index, index - 1)} disabled={index === 0} aria-label="Move record up" title="Move up (earlier = composed lower)">↑</button>
                  <button
                    onClick={() => moveRecordTo(index, index + 1)}
                    disabled={index === config.records.length - 1}
                    aria-label="Move record down"
                    title="Move down (later = composed on top within its class)"
                  >
                    ↓
                  </button>
                  <button onClick={() => duplicateRecord(index)} disabled={config.records.length >= MAX_CONFIG_RECORDS} aria-label="Duplicate record" title="Duplicate this record">⧉</button>
                  <button
                    className={soloRecord === index ? "active" : ""}
                    onClick={() => enterSolo(index)}
                    aria-label="Solo-preview record"
                    aria-pressed={soloRecord === index}
                    title="Preview only this record's cells on the board"
                  >
                    ◉
                  </button>
                  <button onClick={() => removeRecord(index)} aria-label="Delete record" title="Delete record">✕</button>
                </span>
              </li>
            ))}
            {config.records.length === 0 && <li className="record-empty">No records — add one below</li>}
          </ul>
          <p className="compose-note">
            Composed bottom → top by class: always &lt; layer &lt; toggle &lt; host &lt; status.
            Within a class, later records paint over earlier ones.
          </p>
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
                  <option key={id} value={id}>
                    {toggleNames[id] ? `${id} · ${toggleNames[id]}` : `Toggle ${id}`}
                  </option>
                ))}
              </select>
            </label>
          )}
        </section>

        {record && (
          <section>
            <div className="section-heading compact">
              <span className="step-number">04</span>
              <div>
                <h2>Cells</h2>
                <p>Fine-tune one key's effect parameters</p>
              </div>
            </div>
            <CellEditor
              record={record}
              selectedCellKey={selectedCellKey}
              onSelectCell={setSelectedCellKey}
              onUpdateCell={(key, effect) =>
                updateRecord(selected, (r) => ({
                  ...r,
                  cells: r.cells.map((cell) => (cell.key === key ? { key, effect } : cell)),
                }))
              }
              onRemoveCell={(key) =>
                updateRecord(selected, (r) => ({ ...r, cells: r.cells.filter((cell) => cell.key !== key) }))
              }
              capabilities={capabilities}
            />
          </section>
        )}

        <section className="scene-tools">
          <div className="section-heading compact">
            <span className="step-number">{record ? "05" : "04"}</span>
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
              title="Save the encoded config blob as a .bin file (+ a .names.json sidecar when toggles are named — names cannot live in the blob)"
            >
              Export .bin
            </button>
            <button
              className="button tool"
              onClick={() => importInput.current?.click()}
              title="Import a config .bin, or a .names.json sidecar to restore toggle names"
            >
              Import .bin / names
            </button>
            <button
              className="button tool"
              onClick={() => {
                setConfig({ togglePersistMask: 0, toggleInitialState: 0, records: [] });
                setSelected(0);
                setSelectedCellKey(null);
                setSoloRecord(null);
              }}
            >
              New empty config
            </button>
          </div>
          <input
            ref={importInput}
            type="file"
            accept=".bin,.json,application/octet-stream,application/json"
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0];
              event.target.value = "";
              if (file) void importFile(file);
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
        <div className="stage-mode-bar">
          <div className="mode-selector two stage-mode" role="group" aria-label="Board mode">
            <button className={boardMode === "edit" ? "selected" : ""} onClick={() => setMode("edit")} aria-pressed={boardMode === "edit"}>
              Edit record
            </button>
            <button className={boardMode === "preview" ? "selected" : ""} onClick={() => setMode("preview")} aria-pressed={boardMode === "preview"}>
              Composed preview
            </button>
          </div>

          {boardMode === "preview" && soloRecord !== null && (
            <div className="preview-controls solo-banner">
              <span>
                Solo · <strong>{soloLabel ?? `record ${soloRecord + 1}`}</strong> — only this record, forced active
              </span>
              <button className="button tool" onClick={() => setSoloRecord(null)}>Show full composite</button>
            </div>
          )}

          {boardMode === "preview" && soloRecord === null && (
            <div className="preview-controls">
              <label className="preview-field">
                <span>Layer</span>
                <select value={previewLayer} onChange={(event) => setPreviewLayer(Number(event.target.value))}>
                  {Array.from({ length: layerCount }, (_, layer) => (
                    <option key={layer} value={layer}>{layer}</option>
                  ))}
                </select>
              </label>
              {toggleIdsInUse.length > 0 && (
                <div className="preview-toggle-chips" role="group" aria-label="Simulated toggle states">
                  {toggleIdsInUse.map((id) => {
                    const on = (previewToggleMask & (1 << id)) !== 0;
                    return (
                      <button
                        key={id}
                        className={`toggle-chip ${on ? "on" : ""}`}
                        aria-pressed={on}
                        onClick={() => setPreviewToggleMask((mask) => (on ? mask & ~(1 << id) : mask | (1 << id)))}
                        title={`Simulate toggle ${id} ${on ? "off" : "on"} (does not touch the keyboard)`}
                      >
                        {toggleNames[id] ?? `Toggle ${id}`}
                      </button>
                    );
                  })}
                </div>
              )}
              <label className="preview-field checkbox">
                <input
                  type="checkbox"
                  checked={includeHostSample}
                  onChange={(event) => setIncludeHostSample(event.target.checked)}
                />
                <span>Sample host overlay</span>
              </label>
            </div>
          )}
        </div>

        <Board
          cells={boardCells}
          onPaintKey={boardMode === "edit" && record ? paintKey : undefined}
          selectedKey={boardMode === "edit" ? selectedCellKey : null}
          caption={
            boardMode === "preview"
              ? `Client-side simulation of the firmware compositor — the keyboard is not being driven. ` +
                `Ceiling ${CHANNEL_CEILING}/255 (80% safety cap) applied${previewIsAnimated ? "; blink/breathe animated with the firmware's phase math" : ""}.`
              : record
                ? `Editing “${activationLabel(record.activation, toggleNames)}” — unpainted keys stay transparent and reveal the records below.`
                : "Add or select a record to paint its cells."
          }
        />
      </section>
    </section>
  );
}
