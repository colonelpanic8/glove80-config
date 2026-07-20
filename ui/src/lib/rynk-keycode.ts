// Transitional VIA u16 <-> Rynk KeyAction conversion for Lightbench.
//
// The canonical Glove80 config still uses VIA keycodes. Rynk uses typed
// actions, so this mirrors the native CLI's migration boundary. Unknown or
// unrepresentable actions deliberately become KC_NO and are surfaced by the
// editor's existing lossy-readback UI.

import type {
  Action,
  HidKeyCode,
  KeyAction,
  KeyboardAction,
  ModifierCombination,
} from "../vendor/rynk-wasm/rynk_wasm";

const HID_SPECIAL: Record<number, HidKeyCode> = {
  0x00: "No",
  0x01: "ErrorRollover",
  0x02: "PostFail",
  0x03: "ErrorUndefined",
  0x28: "Enter",
  0x29: "Escape",
  0x2a: "Backspace",
  0x2b: "Tab",
  0x2c: "Space",
  0x2d: "Minus",
  0x2e: "Equal",
  0x2f: "LeftBracket",
  0x30: "RightBracket",
  0x31: "Backslash",
  0x32: "NonusHash",
  0x33: "Semicolon",
  0x34: "Quote",
  0x35: "Grave",
  0x36: "Comma",
  0x37: "Dot",
  0x38: "Slash",
  0x39: "CapsLock",
  0x46: "PrintScreen",
  0x47: "ScrollLock",
  0x48: "Pause",
  0x49: "Insert",
  0x4a: "Home",
  0x4b: "PageUp",
  0x4c: "Delete",
  0x4d: "End",
  0x4e: "PageDown",
  0x4f: "Right",
  0x50: "Left",
  0x51: "Down",
  0x52: "Up",
  0x53: "NumLock",
  0x54: "KpSlash",
  0x55: "KpAsterisk",
  0x56: "KpMinus",
  0x57: "KpPlus",
  0x58: "KpEnter",
  0x63: "KpDot",
  0x64: "NonusBackslash",
  0x65: "Application",
  0x66: "KbPower",
  0x67: "KpEqual",
  0x74: "Execute",
  0x75: "Help",
  0x76: "Menu",
  0x77: "Select",
  0x78: "Stop",
  0x79: "Again",
  0x7a: "Undo",
  0x7b: "Cut",
  0x7c: "Copy",
  0x7d: "Paste",
  0x7e: "Find",
  0x7f: "KbMute",
  0x80: "KbVolumeUp",
  0x81: "KbVolumeDown",
  0xa5: "SystemPower",
  0xa6: "SystemSleep",
  0xa7: "SystemWake",
  0xa8: "AudioMute",
  0xa9: "AudioVolUp",
  0xaa: "AudioVolDown",
  0xab: "MediaNextTrack",
  0xac: "MediaPrevTrack",
  0xad: "MediaStop",
  0xae: "MediaPlayPause",
  0xaf: "MediaSelect",
  0xb0: "MediaEject",
  0xb1: "Mail",
  0xb2: "Calculator",
  0xb3: "MyComputer",
  0xb4: "WwwSearch",
  0xb5: "WwwHome",
  0xb6: "WwwBack",
  0xb7: "WwwForward",
  0xb8: "WwwStop",
  0xb9: "WwwRefresh",
  0xba: "WwwFavorites",
  0xbb: "MediaFastForward",
  0xbc: "MediaRewind",
  0xbd: "BrightnessUp",
  0xbe: "BrightnessDown",
  0xbf: "ControlPanel",
  0xc0: "Assistant",
  0xc1: "MissionControl",
  0xc2: "Launchpad",
  0xcd: "MouseUp",
  0xce: "MouseDown",
  0xcf: "MouseLeft",
  0xd0: "MouseRight",
  0xd1: "MouseBtn1",
  0xd2: "MouseBtn2",
  0xd3: "MouseBtn3",
  0xd4: "MouseBtn4",
  0xd5: "MouseBtn5",
  0xd6: "MouseBtn6",
  0xd7: "MouseBtn7",
  0xd8: "MouseBtn8",
  0xd9: "MouseWheelUp",
  0xda: "MouseWheelDown",
  0xdb: "MouseWheelLeft",
  0xdc: "MouseWheelRight",
  0xdd: "MouseAccel0",
  0xde: "MouseAccel1",
  0xdf: "MouseAccel2",
  0xe0: "LCtrl",
  0xe1: "LShift",
  0xe2: "LAlt",
  0xe3: "LGui",
  0xe4: "RCtrl",
  0xe5: "RShift",
  0xe6: "RAlt",
  0xe7: "RGui",
};

function hidFromCode(code: number): HidKeyCode | undefined {
  if (code >= 0x04 && code <= 0x1d) return String.fromCharCode(65 + code - 4) as HidKeyCode;
  if (code >= 0x1e && code <= 0x26) return `Kc${code - 0x1d}` as HidKeyCode;
  if (code === 0x27) return "Kc0";
  if (code >= 0x3a && code <= 0x45) return `F${code - 0x39}` as HidKeyCode;
  if (code >= 0x59 && code <= 0x61) return `Kp${code - 0x58}` as HidKeyCode;
  if (code === 0x62) return "Kp0";
  if (code >= 0x68 && code <= 0x73) return `F${code - 0x5b}` as HidKeyCode;
  return HID_SPECIAL[code];
}

const CODE_BY_HID = new Map<HidKeyCode, number>();
for (let code = 0; code <= 0xff; code++) {
  const key = hidFromCode(code);
  if (key !== undefined) CODE_BY_HID.set(key, code);
}

function modifiers(bits: number): ModifierCombination {
  const right = (bits & 0x10) !== 0;
  return {
    left_ctrl: !right && (bits & 0x01) !== 0,
    left_shift: !right && (bits & 0x02) !== 0,
    left_alt: !right && (bits & 0x04) !== 0,
    left_gui: !right && (bits & 0x08) !== 0,
    right_ctrl: right && (bits & 0x01) !== 0,
    right_shift: right && (bits & 0x02) !== 0,
    right_alt: right && (bits & 0x04) !== 0,
    right_gui: right && (bits & 0x08) !== 0,
  };
}

function modifierBits(mods: ModifierCombination): number {
  const right = mods.right_ctrl || mods.right_shift || mods.right_alt || mods.right_gui;
  return (
    (right ? 0x10 : 0) |
    (mods.left_ctrl || mods.right_ctrl ? 0x01 : 0) |
    (mods.left_shift || mods.right_shift ? 0x02 : 0) |
    (mods.left_alt || mods.right_alt ? 0x04 : 0) |
    (mods.left_gui || mods.right_gui ? 0x08 : 0)
  );
}

function keyAction(action: Action): KeyAction {
  return { Single: action };
}

function spaceCadet(key: HidKeyCode, hold: ModifierCombination): KeyAction {
  return {
    TapHold: [
      { KeyWithModifier: [key, modifiers(0x02)] },
      { Modifier: hold },
      0xff,
    ],
  };
}

function hidAction(code: number): Action | undefined {
  const key = hidFromCode(code);
  return key === undefined ? undefined : { Key: { Hid: key } };
}

export function fromViaKeycode(code: number): KeyAction {
  const basic = code & 0xff;
  if (code === 0x0000) return "No";
  if (code === 0x0001) return "Transparent";
  if (code >= 0x0002 && code <= 0x00ff) return keyAction(hidAction(code) ?? "No");
  if (code >= 0x0100 && code <= 0x1fff) {
    const key = hidFromCode(basic);
    return key ? keyAction({ KeyWithModifier: [key, modifiers(code >> 8)] }) : "No";
  }
  if (code >= 0x2000 && code <= 0x3fff) {
    return {
      TapHold: [hidAction(basic) ?? "No", { Modifier: modifiers((code >> 8) & 0x1f) }, 0xff],
    };
  }
  if (code >= 0x4000 && code <= 0x4fff) {
    return { TapHold: [hidAction(basic) ?? "No", { LayerOn: (code >> 8) & 0x0f }, 0xff] };
  }
  if (code >= 0x5000 && code <= 0x51ff) {
    return keyAction({ LayerOnWithModifier: [(code >> 5) & 0x0f, modifiers(code & 0x1f)] });
  }
  if (code >= 0x5200 && code <= 0x521f) return keyAction({ LayerToggleOnly: code & 0x0f });
  if (code >= 0x5220 && code <= 0x523f) return keyAction({ LayerOn: code & 0x0f });
  if (code >= 0x5240 && code <= 0x525f) return keyAction({ DefaultLayer: code & 0x0f });
  if (code >= 0x5260 && code <= 0x527f) return keyAction({ LayerToggle: code & 0x0f });
  if (code >= 0x5280 && code <= 0x529f) return keyAction({ OneShotLayer: code & 0x0f });
  if (code >= 0x52a0 && code <= 0x52bf) return keyAction({ OneShotModifier: modifiers(code & 0x1f) });
  if (code >= 0x52e0 && code <= 0x52ff) return keyAction({ PersistentDefaultLayer: code & 0x0f });
  if (code >= 0x5700 && code <= 0x57ff) return { Morse: code & 0xff };
  if (code >= 0x7700 && code <= 0x771f) return keyAction({ TriggerMacro: code & 0x1f });
  if (code === 0x7780) return keyAction({ KeyboardControl: "OutputAuto" });
  if (code === 0x7784) return keyAction({ KeyboardControl: "OutputUsb" });
  if (code === 0x7786) return keyAction({ KeyboardControl: "OutputBluetooth" });
  if (code === 0x7c00) return keyAction({ KeyboardControl: "Bootloader" });
  if (code === 0x7c01) return keyAction({ KeyboardControl: "Reboot" });
  if (code === 0x7c02) return keyAction({ KeyboardControl: "DebugToggle" });
  if (code === 0x7c03) return keyAction({ KeyboardControl: "ClearEeprom" });
  if (code === 0x7c16) return keyAction({ Special: "GraveEscape" });
  if (code === 0x7c18) return spaceCadet("Kc9", modifiers(0x01));
  if (code === 0x7c19) return spaceCadet("Kc0", modifiers(0x11));
  if (code === 0x7c1a) return spaceCadet("Kc9", modifiers(0x02));
  if (code === 0x7c1b) return spaceCadet("Kc0", modifiers(0x12));
  if (code === 0x7c1c) return spaceCadet("Kc9", modifiers(0x04));
  if (code === 0x7c1d) return spaceCadet("Kc0", modifiers(0x14));
  if (code === 0x7c1e) {
    return { TapHold: [hidAction(0x28) ?? "No", { Modifier: modifiers(0x12) }, 0xff] };
  }
  if (code === 0x7c50) return keyAction({ KeyboardControl: "ComboOn" });
  if (code === 0x7c51) return keyAction({ KeyboardControl: "ComboOff" });
  if (code === 0x7c52) return keyAction({ KeyboardControl: "ComboToggle" });
  if (code === 0x7c73) return keyAction({ KeyboardControl: "CapsWordToggle" });
  if (code === 0x7c77) return keyAction("TriLayerLower");
  if (code === 0x7c78) return keyAction("TriLayerUpper");
  if (code === 0x7c79) return keyAction({ Special: "Repeat" });
  if (code >= 0x7e00 && code <= 0x7e1f) return keyAction({ User: code & 0x1f });
  return "No";
}

function actionToVia(action: Action): number {
  if (typeof action === "string") {
    if (action === "TriLayerLower") return 0x7c77;
    if (action === "TriLayerUpper") return 0x7c78;
    return 0;
  }
  if ("Key" in action && "Hid" in action.Key) return CODE_BY_HID.get(action.Key.Hid) ?? 0;
  if ("KeyWithModifier" in action) {
    return (modifierBits(action.KeyWithModifier[1]) << 8) | (CODE_BY_HID.get(action.KeyWithModifier[0]) ?? 0);
  }
  if ("LayerToggleOnly" in action) return 0x5200 | action.LayerToggleOnly;
  if ("LayerOn" in action) return 0x5220 | action.LayerOn;
  if ("DefaultLayer" in action) return 0x5240 | action.DefaultLayer;
  if ("LayerToggle" in action) return 0x5260 | action.LayerToggle;
  if ("OneShotLayer" in action) return 0x5280 | action.OneShotLayer;
  if ("OneShotModifier" in action) return 0x52a0 | modifierBits(action.OneShotModifier);
  if ("PersistentDefaultLayer" in action) return 0x52e0 | action.PersistentDefaultLayer;
  if ("TriggerMacro" in action) return 0x7700 | action.TriggerMacro;
  if ("LayerOnWithModifier" in action) {
    return 0x5000 | (action.LayerOnWithModifier[0] << 5) | (modifierBits(action.LayerOnWithModifier[1]) & 0x1f);
  }
  if ("KeyboardControl" in action) {
    const keycodes: Partial<Record<KeyboardAction, number>> = {
      Bootloader: 0x7c00,
      Reboot: 0x7c01,
      DebugToggle: 0x7c02,
      ClearEeprom: 0x7c03,
      OutputAuto: 0x7780,
      OutputUsb: 0x7784,
      OutputBluetooth: 0x7786,
      ComboOn: 0x7c50,
      ComboOff: 0x7c51,
      ComboToggle: 0x7c52,
      CapsWordToggle: 0x7c73,
    };
    return keycodes[action.KeyboardControl] ?? 0;
  }
  if ("Special" in action) return action.Special === "GraveEscape" ? 0x7c16 : action.Special === "Repeat" ? 0x7c79 : 0;
  if ("User" in action) return 0x7e00 | (action.User & 0x1f);
  return 0;
}

export function toViaKeycode(keyActionValue: KeyAction): number {
  if (keyActionValue === "No") return 0;
  if (keyActionValue === "Transparent") return 1;
  if ("Single" in keyActionValue) return actionToVia(keyActionValue.Single);
  if ("TapHold" in keyActionValue) {
    const [tap, hold] = keyActionValue.TapHold;
    if (
      typeof tap === "object" &&
      "KeyWithModifier" in tap &&
      modifierBits(tap.KeyWithModifier[1]) === 0x02 &&
      typeof hold === "object" &&
      "Modifier" in hold
    ) {
      const spaceCadetCodes: Partial<Record<HidKeyCode, Partial<Record<number, number>>>> = {
        Kc9: { 0x01: 0x7c18, 0x02: 0x7c1a, 0x04: 0x7c1c },
        Kc0: { 0x11: 0x7c19, 0x12: 0x7c1b, 0x14: 0x7c1d },
      };
      const code = spaceCadetCodes[tap.KeyWithModifier[0]]?.[modifierBits(hold.Modifier)];
      if (code !== undefined) return code;
    }
    const tapCode = actionToVia(tap) & 0xff;
    if (
      tapCode === 0x28 &&
      typeof hold === "object" &&
      "Modifier" in hold &&
      modifierBits(hold.Modifier) === 0x12
    ) {
      return 0x7c1e;
    }
    if (typeof hold === "object" && "LayerOn" in hold) return 0x4000 | (hold.LayerOn << 8) | tapCode;
    if (typeof hold === "object" && "Modifier" in hold) return 0x2000 | (modifierBits(hold.Modifier) << 8) | tapCode;
    return 0;
  }
  if ("Morse" in keyActionValue) return 0x5700 | keyActionValue.Morse;
  return 0;
}
