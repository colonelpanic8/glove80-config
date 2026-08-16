# TailorKey-for-RMK wishlist status

Moosy Research's TailorKey page collected suggestions (C1–C15) for this
RMK stack. This maps each one to what already exists, what this repository
now provides, and what remains genuine firmware work. Firmware paths are
relative to `dependencies/moergo-rmk`.

| # | Suggestion | Status |
|---|---|---|
| C1 | Easy HRM / AutoShift | Done (morse profiles) |
| C2 | Advanced RGB control | Done |
| C3 | Real-time updates | Done (Rynk) |
| C4 | Dual-OS support | **Host tool added**; firmware `OsProfile` is roadmap |
| C5 | Advanced touchpad | Largely done on Go60; runtime speed control is roadmap |
| C6 | Easy alpha-layer switching | **Host tool added**; runtime default-layer switch already existed |
| C7 | Best-practice layouts / capability store | Seeded (`config/tailorkey-v52-bilateral.toml`) |
| C8 | Behavior usage tracking | **Host tool added** |
| C9 | Fully wired, BLE disabled | Unverified; likely compile-time |
| C10 | On-screen display | Roadmap; all protocol pieces exist |
| C11 | Humanized keystroke timing | Roadmap; gap is well mapped |
| C12 | Great RGB UI | Foundation exists (`rynk-wasm` browser stack) |
| C13 | Startup / sleep animations | Roadmap; needs new firmware hooks |
| C14 | Customizable RGB animations | Mostly done (PaletteFX params) |
| C15 | Rename the BLE device | Done |

The three host tools live in this repository as `moergo-layout`
(`just usage`, `just layout …`) and are documented in
[layout-tools.md](layout-tools.md). C8, C4, and C6 also ship as Rynkbench
UI: the Advanced mode's **Usage** tab (reference tracking, orphan and
reachability warnings) and **Transforms** tab (one-click Ctrl↔GUI swap, and
a bidirectional alpha-layout switcher — any pair of QWERTY, Colemak,
Colemak-DH, Dvorak — with a per-layer migration mode: substitute alphas in
place, move bindings mnemonically so shortcuts follow their letters, or
leave positional layers untouched; written live to the keyboard, or to an
offline workspace for export).

## Already done in the stack

- **C1 — HRM/AutoShift.** `[behavior.morse.profiles.*]` names a timing
  profile once and typed keys reference it: `MT(KC_A, LGui, hrm_pinky)`,
  `TH(KC_1, LSFT(KC_1), autoshift)`. The TailorKey port in
  `config/tailorkey-v52-bilateral.toml` collapses the source layout's 58
  duplicated hold-tap records into seven profiles, using
  `opposite_hand_hold` for bilateral combinations instead of ZMK's
  eight per-finger helper layers.
- **C2/C14 — RGB.** ~60 Rynk lighting commands: transient overlay, durable
  per-layer scenes, ordered conditional scenes (layer/connection/battery/
  charge conditions), 13 PaletteFX effects with 16 palettes, and per-effect
  parameters tunable at runtime (`moergo-control lighting params`).
- **C3 — Real-time updates.** The Rynk protocol runs over USB HID and BLE;
  `moergo-control config apply` performs read-modify-verify of the full
  runtime state with no flash.
- **C5 — Touchpad.** Go60 pointing devices take per-device modes
  (`cursor`, `scroll` — knob-style emulation, `press`) with per-layer
  overrides; this config's Magic layer turns the right pad into a
  press-and-drag tool. Momentary speed scaling exists compiled
  (`[[behavior.mouse_layer_scale]]`); making it runtime is roadmap.
- **C15 — BLE name.** Runtime `bluetooth_name` (with `{slot}` template),
  `moergo-control connection name set`, and Rynk `SetBleName`.

## Added by this repository's host tooling

- **C8 — Behavior usage tracking** (`just usage`): per-layer activation
  sites and reachability, morse/macro/profile reference counts, orphan and
  dangling-reference warnings, `--check` for CI.
- **C4 — Dual-OS support** (`just layout os`): generates the other OS's
  variant of a config by swapping Ctrl/GUI across every binding; switching
  OS is one `config apply`, in real time. The firmware half — a persisted
  `OsProfile` so one keymap serves both OSes without reapplying — has an
  exact template to copy: `UnicodeMode` (`dependencies/rmk/rmk-types/src/
  unicode.rs`) is already a persisted, runtime-cycled OS enum, and the
  modifier resolution point is `resolve_explicit_modifiers` in
  `dependencies/rmk/rmk/src/keyboard.rs`.
- **C6 — Alpha switching** (`just layout alpha`): Colemak, Colemak-DH, or
  Dvorak variants generated from a QWERTY source, following keycodes into
  hold-taps and autoshift pairs. Independently, the persistent default
  layer is already runtime-switchable (`moergo-control keymap default`,
  Rynk `SetDefaultLayer`), so an alternate alpha layer in a spare slot is
  a one-command switch.
- **C7 — Best-practice layouts.** Runtime configs are single shareable
  TOML files; `config/tailorkey-v52-bilateral.toml` is a complete,
  hand-audited TailorKey v5.2 port to seed from. Beyond whole layouts,
  `just layout preset apply` merges *partial* fragments — home row mods
  with lighting, a symbols layer, a Magic-style controls layer — into any
  config, resolving layer/morse/macro slots at apply time (`presets/`,
  documented in [layout-tools.md](layout-tools.md)).

## Firmware roadmap (rmk-assembly fold branches)

These need changes in the nested firmware stack and must go through its
assembly workflow — topic branches folded by fork-assembler, never commits
onto the generated branch.

- **C11 — Humanized keystroke timing.** Today only the explicit macro
  `delay` operation is user-controllable; `execute_macro` hard-codes
  2/12/1 ms waits and `WM()` sends modifier+key in a single HID report
  (that zero-delay report is exactly what endpoint anti-injection tooling
  flags). The cheap first step needs no protocol change: `tap_interval_ms`
  is already configured, persisted, and wire-exposed
  (`SetBehaviorConfig`), but nothing in the key path consumes it — wiring
  it into `execute_macro` and `process_key_action_tap`
  (`dependencies/rmk/rmk/src/keyboard.rs`) makes macro pacing a runtime
  setting. Splitting `WM()`'s single-report semantics into
  modifier-then-key with a configurable gap is the deeper, behavior-
  changing second step.
- **C10 — On-screen display.** No firmware work needed: Rynk already
  pushes `LayerChange`, `ConnectionChange`, `BatteryStatusChange`, and
  `LedIndicatorChange` topics. What's missing is a small host daemon that
  subscribes and draws an overlay; `src/` in this repository
  (`rmk-attentiond`) is prior art for the transport half.
- **C13 — Startup/sleep animations.** Sleep is currently only a gate
  (`SleepStateEvent` zeroes output in `crates/moergo-rmk/src/lighting.rs`);
  an entry/exit animation would hook there. A boot animation has no hook
  at all and must respect the ~10 s nRF watchdog budget during init, so it
  needs a non-blocking design in the lighting service.
- **C4 (firmware half)** — see above: `OsProfile` mirroring `UnicodeMode`,
  a swap table in modifier resolution, and optionally OS-conditional
  default layers.
- **C5 (runtime speed)** — expose `mouse_layer_scale` through a Rynk
  command instead of compile-time TOML.
- **C9 — BLE fully off.** Not investigated in this pass; radio behavior is
  compiled, so if supported it will be a build-time switch, not runtime
  config. (The nRF52840's radio is part of the SoC either way.)
- **C12 — RGB UI.** The `rynk-wasm` crates (WebHID/WebBluetooth) are the
  intended browser-configurator foundation; a lighting-first UI on top of
  them is application work outside this repository.
