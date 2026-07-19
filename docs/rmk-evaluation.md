# Evaluation of RMK for the Glove80 runtime system

## Executive conclusion

[RMK](https://github.com/haobogu/rmk) is a credible alternative to ZMK and is
better aligned with the preferred implementation language and several desired
runtime-configuration properties. It should receive a bounded hardware spike
before more custom ZMK persistence and compositor work is undertaken.

It is not currently a drop-in replacement. RMK supplies USB, BLE, wireless
split operation, fixed-capacity runtime keymaps, Vial editing, persistent
storage, and an asynchronous event architecture. It does not currently supply
the Glove80 hardware port or a working RGB controller. The official roadmap
still lists RGB as unfinished. Adopting RMK therefore exchanges substantial ZMK
core customization for a substantial initial hardware and lighting port.

Recommendation: preserve ZMK as the known-good implementation, build an RMK
candidate alongside it, and decide only after RMK demonstrates USB typing,
wireless split typing, Vial persistence, recovery, and one controlled key LED on
each half.

## Evaluation snapshot

This evaluation was performed on 2026-07-18 against:

- RMK `main` commit `1156f82bddecda6d3bcfce60adb7a6504083b9e8`;
- latest published GitHub release shown as `rmk-v0.8.2` from 2025-12-18;
- the current RMK documentation under `rmk.rs/main`;
- the Glove80 board definitions vendored in this repository.

RMK is active and pre-1.0. A production Glove80 port should pin an exact commit
or vendored subtree rather than follow `main` implicitly.

Primary references:

- [RMK repository and feature overview](https://github.com/haobogu/rmk)
- [Wireless split documentation](https://rmk.rs/main/docs/features/split_keyboard)
- [Vial support](https://rmk.rs/main/docs/features/vial_support)
- [Layer configuration](https://rmk.rs/main/docs/configuration/layout)
- [Persistent storage](https://rmk.rs/main/docs/features/storage)
- [RMK development roadmap](https://rmk.rs/main/docs/development/roadmap)
- [`sequential-storage` documentation](https://docs.rs/sequential-storage)

## Requirement fit

| Requirement | RMK status | Consequence |
| --- | --- | --- |
| nRF52840 | Native | The Glove80 MCU is supported directly. |
| USB HID | Native | Central can expose normal USB keyboard and Vial transports. |
| BLE HID | Native | RMK supports multiple BLE profiles and reconnection. |
| Wireless split | Native | nRF52 central/peripheral BLE split is an established RMK mode. |
| USB/BLE automatic selection | Native | RMK can use USB when attached and BLE otherwise. |
| Eight runtime layers | Native model | Layer count is a compile-time capacity; omitted defaults are filled as editable empty layers. |
| Live keymap editing | Native through Vial | No firmware reflash is required for ordinary key changes. |
| Persistent keymap | Native | RMK stores key actions and configuration in flash. |
| No physical unlock | Native option | `vial_insecure = true` starts Vial unlocked. |
| Factory defaults | Mostly native | Compiled default `KeymapData` initializes/reset storage, matching the desired factory-snapshot concept. |
| Stable named layer CRUD/order | Missing | Vial exposes fixed numbered layers rather than the full stable-ID model. |
| Whole-config atomic apply | Missing | RMK persists individual records; a custom transaction layer is needed for atomic bulk imports. |
| Per-key WS2812 lighting | Missing | Must be implemented for the Glove80. |
| Stackable lighting compositor | Missing | Must be implemented as an RMK application/library extension. |
| Host lighting protocol | Missing | Requires a custom USB/BLE command path and Lightbench transport update. |
| Right-half lighting transfer | Missing | Requires extending or wrapping RMK's split protocol. |
| Programmatic central bootloader | Native | RMK supports the Adafruit nRF52 `GPREGRET=0x57` convention. |
| Programmatic peripheral bootloader/update | Partial | Current source has split DFU machinery, but Glove80 bootloader compatibility must be proven. |
| Battery and power-button LED | Framework pieces exist | The exact VDDH measurement and PWM wiring require a Glove80 port. |

## What RMK gives us

### Rust and Embassy

RMK is predominantly Rust and uses Embassy's asynchronous executor and Nordic
drivers. Key scanning, USB, BLE, storage, split communication, battery
processing, displays, and user processors are expressed as cooperating async
tasks.

This is a good fit for the desired compositor: layer changes, connection state,
battery state, and host updates can feed a renderer without placing lighting in
the key-scan path. RMK already publishes layer and connection events and sends
layer state from the central to peripherals.

### Runtime keymap behavior

RMK's `KeymapData` has a compile-time `NUM_LAYER` and a uniform action array.
Vial reads and writes those actions at runtime. The TOML configuration explicitly
supports declaring a larger layer count than the number of authored defaults;
the remaining layers are filled automatically and editable through Vial.

This directly matches the most important layer decision:

- one total capacity;
- no factory-versus-dynamic runtime record type;
- defaults copied into mutable runtime state;
- no reflash for routine binding changes.

The mismatch is product semantics rather than memory layout. Standard Vial
layers are numbered slots and do not provide the desired stable string IDs,
names, arbitrary add/remove semantics, or independent reordering. Those can be
added by a canonical import/export layer, or the product can deliberately adopt
fixed numbered slots and simplify the requirement.

### Vial over USB and BLE

RMK uses Vial as its standard runtime editor and documents wireless editing.
Its insecure mode removes the physical unlock requirement. Vial is mature as a
keymap protocol, which would let the project delete a meaningful amount of
custom ZMK Studio layer-storage work.

Vial does not express the proposed persistent lighting schema. It would remain
the standard binding editor while Lightbench or the Rust CLI manages the custom
configuration sections that Vial cannot represent.

### Storage

RMK uses the Rust `sequential-storage` map, including CRC-protected records and
power-failure recovery. It stores key actions independently and minimizes flash
erase cycles. This is a stronger starting point than creating a general flash
store from scratch.

It is not equivalent to the proposed A/B complete-configuration transaction.
If a host changes hundreds of keys and loses power halfway through, the valid
individual writes may describe a partial bulk update. We must either:

1. add a generation/manifest layer for complete imports;
2. stage bulk changes in a second storage region before publishing them; or
3. relax atomicity for editor operations while retaining it for canonical
   imports.

The third option is likely the best balance: keep Vial's durable per-key edits,
and make only `config apply` a full transaction.

### Bootloader support

RMK contains explicit Adafruit nRF52 bootloader support using the same
`GPREGRET=0x57` mechanism already proven by the current firmware. Its nRF52840
split example also assumes a bootloader at `0xf4000`, matching the Glove80's
bootloader location.

The example application starts at `0x1000`, while the current Glove80 application
starts at `0x26000` after a Nordic SoftDevice region. A spike must prove that the
existing Glove80 bootloader launches an RMK application linked at the expected
address and that RMK's Nordic SDC/MPSL stack works correctly in the remaining
flash layout. This must be verified before either half loses its known-good
image.

## Missing work and risk

### RGB is not implemented by RMK

RMK defines RGB-related key-action values for Vial compatibility, but the host
conversion code reports RGB configuration as unsupported and the official
roadmap still marks the RGB controller as incomplete.

For the Glove80 we must implement:

- WS2812 signaling through nRF52840 SPIM at the board's proven timing;
- the left and right LED-chain mappings;
- the left `P0.31` and right `P0.19` LED power-enable GPIOs;
- GRB channel ordering;
- brightness and total-current clamping;
- static, blink, and breathe rendering;
- sparse stack composition;
- power and idle behavior;
- central-to-peripheral live updates.

This is the largest adoption risk. The renderer itself is naturally expressed
as an Embassy task, but split protocol integration requires modifying RMK or
maintaining an extension point upstream does not currently expose.

### No existing Glove80 RMK port was found

The port must reproduce the existing board knowledge:

- the two distinct 6-by-7 matrix pin maps and logical 80-key transform;
- pull direction and diode orientation;
- 32 kHz clock source and Nordic radio configuration;
- USB VID/PID, product strings, and serial identity policy;
- VDDH battery measurement and peripheral battery forwarding;
- power-button PWM LED;
- LED-chain SPI pins and enable controls;
- left/right UF2 family and memory layout;
- safe sleep and wake behavior.

RMK has examples for the same MCU, bootloader location, USB+BLE central, BLE
peripheral, battery events, and split connection. That lowers the conceptual
risk, but physical qualification remains mandatory.

### Custom host interface

Lightbench currently speaks a custom ZMK Studio protobuf transport. RMK would
require a new transport, likely a versioned command set carried by raw HID/Vial
packets over USB and RMK's BLE host channel.

The protocol semantics can remain unchanged—capability query, bounded pixel
updates, effects, sparse clear, and bootloader command—but the framing and
browser connection code must be replaced. The Rust CLI can share the same new
codec.

### Split extension maintenance

RMK's split message enum is currently crate-private and contains keyboard,
pointing, connection, layer, battery, and firmware-update messages. Adding
lighting batches cleanly likely requires one of:

- an upstream generic application-message hook;
- a small maintained RMK fork;
- vendoring RMK as a git subtree and carrying the extension directly.

Because custom lighting and persistent configuration will touch RMK internals,
the final implementation should not depend on an unpinned moving `main` branch.

### Project maturity

RMK is active, widely noticed, and already used by real keyboards, but remains
pre-1.0 and is changing quickly. The documentation roadmap still contains
important incomplete features. This increases upgrade and regression cost
compared with the mature Glove80-specific ZMK board support already in hand.

## Comparison with the current ZMK implementation

| Area | Current ZMK branch | RMK candidate |
| --- | --- | --- |
| Physical Glove80 support | Proven on both halves | Must be ported and qualified |
| USB and BLE typing | Proven | Framework-supported, board unproven |
| Wireless split | Proven | Framework-supported, board unproven |
| Programmatic bootloader | Proven for both halves | Central mechanism exists; split path unproven |
| Host per-key lighting | Static/blink/breathe proven on both halves | Must be reimplemented |
| Lightbench and Rust CLI | Working | Transport must be migrated |
| Runtime layer editing | ZMK Studio plus custom capacity work | Vial is native |
| Uniform eight-layer capacity | Implemented locally | Native fixed-capacity model |
| Persistent keymap store | ZMK settings plus planned transaction layer | Native sequential store, no whole-config transaction |
| Factory data snapshot | Planned | Defaults already initialize the mutable keymap; canonical export still needed |
| Lighting compositor | Planned on top of working ZMK LED code | New Rust implementation required |
| Primary implementation language | C, devicetree, Kconfig | Rust and TOML |
| Long-term custom architecture | Increasing ZMK fork surface | Increasing RMK fork/application surface |

Staying on ZMK minimizes near-term hardware risk and preserves all proven
lighting work. Moving to RMK better matches the preferred language and avoids
rebuilding basic runtime-keymap infrastructure, but resets hardware confidence
and lighting progress.

## Proposed bounded spike

The spike lives beside the existing ZMK firmware in this monorepo. It does not
delete or mutate the known-good ZMK build.

### Stage 1: compile-only board skeleton

- Pin an exact RMK revision.
- Add left and right binaries with the Glove80 memory layout.
- Encode both matrix pin maps and the logical-key transform.
- Produce half-specific UF2 files without flashing.
- Inspect image ranges to ensure no overlap with bootloader or retained flash.

Exit: reproducible release builds and verified UF2 address ranges.

### Stage 2: left-local safety test

- Flash only the left half.
- Confirm it boots, enumerates over USB, and types the left-side keys.
- Confirm its physical bootloader binding.
- Confirm programmatic bootloader entry and return to the known-good ZMK image.

Exit: the experimental firmware cannot trap the left half.

### Stage 3: wireless split

- Flash the right candidate only after left recovery is proven.
- Pair the halves and verify every right-side key.
- Test right power-cycle and split reconnect.
- Test simultaneous fast typing across halves.
- Verify programmatic right bootloader or split update before relying on it.

Exit: typing and recovery work on both halves.

### Stage 4: USB, BLE, Vial, and storage

- Verify USB and BLE host typing.
- Expose eight uniform Vial layers with insecure/unlocked access.
- Edit and persist bindings over USB and BLE.
- Test reset to defaults and interrupted individual writes.
- Export the live keymap into the canonical schema.

Exit: RMK replaces the custom ZMK layer-capacity/storage work for ordinary
editing.

### Stage 5: minimum viable lighting

- Drive LED 0 on the left with current clamping.
- Drive one LED on the right from a central command.
- Control both power-button LEDs.
- Subscribe lighting to layer and connection events.
- Measure typing while updating LEDs.

Exit: no unknown hardware or split blocker prevents the planned compositor.

### Stage 6: decision

Choose RMK only if all mandatory spike exits pass without requiring an unsafe
bootloader replacement or an unmaintainable split fork.

If RMK passes, implement the complete compositor and host protocol in Rust,
then repeat the full qualification matrix in
[`design-goals.md`](./design-goals.md). If it fails,
the spike remains useful documentation and the ZMK implementation continues
from its preserved baseline.

## Go/no-go criteria

### Go

- Existing UF2 bootloaders launch and recover RMK reliably.
- USB, BLE, and the wireless split are stable under real typing.
- Vial provides reliable unlocked runtime edits on both transports.
- Storage survives reboot and interrupted writes predictably.
- WS2812 and power-button LEDs can be driven safely from Embassy.
- A bounded right-half lighting message can coexist with key traffic.
- The required custom RMK changes can be maintained in a small, reviewable
  subtree or upstreamable extension.

### No-go

- RMK requires replacing the proven bootloaders before basic evaluation.
- Split input or reconnection is less reliable than the current firmware.
- The Glove80's LED signaling conflicts with RMK's Nordic BLE timing.
- Vial over BLE is not usable on the intended desktop/browser path.
- Custom split and host hooks require a broad permanent fork of RMK internals.
- Power use or typing latency materially regresses.

## Current recommendation

Run the spike now, before implementing the complete ZMK transactional store or
lighting compositor. The project is at the point where RMK could eliminate a
large category of planned C work, while the existing ZMK firmware is still a
strong recovery baseline.

Do not declare a migration or flash both halves based on compile success alone.
The decision hinges on the Glove80-specific bootloader, split, battery, and LED
tests—not on RMK's general feature list.
