# Glove80 RMK spike

RMK firmware for the MoErgo Glove80, built as the bounded hardware spike
described in [`docs/rmk-evaluation.md`](../../docs/rmk-evaluation.md). The
known-good ZMK firmware in `zmk/` remains the recovery baseline; nothing here
modifies it.

## Layout and hardware sources

All hardware facts are transcribed from the ZMK board definition in
`zmk/app/boards/arm/glove80/`:

- 6x14 logical grid identical to the ZMK matrix transform (columns 0-6 left
  half, 7-13 right half; columns 6/7 are the thumb clusters; positions
  (0,5), (0,8), (5,5), (5,8) are unpopulated).
- Left half is the split central (USB + BLE), right half the BLE peripheral.
- Flash layout matches the ZMK partition table: app `0x26000`-`0xdc000`,
  reserved runtime-config partition `0xdc000`-`0xec000` (untouched), RMK
  storage `0xec000`-`0xf4000` (the ZMK settings partition), bootloader at
  `0xf4000`. The SoftDevice region `0x0`-`0x26000` is left in place unused.
- Battery uses the internal VDDH/5 ADC channel on both halves.
- UF2 family IDs: left `0x9807B007`, right `0x9808B007`.

Not yet ported (Stage 5 of the spike): WS2812 key LEDs (left data `P0.27`,
enable `P0.31`; right data `P0.13`, enable `P0.19`; GRB, 40 LEDs per half),
the power-button PWM LED (left `P1.15`, right `P0.16`), and the Glove80-ext
connector.

## Building

```sh
./build.sh
```

produces `glove80_lh_rmk.uf2` and `glove80_rh_rmk.uf2`. The toolchain is
pinned in `rust-toolchain.toml`; RMK and nrf-sdc are pinned to exact revisions
in `Cargo.toml`.

Note: the RMK chip defaults for nrf52840 inject a `[dfu]` section, which makes
config resolution print warnings that our `[storage]` addresses are ignored.
They are not: the `dfu_nrf` cargo feature is disabled, so the generated code
uses the explicit `start_addr = 0xec000`. The warnings are cosmetic.

## Spike status

- [x] Stage 1: compile-only board skeleton. Both halves build reproducibly;
      UF2 address ranges verified against the bootloader layout
      (left `0x26000`-`0x9478c`, right `0x26000`-`0x6d134`; initial SP
      `0x2003fc08`, reset vector `0x26101`).
- [x] Stage 2: left-half safety test (2026-07-18). Boots through the stock
      MoErgo bootloader from `0x26000` with no SoftDevice, enumerates as
      `16c0:27db` "MoErgo Glove80" (RMK identifiable by its `vial:` USB
      serial), types correctly.
- [x] Stage 3: wireless split (2026-07-18). Halves pair on the configured
      static addresses; right-side keys type through the central.
- [x] Stage 4: USB/BLE Vial editing and storage (2026-07-18). Live keymap
      editing via vial.rocks over USB WebHID; BLE pairs without a passkey,
      registers HID keyboard + mouse, and reports battery over GATT.
      Known limitation: browser Vial cannot reach the keyboard over BLE
      (WebHID has no GATT transport) — the planned custom host protocol
      must cover that path.
- [ ] Stage 5: minimum viable lighting (in progress).

## Safety rules for flashing

1. Never flash the right half before Stage 2 has proven left-half recovery.
2. Keep the known-good ZMK UF2s for both halves at hand before any flash.
3. Both halves keep their physical bootloader entry (reset-button double-tap /
   Magic+bootloader binding on the ZMK side); verify it before and after.
