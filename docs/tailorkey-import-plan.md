# Plan: Importing full MoErgo/TailorKey configurations (aptmak-blue case)

## Background

`aptmak-blue _1_.json` is a valid MoErgo layout export (a TailorKey-style layout),
but it fails to import through the shared Rust parser used by both
`glove80-control config validate` and RynkBench:

```text
layer 0, editor key 35 (r3,c1) behavior '&HRM_left_pinky_v1B_TKZ' cannot be represented
```

The failure is not malformed input. The layout exceeds the supported subset of the
current importer and document model in four independent ways:

1. **Custom behaviors.** 275 bindings across 49 user-defined behavior names,
   backed by 35 hold-taps and 26 macros. The MoErgo importer
   (`dependencies/glove80-rmk/crates/glove80-config/src/moergo.rs:470`) only
   accepts bindings that collapse to a single Rynk/VIA key action; it has no
   pathway to emit hold-tap (morse) or macro definitions.
2. **Layer references above 15.** Layer actions target layers 16–19, but the
   intermediate VIA-style `u16` encoding restricts layer indices to `0..15`
   (`moergo.rs:484`) even though native typed Rynk actions can address higher
   layers.
3. **Capacity.** The document has 20 layers; the current firmware config
   (`config/firmware.toml`, `[rmk] layers = 8` region) builds only 8 runtime
   layers, and combo/macro/morse table sizes are similarly modest.
4. **Document/transfer model gaps.** The configuration snapshot exposed to
   RynkBench carries only keymap and lighting
   (`dependencies/glove80-rmk/crates/glove80-config-wasm/src/types.rs:44`), and
   the import path writes only keys and lighting
   (`rynkbench/src/config/transfer.ts:43`). The JSON's 11 combos, macros, and
   hold-taps would be silently dropped even if parsing succeeded. Its two
   per-layer mouse-scaling input listeners have no runtime equivalent at all.

These map onto three repos: `glove80-rmk` (fork `colonelpanic8/rmk`, importer +
document model + firmware), `rynkbench` (UI transfer path), and this repo
(firmware capacity config). Note the `assembled` branch of the rmk fork is
generated from `fold/*` topic branches — all changes land on topic branches,
never on `assembled` directly.

## Goal

`glove80-control config validate` accepts the file with zero errors, and a
RynkBench import followed by a flash reproduces the layout's behavior on the
keyboard: home-row mods, all 20 layers, combos, and macros. Mouse-scaling
listeners are explicitly out of scope for behavior parity (see Phase 5).

## Phases

Ordered so that each phase is independently landable and testable; 1–3 are
prerequisites for the end-to-end goal, 4–5 are follow-ons.

### Phase 1 — Native typed actions in the importer (removes the VIA u16 bottleneck)

- In `moergo.rs`, stop lowering every binding to the VIA `u16` key space.
  Convert to Rynk's native typed `KeyAction`/`Action` representation, which
  already supports layer indices beyond 15.
- Acceptance: a synthetic MoErgo JSON using `&mo 17` / `&lt 19 A` validates and
  round-trips through the document model.

### Phase 2 — First-class hold-taps and macros

- Extend the importer to translate:
  - MoErgo `holdTaps` → Rynk morse/hold-tap table entries, preserving per-entry
    timing (tapping-term, quick-tap, flavor) rather than collapsing to `&mt`.
  - MoErgo `macros` → Rynk macro definitions (macro space encoding).
  - Bindings that reference a named behavior (`&HRM_left_pinky_v1B_TKZ`) →
    references into those tables.
- Extend the config document model (`glove80-config` + `glove80-config-wasm`
  `types.rs`) so snapshots carry morse, macro, and combo sections alongside
  keymap and lighting. This is the shared model used by both the CLI and
  RynkBench, so it lands once in `glove80-rmk`.
- Translate `combos` (11 in this file) — Rynk already supports combos at
  runtime; this is document-model plumbing plus importer mapping.
- Acceptance: `config validate` on aptmak-blue reports no unrepresentable
  bindings; a diff of imported vs. source shows every hold-tap/macro/combo
  accounted for.

### Phase 3 — Firmware capacity

- Raise in `config/firmware.toml` (and keep `keyboard.toml` consistent):
  layers 8 → ≥20, and combo/macro-space/morse maxima to cover 11 combos,
  26 macros, 35 hold-taps, with headroom.
- Known risks from prior work: RAM pressure (see commit `baa022fb`, which
  already had to squeeze a widened cell into RAM) and the event-subscriber
  budget (overflow panics at runtime — see memory notes). Budget check the
  build: verify flash/RAM usage after the bump, and consider making layer
  count a per-config knob rather than a global maximum if RAM is tight.
- Acceptance: firmware builds for both halves; boots; existing 8-layer config
  still works after flash.

### Phase 4 — RynkBench transfer path

- Update `rynkbench/src/config/transfer.ts` (and the UI around it) to read and
  write the new snapshot sections: combos, macros, morse/hold-taps.
- Import UX: surface a per-section summary ("20 layers, 11 combos, 26 macros,
  35 hold-taps imported; 2 input listeners skipped") instead of silent drops.
- Regenerate `rynkbench/src/vendor/rynk-wasm` bindings after the wasm crate's
  types change.
- Acceptance: importing aptmak-blue in RynkBench shows all sections populated
  and writes them to the keyboard; a subsequent read-back snapshot matches.

### Phase 5 — Mouse-scaling input listeners (decision required)

The two ZMK per-layer mouse input listeners have no Rynk runtime equivalent.
Options, in rough preference order:

1. **Skip with a warning** (cheapest; ship Phases 1–4 without blocking on this).
2. Approximate with Rynk's existing mouse-key speed settings on those layers.
3. Add a small runtime feature to the rmk fork for per-layer pointer scaling.

Default plan: option 1 now, revisit 3 only if the imported layout feels wrong
in use.

## Cross-cutting rules

- All rmk-fork changes go on `fold/*` topic branches, then regenerate
  `assembled`; verify the rebuild is lossless (`git diff <old> <new>` empty
  beyond the intended change).
- The importer changes should be covered by unit tests in `glove80-config`
  using trimmed fixtures cut from aptmak-blue (one per feature: high layer
  refs, named hold-tap, macro, combo).
- Keep `config validate` as the fast feedback loop throughout; it exercises the
  same code path as RynkBench without hardware.

## Sequencing and effort

Phases 1 and 3 are independent and can proceed in parallel. Phase 2 depends on
1 (typed actions) and is the bulk of the work. Phase 4 depends on 2's document
model. Rough weight: Phase 2 ≈ half the total effort, Phase 4 ≈ a quarter,
Phases 1/3/5 the rest.
