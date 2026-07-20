import { describe, expect, it } from "vitest";

import { fromViaKeycode, toViaKeycode } from "./rynk-keycode";

describe("Rynk/VIA compatibility conversion", () => {
  it("round-trips representative key actions", () => {
    const keycodes = [
      0x0000,
      0x0001,
      0x0004,
      0x0104,
      0x2104,
      0x4304,
      0x5223,
      0x5264,
      0x5283,
      0x52a2,
      0x52e3,
      0x5702,
      0x7704,
      0x7780,
      0x7784,
      0x7786,
      0x7c00,
      0x7c02,
      0x7c18,
      0x7c1e,
      0x7c77,
      0x7c79,
      0x7e10,
    ];

    for (const keycode of keycodes) {
      expect(toViaKeycode(fromViaKeycode(keycode)), `0x${keycode.toString(16)}`).toBe(keycode);
    }
  });

  it("maps unsupported values to KC_NO", () => {
    expect(fromViaKeycode(0xffff)).toBe("No");
    expect(toViaKeycode({ Single: { KeyboardControl: "ComboToggle" } })).toBe(0x7c52);
  });
});
