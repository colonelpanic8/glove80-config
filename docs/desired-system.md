# Desired Glove80 runtime system

## Purpose

The target is a Glove80 firmware and host-tooling system that remains a complete,
reliable keyboard without a computer, while allowing keymaps and per-key
lighting to be changed live over USB or Bluetooth.

The desktop integration is an enhancement, not part of the keyboard's critical
path. Typing, layer switching, saved configuration, ordinary lighting, recovery,
and firmware updates must continue to work when Lightbench, a background service,
or Codex integration is absent.

This document describes the desired behavior independently of the firmware
framework. The implementation may use ZMK, RMK, or another substrate as long as
it satisfies the same acceptance criteria.

See [`rmk-evaluation.md`](./rmk-evaluation.md) for the current assessment of RMK
against these requirements.

## Hardware scope

- MoErgo Glove80 with two nRF52840 halves.
- Left half is the normal central and USB-connected half.
- Right half communicates wirelessly with the central during normal use.
- Each half has 40 addressable WS2812-compatible key LEDs.
- Each half also has a separate PWM-controlled LED near the power button.
- Both halves retain their existing UF2 bootloaders and independent recovery
  path.
- Battery reporting, low-power operation, and the keyboard's safe LED current
  limits must be preserved.

## Non-negotiable properties

1. The keyboard must type correctly without any desktop software.
2. USB and Bluetooth must expose equivalent keymap and lighting semantics.
3. Loss of the split connection may disable the unavailable half, but must not
   break typing or configuration on the central half.
4. Key scanning and HID reporting must take priority over rendering, storage,
   configuration transfer, and logging.
5. A malformed configuration or interrupted write must not strand the keyboard.
6. Both halves must always retain a documented physical and programmatic route
   into their bootloaders.
7. Firmware must enforce the Glove80's conservative LED current limits even if
   a host requests brighter values.
8. Routine keymap and persistent-lighting changes must not require a firmware
   recompile or reflash.
9. The configuration interface must not require a physical unlock chord.
10. A known-good recovery image and factory configuration must remain available
    throughout development and migration.

## Runtime keymap model

### Uniform layers

The keyboard has one configurable compile-time maximum layer count. The Glove80
configuration initially sets this maximum to eight.

There is no runtime distinction between factory, static, reserved, dynamic, or
editor-created layers. Every occupied slot is the same mutable runtime record.
Unused capacity is an allocation detail rather than a user-visible layer type.

A runtime layer contains at least:

- a stable identifier;
- a mutable display name;
- one binding for each of the 80 logical key positions.

Reordering or renaming a layer must not break references from bindings or
lighting. Layer references therefore use stable identifiers in the canonical
configuration and are resolved to firmware-local slot numbers when installed.

### Data-defined defaults

The default keymap is authored as versioned data, not as a second immutable
keymap model. The firmware carries a read-only factory snapshot of that data for
first boot and recovery. On first boot or restore, the snapshot is copied into
the same runtime representation used for later edits.

A layer originating in the factory snapshot can be renamed, reordered, rebound,
or removed exactly like a layer created later. Its origin is not persisted as
runtime metadata.

### Editing and persistence

The runtime interface must support:

- inspecting capacity and occupied layers;
- adding and removing layers within capacity;
- renaming and reordering layers;
- reading and replacing bindings;
- applying a complete configuration transactionally;
- exporting the complete live configuration;
- discarding unsaved edits where supported;
- restoring the factory snapshot;
- validating behavior names, parameters, layer references, key counts, and
  target capabilities before activation.

An interrupted complete-configuration update must leave either the previous
valid configuration or the complete new configuration active, never a hybrid.
Small editor operations may be represented internally as deltas, but exporting
and importing must round-trip through one canonical versioned schema.

At boot, recovery order is:

1. newest complete and valid runtime configuration;
2. factory snapshot;
3. minimal recovery keymap containing USB, configuration, reset, and bootloader
   access.

## Lighting model

### One compositor

There is one per-key lighting renderer. Legacy global modes such as solid,
spectrum, swirl, and an independent underglow toggle are not separate operating
modes. Lighting cannot accidentally stop working because the firmware is in a
different RGB mode.

Every lighting definition is a sparse map from logical key index to a cell:

```text
transparent | { color, effect, effect parameters }
```

Missing cells are transparent and reveal the next lower layer. A defined
effect's dark phase renders black; it does not become transparent.

Required initial effects are:

- static;
- blink with period, phase, and duty cycle;
- breathe with period and phase.

The implementation should make adding effects possible without changing the
meaning of existing configuration records.

### Stack and activation

Lighting is composited from bottom to top:

1. always-active base lighting;
2. lighting associated with active keymap layers;
3. independently toggleable lighting overlays;
4. the sparse live host overlay;
5. authoritative safety and firmware-status indicators.

Lighting layers are not separate structural types. They share one record format
and differ only by activation predicate:

- always active;
- active while a referenced keymap layer is active;
- controlled by a named toggle;
- active for a host session;
- active for a firmware system state.

Within a class, priority and stable activation order determine composition.
Defining a cell replaces the composed cell below it.

Toggle state is non-persistent by default, with explicit per-toggle opt-in for
persistence.

### Host overlay

Lightbench, the Rust CLI, and future Codex integration all manipulate the same
sparse, RAM-only host overlay. They do not replace the persistent base or layer
lighting configuration.

Host operations must support:

- setting one or more cells;
- setting static, blink, or breathe effects;
- unsetting selected cells;
- clearing the complete overlay;
- querying capabilities and physical key count;
- reading back the complete current overlay, including effect parameters and
  remaining TTLs;
- atomically replacing the complete overlay with a supplied sparse map, so a
  client can force the keyboard into a known state in one idempotent
  operation;
- reporting partial application when a split peripheral is unavailable.

A cell write may carry an optional firmware-enforced TTL; when it expires the
cell reverts to transparent. The default is no TTL. Expiry is handled by the
firmware, not the host, so an indicator written by a crashed client cannot
outlive the state it describes when its writer opted into expiry.

Cells without a TTL survive until explicit clear or keyboard reboot. A daemon
crash does not trigger an implicit timeout that unexpectedly changes the
keyboard's appearance.

### Split rendering

The central owns the active configuration and host protocol. Each half renders
its own 40 physical LEDs. Central-to-peripheral commands are bounded, versioned,
and tolerant of retransmission and reconnection.

Persistent layer lighting should be renderable on the peripheral from compact
configuration/state synchronization. Live host changes may use bounded batches.
No lighting transfer may block key events.

The power-button LED is independently controllable as a firmware-status output
and must work on both halves.

## Host interfaces

### Canonical configuration

One versioned symbolic schema is shared by all tools. It represents runtime
layers, symbolic bindings, stable layer references, lighting layers, activation
predicates, toggle definitions, and required target capabilities.

The schema must not persist implementation-specific distinctions such as
factory versus dynamic layers. Tools query target capacity and capabilities
rather than assuming them.

### Manual UI

Glove80 Lightbench remains daemon-independent. A supported browser can connect
directly to the keyboard and manually edit or preview every key LED. The UI
eventually expands to persistent keymap and lighting-layer editing without
inventing a private data format.

### Rust CLI

The native command-line tool is written in Rust. It supports live lighting,
configuration validation, export, transactional apply, restore, capability
inspection, and programmatic bootloader entry.

Python is not part of the normal control path.

### Optional background service

A background service may coordinate device ownership and translate application
state into host-overlay lighting. Its absence must affect only that overlay.

The initial Codex state vocabulary is:

- idle;
- working or thinking;
- waiting for approval or user input;
- unread completion;
- completed;
- error.

Keyboard-driven Codex actions use explicit otherwise-unused key actions and
require deliberate bindings. Ordinary typing must never be interpreted as a
Codex command.

## Firmware update and recovery

- Either half can enter its UF2 bootloader through a physical binding.
- Once compatible firmware is installed, the central can request bootloader
  entry for itself and the peripheral programmatically.
- Peripheral update requests occur before rebooting the central transport.
- Reset images or an equivalent recovery flow erase runtime configuration while
  preserving a clear route back to normal firmware.
- Firmware images are half-specific; combined artifacts are archival rather
  than routine flash targets.
- A migration to a different firmware framework is tested one capability at a
  time and retains the known-good ZMK images until the replacement passes the
  complete qualification matrix.

## Performance and power

- LED animation runs on a low-priority asynchronous task or work queue.
- Static updates do not require a permanent high-frequency animation tick.
- Only animated cells are recomputed on each render tick.
- Configuration validation and flash erases yield between bounded operations.
- Maximum animation load and configuration writes must not measurably drop key
  events or increase normal typing latency.
- Idle behavior turns off or reduces unnecessary LED and peripheral activity.
- Battery reporting works for both halves.
- Low-battery protection and current clamping override host lighting requests.

## Qualification matrix

A candidate implementation is not ready to replace the current firmware until
all of the following pass on physical hardware:

- left-local typing over USB;
- host typing over Bluetooth;
- complete two-half typing over the wireless split;
- disconnect and reconnect of the right half;
- configuration editing and persistence over USB;
- configuration editing and persistence over Bluetooth;
- eight uniform editable layer slots;
- reboot, factory restore, corrupt-record fallback, and interrupted update;
- programmatic bootloader entry for each half;
- static, blink, and breathe on both halves;
- simultaneous base, active-layer, toggle, host, and status lighting;
- sparse host clear revealing the lower composed stack;
- power-button LED control on both halves;
- battery reporting and low-battery behavior;
- sustained maximum-rate typing during animation and flash activity;
- safe recovery to a known-good image after every destructive test.
