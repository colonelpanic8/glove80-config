// The paint brush shared by the overlay and config panels.

import type { Capabilities, Effect, EffectKind } from "./host-protocol";

export interface Brush {
  mode: "paint" | "erase";
  /** 0xRRGGBB */
  color: number;
  kind: EffectKind;
  periodMs: number;
  dutyPercent: number;
}

export const DEFAULT_BRUSH: Brush = {
  mode: "paint",
  color: 0x48cae4,
  kind: "solid",
  periodMs: 1500,
  dutyPercent: 50,
};

const EFFECT_BITS: Record<EffectKind, number> = { solid: 0, blink: 1, breathe: 2 };

/** Effect kinds the connected keyboard advertises (capability-driven; never
 * assume). With no capabilities yet (offline editing), offer all of them. */
export function supportedEffectKinds(capabilities: Capabilities | null): EffectKind[] {
  const all: EffectKind[] = ["solid", "blink", "breathe"];
  if (!capabilities) return all;
  return all.filter((kind) => (capabilities.effectMask & (1 << EFFECT_BITS[kind])) !== 0);
}

export function brushToEffect(brush: Brush): Effect {
  return {
    kind: brush.kind,
    r: (brush.color >>> 16) & 0xff,
    g: (brush.color >>> 8) & 0xff,
    b: brush.color & 0xff,
    periodMs: brush.kind === "solid" ? 0 : brush.periodMs,
    phaseMs: 0,
    dutyPercent: brush.kind === "blink" ? brush.dutyPercent : 0,
  };
}

export function rgbToHex(rgb: number): string {
  return `#${rgb.toString(16).padStart(6, "0")}`;
}

export function parseHex(value: string): number | undefined {
  const match = /^#?([0-9a-f]{6})$/i.exec(value.trim());
  return match ? Number.parseInt(match[1], 16) : undefined;
}
