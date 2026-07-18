# Runtime configuration and lighting architecture

## Objective

Make the Glove80 firmware a standalone runtime engine whose keymap and lighting
configuration are data, while preserving reliable typing, recovery, and ZMK
Studio compatibility without routine firmware rebuilds.

The finished system should have:

- one fixed total layer capacity, with no user-visible distinction between
  authored, default, reserved, and Studio-created layers;
- a versioned source-controlled data format for factory defaults;
- transactional runtime import, export, editing, and restore;
- one firmware lighting compositor instead of legacy underglow modes;
- stackable lighting activated by keymap layers, explicit toggles, or a host;
- an optional Codex service that is never required for typing or fallback.

## Non-negotiable properties

1. The keyboard must type and expose a recoverable keymap without a computer.
2. A malformed or interrupted configuration update must not strand the keyboard.
3. Key scanning and HID work must take priority over lighting and configuration.
4. Low-battery LED protection remains authoritative.
5. The central half owns configuration; the peripheral receives bounded render
   commands and remains independently recoverable.
6. USB and Bluetooth expose equivalent configuration and lighting semantics.
7. No ordinary editor action requires a firmware reflash or physical unlock.

## Runtime model

### Layer capacity

Use one configurable compile-time capacity because ZMK stores bindings in
fixed-size arrays and avoids heap allocation. Add
`CONFIG_ZMK_KEYMAP_LAYER_CAPACITY`, configured as 8 for this keyboard: the
current defaults occupy six layers, leaving two slots for runtime additions.
Build assertions require the value to cover every factory layer and remain at
or below the current 32-layer state-bitmask limit. The nRF52840 build must retain
comfortable RAM and flash margins.

Capacity is not a layer type. Every occupied slot contains the same runtime
layer record. Empty slots are an internal allocation detail and are not exposed
as fake layers in JSON, Studio, Lightbench, or the CLI.

Runtime records carry no `stock`, `factory`, `static`, `reserved`, or `dynamic`
classification. A layer loaded from the factory snapshot can be renamed,
reordered, rebound, or removed exactly like one created later through Studio.
Its origin is not persisted as layer metadata.

### Canonical configuration

Use a versioned symbolic format as the source of truth. Bindings refer to
behavior names such as `&kp`, `&lt`, and `&mo`; a compiler resolves those names
to firmware-local behavior IDs after validating parameters and capabilities.

```json
{
  "schemaVersion": 1,
  "layers": [
    {
      "id": "base",
      "name": "Base",
      "bindings": []
    }
  ],
  "lightingLayers": [
    {
      "id": "base-lighting",
      "activation": { "type": "keymap-layer-active", "layerId": "base" },
      "cells": {}
    }
  ]
}
```

Lighting association has one representation: the lighting layer's activation
predicate. Keymap records do not also point back to lighting layers, avoiding
two competing sources of activation truth.

The repository data file produces two equivalent artifacts:

- an uploadable runtime configuration for an already-flashed keyboard;
- a read-only factory snapshot bundled with firmware for first boot and
  recovery.

The factory snapshot is data carried by the firmware image, not a separate set
of devicetree layer semantics. Changing the live keymap only uploads data;
flashing is needed only when changing the recoverable snapshot or engine.

### Persistence and recovery

Store mutable configuration in a dedicated 64 KiB flash partition, split into
two 32 KiB slots. Keep the existing 32 KiB Zephyr settings/NVS partition at its
current address for BLE bonds, behavior IDs, and legacy migration; a complete
keymap plus lighting and two generations do not fit there with safe GC margin.

Each runtime slot has a 4 KiB manifest/commit page and up to 28 KiB of payload.
The manifest records format version, generation, payload length and CRC32,
required layer capacity, key count, active layer count, binding encoding, and a
header CRC. Updates erase and write the inactive payload, read it back, and
write the manifest last. There is no separately persisted active-slot pointer;
boot validates both manifests and selects the newest valid generation.

Boot behavior:

1. Load the newest valid runtime record.
2. If none exists or validation fails, load the factory snapshot.
3. If the snapshot is invalid, load a minimal recovery keymap that preserves
   USB, Studio, reset, and bootloader access.

`Restore Defaults` copies the factory snapshot into mutable runtime state. It
does not switch to a different kind of layer.

## Lighting compositor

Legacy solid, spectrum, swirl, and global underglow modes are removed from the
user-facing configuration. The compositor is the only renderer.

Every lighting layer uses the same sparse per-key cell format:

```text
cell = transparent | { color, effect, effect parameters }
```

Initial composition order, from bottom to top:

1. Always-active base lighting.
2. Lighting associated with active keymap layers.
3. Manually toggled lighting layers.
4. The ephemeral host overlay used by Lightbench, the CLI, and Codex.
5. Safety and explicitly reserved system status indicators.

Within a category, use stable priority followed by activation order. A defined
cell replaces the cell below it; a transparent cell reveals the next layer. An
effect's dark phase renders black rather than becoming transparent.

Activation is data, not a distinct layer type:

- `always`
- `keymap-layer-active(layer_id)`
- `toggle(toggle_id)`
- `host-session`
- `system-state(state_id)`

The compositor resolves the active sparse stack when activation changes and
renders only animated cells on the 25 ms low-priority tick. Static changes are
written immediately without requiring a permanent animation tick.

### Host overlay

Refactor the current flat host frame into one sparse highest-priority lighting
layer. Host updates set or unset individual cells. Clearing or disconnecting
the host removes only that layer and immediately reveals the current firmware
composition.

The host overlay remains RAM-only. Persistent lighting definitions use the
transactional configuration API and require an explicit save operation.

## Interfaces

### Standard ZMK Studio

Keep standard Studio RPCs for supported layer CRUD, ordering, naming, bindings,
and saving. Disable `CONFIG_ZMK_STUDIO_LOCKING`; no unlock chord is generated.

Adapt ZMK's keymap arrays so Studio sees occupied runtime layers plus remaining
capacity, without requiring devicetree `reserved` nodes as the public model.

### Rust CLI

Extend `glove80-control` with:

- `config export`
- `config validate`
- `config apply`
- `config restore-defaults`
- transactional progress and verification
- lighting-layer list, enable, disable, and preview commands

The CLI resolves symbolic behavior metadata through Studio before upload and
refuses incompatible schemas or behaviors.

### Lightbench

Evolve Lightbench into a configuration editor with separate views for:

- runtime keymap layers and bindings;
- lighting attached to a selected keymap layer;
- manually toggled lighting layers;
- the live host overlay and previews.

Lightbench and the Rust CLI operate on the same schema. Neither owns a private
configuration format.

## Implementation phases

### Phase 0: stabilize the proven baseline

- Disable Studio locking and remove the generated unlock binding.
- Preserve protocol-v2 per-key static, blink, and breathe rendering.
- Do not flash an intermediate build if a later phase immediately changes the
  storage model.
- Export any valuable existing Studio state before migration.

Exit criteria: clean build, reproducible factory keymap, and known-good recovery
images for both halves.

### Phase 1: capacity and schema

- Add `CONFIG_ZMK_KEYMAP_LAYER_CAPACITY`, set it to 8 for the Glove80, and verify
  its RAM margin.
- Separate stock layer count from allocated capacity inside ZMK.
- Define and test the symbolic JSON schema.
- Build a Rust validator/compiler with golden protocol vectors.
- Keep standard Studio layer operations working.

Exit criteria: authored and Studio-created layers occupy identical runtime
records, and no user-facing tool exposes reserved layers.

### Phase 2: factory snapshot and transactions

- Compile JSON into a read-only factory data blob.
- Load the blob into the runtime keymap on first boot or restore.
- Add checksummed A/B runtime records and atomic activation.
- Carve out the dedicated 64 KiB runtime partition without moving the existing
  settings partition.
- Add export/import and legacy-settings migration tooling.
- Add a minimal recovery keymap fallback.

Exit criteria: defaults, imported configuration, Studio edits, reboot, failed
upload, and restore all converge on the same runtime representation.

### Phase 3: compositor core

- Introduce sparse lighting-layer records and activation predicates.
- Implement deterministic topmost-cell composition.
- Move static rendering off the periodic tick when no animation is active.
- Preserve channel limits, battery protection, and split batching.
- Remove legacy RGB mode/toggle bindings from the default configuration.

Exit criteria: base and multiple active keymap lighting layers compose correctly
on both halves without changing typing latency.

### Phase 4: host overlay and toggles

- Change host updates from a flat frame to sparse overlay cells.
- Add per-cell unset and whole-overlay clear operations.
- Add runtime lighting toggle behaviors and activation events.
- Define disconnect, reboot, and explicit-clear semantics.
- Keep host commands unsecured and non-persistent.

Exit criteria: toggled and keymap-associated lighting stacks correctly, while a
host overlay can replace selected cells and reveal the stack when removed.

### Phase 5: tools and migration

- Add configuration and lighting-layer commands to the Rust CLI.
- Add layer/binding/lighting editing to Lightbench.
- Import the current MoErgo JSON into the canonical schema.
- Export and migrate existing on-keyboard Studio settings.
- Document backup, restore, and recovery workflows.

Exit criteria: routine keymap and persistent lighting changes require no
firmware build, and both tools round-trip one canonical configuration.

### Phase 6: hardware qualification

Verify on physical hardware:

- USB and Bluetooth Studio editing;
- central and peripheral lighting;
- simultaneous keymap-layer, toggle, and host overlays;
- reboot persistence and factory restore;
- interrupted uploads and corrupt-record fallback;
- split disconnect/reconnect;
- idle and low-battery behavior;
- sustained typing during maximum animation load.

Only after these pass should the new storage format replace the current stable
firmware on both halves.

### Phase 7: optional host integrations

Build Codex state lighting and keyboard-driven Codex actions against the stable
runtime APIs. The service is optional and contributes only the host overlay; it
never owns the base keymap or persistent lighting stack.

## Settled initial policy

1. `CONFIG_ZMK_KEYMAP_LAYER_CAPACITY=8`. The resulting left build uses 77,052
   bytes of RAM, 2,624 bytes less than the earlier ten-slot workaround.
2. Manual lighting-toggle state is non-persistent by default, with explicit
   per-toggle opt-in persistence.
3. Safety and firmware status indicators compose above the host overlay.
4. Host overlays remain until explicit clear or reboot; they do not use a short
   lease.
5. Runtime A/B records use a dedicated 64 KiB partition rather than sharing
   Zephyr settings/NVS.
