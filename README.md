# Glove80 ZMK Studio Config

Custom Glove80 firmware for Ivan's layout, generated from the MoErgo layout
`34957465-9943-4236-852c-d88044706dcb`.

This is a monorepo: the MoErgo Glove80 ZMK source is vendored as a Git subtree
under [`zmk/`](./zmk/), and the custom firmware code is maintained there as
ordinary source changes. There is no source-patching layer or separate firmware
fork to coordinate.

The keyboard remains fully functional with the generated keymap when Studio is
not connected, while Studio can edit and persist bindings at runtime over USB
or Bluetooth. The host-lighting protocol is protobuf over ZMK Studio RPC; it
does not depend on the JavaScript keymap generator.

## ZMK Studio

- Studio configuration is intentionally unlocked whenever connected; no
  physical unlock chord is required.
- USB Studio communication uses the CDC/ACM serial transport on the left half.
- Bluetooth Studio communication uses ZMK's GATT transport.
- The firmware has a configurable total capacity of eight runtime layers.

Open [ZMK Studio](https://zmk.studio/) after connecting the keyboard. If both
USB and Bluetooth are connected, select the same keyboard output transport that
Studio is using.

ZMK allocates its keymap as a fixed-size array at build time, so Studio cannot
grow beyond the firmware-provided capacity. Occupied slots all use the same
mutable runtime representation; empty capacity is not exposed as a separate
kind of layer.

Studio changes are stored on the keyboard. Later changes to the generated
`glove80.keymap` become the new stock configuration, but do not replace saved
Studio settings until **Restore Stock Settings** is used in Studio.

## Experimental host lighting

The firmware now contains the first roadmap implementation: a versioned,
ephemeral RPC for setting individual key LEDs. It works through Studio's USB or
Bluetooth transport, never writes live frames to flash, and restores ordinary
firmware lighting when the host clears the override or its timeout expires.

See [`docs/host-lighting-protocol.md`](./docs/host-lighting-protocol.md) for the
wire contract and current limitations. Static lighting has been exercised on
both halves over USB, including simultaneous blink and breathe effects.

## Manual lighting editor

[`ui/`](./ui/) contains **Glove80 Lightbench**, a standalone per-key lighting
editor. It connects directly through the standard ZMK Studio USB or BLE
transport and does not depend on a daemon or any Codex integration.

```sh
cd ui
npm ci
npm run dev
```

Open the printed localhost URL in Chrome or Edge, connect the keyboard, select a
color, and click or drag across keys. See [`ui/README.md`](./ui/README.md) for
browser support, architecture, and connection details.

For terminal control, use the Rust CLI (no daemon required). It speaks the
RMK host protocol over USB raw HID or BLE — see
[`tools/glove80-control/README.md`](./tools/glove80-control/README.md):

```sh
cargo run --quiet -- lighting caps
cargo run --quiet -- lighting set 0-5,12 ff0066
cargo run --quiet -- lighting clear
cargo run --quiet -- config validate path/to/config.json --layer-capacity 8
cargo run --quiet -- version
```

Run `cargo install --path tools/glove80-control` if you prefer a normal
`glove80-control` executable on your `PATH`.

With RMK firmware installed, either half can be put into its UF2 bootloader
without using a key chord:

```sh
cargo run --quiet -- bootloader --peripheral
cargo run --quiet -- bootloader
```

Request the peripheral bootloader before the central, since the central
provides the split and host-protocol transports used to reach the peripheral.
(The legacy ZMK Studio serial commands were retired after the RMK cutover;
the CLI no longer talks to the ZMK recovery firmware.)

The left Magic/MoErgo key is reserved as a firmware status pixel: cyan means a
host lighting frame is active, green means USB HID is ready, blue means the
active Bluetooth profile is connected, amber means the selected transport is
not ready, and dim white means the firmware is running without a more specific
connection state.

Right-half host lighting exposes LED indices 40 through 79. Static colors use
four-pixel split batches; animated effects use two-effect batches with 50 ms
timing resolution. Both fit a default BLE ATT payload and also work over the
wired split transport. A partial-result response indicates that the peripheral
half was unavailable for at least one batch.

## Build

```sh
nix run .#generate-keymap
nix build .#firmware
```

The build produces half-specific images plus a combined archival artifact:

```sh
result/glove80-left.uf2
result/glove80-right.uf2
result/glove80-left-settings-reset.uf2
result/glove80-right-settings-reset.uf2
result/glove80.uf2
```

Flash `glove80-left.uf2` to the left bootloader and `glove80-right.uf2` to the
right bootloader. Do not use the combined artifact for routine flashing.

The settings-reset images are recovery tools. Flash the matching reset image,
allow it to boot once and erase persistent state, then return that half to its
bootloader and flash the matching normal image. Reset both halves together when
repairing their split bond.

## Updating From MoErgo

To merge a newer MoErgo ZMK revision into the vendored subtree:

```sh
git subtree pull --prefix=zmk https://github.com/moergo-sc/zmk.git main --squash
```

Resolve any conflicts in the locally customized firmware source, then run the
full firmware build before committing the merge. The initial subtree import is
MoErgo ZMK revision `2f73a230e2fc7b2bd64a9736181e87bf54338131`.

To update the keyboard layout itself:

1. Export or fetch the MoErgo layout JSON.
2. Replace `config/moergo-layout.json`.
3. Run `nix run .#generate-keymap`.
4. Commit the regenerated `config/glove80.keymap`.

## Direction

See [`ROADMAP.md`](./ROADMAP.md) for the planned optional host integration,
including live Codex status lighting and keyboard-driven Codex actions.

`scripts/generate-keymap.mjs` is only a build-time converter for the existing
MoErgo JSON export. The manual editor uses TypeScript because it runs in a web
browser, but neither ZMK Studio nor the live host protocol requires JavaScript;
the generator and any future daemon can be replaced independently.
