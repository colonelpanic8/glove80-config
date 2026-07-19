import { useState } from "react";

import { parseHex, rgbToHex, supportedEffectKinds, type Brush } from "../lib/brush";
import type { Capabilities, EffectKind } from "../lib/host-protocol";

const PALETTE = [
  0xf05d3e, 0xf5a524, 0xf4d35e, 0x6ecb63, 0x48cae4, 0x4d7cff, 0x9b6cff, 0xe56bdb,
  0xffffff, 0x5d6460, 0x151716, 0x000000,
];

function effectLabel(kind: EffectKind): string {
  return kind === "blink" ? "Blink" : kind === "breathe" ? "Breathe" : "Static";
}

interface BrushControlsProps {
  brush: Brush;
  onChange: (brush: Brush) => void;
  capabilities: Capabilities | null;
}

/** Color, motion and erase controls for the shared paint brush. */
export function BrushControls({ brush, onChange, capabilities }: BrushControlsProps) {
  const [hexDraft, setHexDraft] = useState(rgbToHex(brush.color));
  const kinds = supportedEffectKinds(capabilities);

  const setColor = (color: number) => {
    setHexDraft(rgbToHex(color));
    onChange({ ...brush, color, mode: "paint" });
  };

  return (
    <>
      <section>
        <div className="section-heading">
          <span className="step-number">01</span>
          <div>
            <h2>Brush</h2>
            <p>Click or drag across keys</p>
          </div>
        </div>
        <div className="mode-selector" role="group" aria-label="Brush mode">
          <button
            className={brush.mode === "paint" ? "selected" : ""}
            onClick={() => onChange({ ...brush, mode: "paint" })}
            aria-pressed={brush.mode === "paint"}
          >
            Paint
          </button>
          <button
            className={brush.mode === "erase" ? "selected" : ""}
            onClick={() => onChange({ ...brush, mode: "erase" })}
            aria-pressed={brush.mode === "erase"}
            title="Erased keys become transparent and reveal the layers below"
          >
            Erase
          </button>
        </div>
        <div className={`brush-paint-settings ${brush.mode === "erase" ? "disabled" : ""}`}>
          <div className="color-control">
            <label className="native-color" style={{ background: rgbToHex(brush.color) }}>
              <input
                type="color"
                value={rgbToHex(brush.color)}
                onChange={(event) => setColor(Number.parseInt(event.target.value.slice(1), 16))}
                aria-label="Choose paint color"
              />
            </label>
            <label className="hex-field">
              <span>HEX</span>
              <input
                value={hexDraft}
                spellCheck={false}
                onChange={(event) => {
                  setHexDraft(event.target.value);
                  const parsed = parseHex(event.target.value);
                  if (parsed !== undefined) onChange({ ...brush, color: parsed, mode: "paint" });
                }}
                onBlur={() => setHexDraft(rgbToHex(brush.color))}
                aria-label="Paint color hexadecimal value"
              />
            </label>
          </div>
          <div className="palette" aria-label="Color palette">
            {PALETTE.map((rgb) => (
              <button
                key={rgb}
                className={`swatch ${rgb === brush.color && brush.mode === "paint" ? "selected" : ""}`}
                style={{ background: rgbToHex(rgb) }}
                onClick={() => setColor(rgb)}
                aria-label={`Use color ${rgbToHex(rgb)}`}
                aria-pressed={rgb === brush.color && brush.mode === "paint"}
              />
            ))}
          </div>
        </div>
      </section>

      <section>
        <div className="section-heading compact">
          <span className="step-number">02</span>
          <div>
            <h2>Motion</h2>
            <p>
              {capabilities
                ? "Effects this keyboard supports"
                : "Solid, blink and breathe"}
            </p>
          </div>
        </div>
        <div className="mode-selector" role="group" aria-label="Paint effect">
          {kinds.map((kind) => (
            <button
              key={kind}
              className={brush.kind === kind ? "selected" : ""}
              onClick={() => onChange({ ...brush, kind, mode: "paint" })}
              aria-pressed={brush.kind === kind}
            >
              {effectLabel(kind)}
            </button>
          ))}
        </div>
        {brush.kind !== "solid" && (
          <div className="motion-ranges">
            <label className="range-control">
              <span>Period</span>
              <strong>{(brush.periodMs / 1000).toFixed(2)}s</strong>
              <input
                type="range"
                min="200"
                max="10000"
                step="50"
                value={brush.periodMs}
                onChange={(event) => onChange({ ...brush, periodMs: Number(event.target.value) })}
              />
            </label>
            {brush.kind === "blink" && (
              <label className="range-control">
                <span>On time</span>
                <strong>{brush.dutyPercent}%</strong>
                <input
                  type="range"
                  min="1"
                  max="99"
                  value={brush.dutyPercent}
                  onChange={(event) => onChange({ ...brush, dutyPercent: Number(event.target.value) })}
                />
              </label>
            )}
          </div>
        )}
      </section>
    </>
  );
}
