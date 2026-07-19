// Per-cell effect parameter editor for the selected config record: pick a
// painted cell, then fine-tune its color and blink/breathe parameters with
// live validation against the codec's wire ranges (period/phase u16,
// duty 0–100).

import { useEffect, useState } from "react";

import { parseHex, rgbToHex, supportedEffectKinds } from "../lib/brush";
import type { Capabilities, CellWrite, ConfigRecord, Effect, EffectKind } from "../lib/host-protocol";
import { effectColor } from "./Board";

/** Wire ranges of the 10-byte effect record (PROTOCOL.md). */
const PARAM_RANGES = {
  periodMs: { min: 0, max: 65535, unit: "ms" }, // u16
  phaseMs: { min: 0, max: 65535, unit: "ms" }, // u16
  dutyPercent: { min: 0, max: 100, unit: "%" }, // u8, 0–100
} as const;

type ParamName = keyof typeof PARAM_RANGES;

function describeEffect(effect: Effect): string {
  switch (effect.kind) {
    case "solid":
      return "solid";
    case "blink":
      return `blink ${effect.periodMs}ms · ${effect.dutyPercent}%`;
    case "breathe":
      return `breathe ${effect.periodMs}ms`;
  }
}

interface ParamFieldProps {
  label: string;
  param: ParamName;
  value: number;
  /** Reset the field's draft when this changes (cell switch). */
  resetKey: string;
  onCommit: (value: number) => void;
  hint?: string;
}

/** A numeric field that commits every valid keystroke and flags out-of-range
 * input inline instead of silently clamping. */
function ParamField({ label, param, value, resetKey, onCommit, hint }: ParamFieldProps) {
  const range = PARAM_RANGES[param];
  const [draft, setDraft] = useState(String(value));
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(String(value));
    setError(null);
    // Reset when the edited cell changes OR the value changes underneath us
    // (e.g. the same key is repainted with the brush).
  }, [resetKey, value]);

  const handle = (text: string) => {
    setDraft(text);
    const parsed = Number(text);
    if (text.trim() === "" || !Number.isInteger(parsed)) {
      setError("whole number required");
      return;
    }
    if (parsed < range.min || parsed > range.max) {
      setError(`${range.min}–${range.max} ${range.unit}`);
      return;
    }
    setError(null);
    onCommit(parsed);
  };

  return (
    <label className={`param-field ${error ? "invalid" : ""}`}>
      <span>
        {label}
        <em>
          {range.min}–{range.max} {range.unit}
        </em>
      </span>
      <input
        type="number"
        inputMode="numeric"
        min={range.min}
        max={range.max}
        value={draft}
        onChange={(event) => handle(event.target.value)}
        onBlur={() => {
          setDraft(String(value));
          setError(null);
        }}
        aria-invalid={error !== null}
        aria-label={`${label} (${range.min}–${range.max} ${range.unit})`}
      />
      {error ? <small className="param-error">{error}</small> : hint ? <small>{hint}</small> : null}
    </label>
  );
}

interface CellEditorProps {
  record: ConfigRecord;
  selectedCellKey: number | null;
  onSelectCell: (key: number | null) => void;
  onUpdateCell: (key: number, effect: Effect) => void;
  onRemoveCell: (key: number) => void;
  capabilities: Capabilities | null;
}

export function CellEditor({
  record,
  selectedCellKey,
  onSelectCell,
  onUpdateCell,
  onRemoveCell,
  capabilities,
}: CellEditorProps) {
  const cells = [...record.cells].sort((a, b) => a.key - b.key);
  const selected: CellWrite | undefined = cells.find((cell) => cell.key === selectedCellKey);
  const kinds = supportedEffectKinds(capabilities);
  const [hexDraft, setHexDraft] = useState(selected ? effectColor(selected.effect) : "#000000");

  useEffect(() => {
    if (selected) setHexDraft(effectColor(selected.effect));
  }, [selectedCellKey, selected]);

  if (cells.length === 0) {
    return <p className="cell-editor-empty">Paint some keys first — each painted key becomes a cell you can fine-tune here.</p>;
  }

  const update = (patch: Partial<Effect>) => {
    if (!selected) return;
    onUpdateCell(selected.key, { ...selected.effect, ...patch });
  };

  const setKind = (kind: EffectKind) => {
    if (!selected || kind === selected.effect.kind) return;
    const e = selected.effect;
    // Encoders SHOULD zero ignored fields (PROTOCOL.md), and switching to an
    // animated kind needs sane parameters instead of zeros.
    if (kind === "solid") {
      update({ kind, periodMs: 0, phaseMs: 0, dutyPercent: 0 });
    } else if (kind === "blink") {
      update({ kind, periodMs: e.periodMs >= 2 ? e.periodMs : 1500, dutyPercent: e.dutyPercent > 0 && e.dutyPercent < 100 ? e.dutyPercent : 50 });
    } else {
      update({ kind, periodMs: e.periodMs >= 2 ? e.periodMs : 1500, dutyPercent: 0 });
    }
  };

  const setColorHex = (text: string) => {
    setHexDraft(text);
    const parsed = parseHex(text);
    if (parsed !== undefined) {
      update({ r: (parsed >>> 16) & 0xff, g: (parsed >>> 8) & 0xff, b: parsed & 0xff });
    }
  };

  const resetKey = `${selectedCellKey ?? "none"}`;

  return (
    <div className="cell-editor">
      <div className="cell-chip-list" role="listbox" aria-label="Cells in this record">
        {cells.map((cell) => (
          <button
            key={cell.key}
            role="option"
            aria-selected={cell.key === selectedCellKey}
            className={`cell-chip ${cell.key === selectedCellKey ? "selected" : ""}`}
            onClick={() => onSelectCell(cell.key === selectedCellKey ? null : cell.key)}
            title={`Key ${cell.key} · ${describeEffect(cell.effect)}`}
          >
            <span className="cell-dot" style={{ background: effectColor(cell.effect) }} aria-hidden="true" />
            {cell.key}
          </button>
        ))}
      </div>

      {selected ? (
        <div className="cell-params">
          <div className="cell-params-head">
            <strong>Key {selected.key}</strong>
            <button
              className="button tool"
              onClick={() => {
                onRemoveCell(selected.key);
                onSelectCell(null);
              }}
              title="Remove this cell — the key becomes transparent and reveals what is below"
            >
              Remove cell
            </button>
          </div>
          <div className="mode-selector" role="group" aria-label="Cell effect kind">
            {kinds.map((kind) => (
              <button
                key={kind}
                className={selected.effect.kind === kind ? "selected" : ""}
                onClick={() => setKind(kind)}
                aria-pressed={selected.effect.kind === kind}
              >
                {kind === "blink" ? "Blink" : kind === "breathe" ? "Breathe" : "Static"}
              </button>
            ))}
          </div>
          <div className="color-control">
            <label className="native-color" style={{ background: effectColor(selected.effect) }}>
              <input
                type="color"
                value={effectColor(selected.effect)}
                onChange={(event) => setColorHex(event.target.value)}
                aria-label="Cell color"
              />
            </label>
            <label className="hex-field">
              <span>HEX</span>
              <input
                value={hexDraft}
                spellCheck={false}
                onChange={(event) => setColorHex(event.target.value)}
                onBlur={() => setHexDraft(effectColor(selected.effect))}
                aria-label="Cell color hexadecimal value"
              />
            </label>
          </div>
          {selected.effect.kind !== "solid" && (
            <div className="param-grid">
              <ParamField
                label="Period"
                param="periodMs"
                value={selected.effect.periodMs}
                resetKey={resetKey}
                onCommit={(periodMs) => update({ periodMs })}
                hint={selected.effect.kind === "breathe" && selected.effect.periodMs < 2 ? "< 2ms renders static" : undefined}
              />
              <ParamField
                label="Phase"
                param="phaseMs"
                value={selected.effect.phaseMs}
                resetKey={resetKey}
                onCommit={(phaseMs) => update({ phaseMs })}
                hint="offsets the waveform"
              />
              {selected.effect.kind === "blink" && (
                <ParamField
                  label="Duty"
                  param="dutyPercent"
                  value={selected.effect.dutyPercent}
                  resetKey={resetKey}
                  onCommit={(dutyPercent) => update({ dutyPercent })}
                  hint={
                    selected.effect.dutyPercent === 0
                      ? "0% = always dark"
                      : selected.effect.dutyPercent >= 100
                        ? "100% = always on"
                        : undefined
                  }
                />
              )}
            </div>
          )}
        </div>
      ) : (
        <p className="cell-editor-hint">Select a cell above to edit its color and timing.</p>
      )}
    </div>
  );
}
