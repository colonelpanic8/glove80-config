# Rynk migration

## Decision

Adopt Rynk for keymap configuration, but do not pretend it is stable upstream
yet. [HaoboGu/rmk PR #962](https://github.com/HaoboGu/rmk/pull/962) was open,
non-draft, and mergeable when verified on 2026-07-19. The evaluated integration
is published as `colonelpanic8/rmk` branch `glove80-rynk` at `67f444b2`; the
pre-Rynk rollback is branch `glove80` at `8089822e`.

Rynk is worth adopting because it supplies the missing native configuration
protocol rather than another Glove80-only keymap bridge: typed actions,
capability discovery, bulk keymap operations, persistence, native host
libraries, serial and BLE transports, browser-compatible HID, and WASM.

## Runtime ownership

| Capability | Owner | Transport |
| --- | --- | --- |
| Keymap read/write and persistence | Rynk | CLI: USB CDC serial or native BLE GATT; Lightbench: Web Serial or BLE WebHID |
| Lighting overlay and toggles | Glove80 protocol | USB vendor raw HID or custom encrypted BLE GATT |
| Transactional lighting config | Glove80 protocol | USB vendor raw HID or custom encrypted BLE GATT |
| Build identity and bootloader | Glove80 protocol | USB vendor raw HID or custom encrypted BLE GATT |
| Split lighting/version/remote boot | Downstream `split_app` | RMK split link |
| Shared application/RMK storage | Downstream `shared_flash` | nRF internal flash service |

The old host-protocol v1.2 keymap codec remains as frozen compatibility and
test material, but production firmware does not advertise feature bit 7 or
dispatch its keymap commands. The downstream RMK `keymap_ops` module has been
removed from `glove80-rynk`.

## Compatibility boundary

Existing canonical configuration files and editor UX use QMK/VIA-style u16
keycodes. The CLI (`rynk_keycode.rs`) and browser (`rynk-keycode.ts`) convert
those values to Rynk's typed `KeyAction` at the edge, then perform canonical
readback. Unsupported or non-representable actions are surfaced as lossy; they
are never silently reported as successful round trips. Native Rynk actions
should eventually replace u16 values in a versioned config schema.

## Browser packaging

Lightbench commits a release-mode `wasm-pack --target web` build under
`ui/src/vendor/rynk-wasm` so an npm-only build does not require Rust. Regenerate
it from the pinned submodule with Rust 1.97.0:

```sh
cd dependencies/rmk
RUSTUP_TOOLCHAIN=1.97.0 wasm-pack build --release --target web \
  --out-dir ../../ui/src/vendor/rynk-wasm rynk/rynk-wasm
```

`wasm-pack` writes a catch-all `.gitignore`; replace it with the repository's
intentional-commit comment before committing regenerated output.

## Remaining qualification and risk

- Flash both halves (right/peripheral first), then verify Rynk capability,
  layer read, write, persistence, and readback over all four host paths.
- Verify product-protocol lighting and Rynk can coexist over USB and BLE,
  including reconnects and right-half outages.
- Rynk currently uses `insecure = true` in `keyboard.toml`. Select and test a
  deliberate unlock policy before final release.
- Lightbench currently opens separate lighting and keymap sessions. A single
  connection requires an upstream Rynk application-command/topic extension,
  not another ad hoc transport.
- Track #962 rebases carefully. Keep `67f444b2` pinned and reproducible until a
  reviewed upstream commit replaces it; keep `8089822e` available for rollback.

## Upstream posture

Do not upstream the retired keymap bridge or the overlapping transport hook as
they stand. Report the Glove80 hardware matrix on #962 and propose only gaps
demonstrated by that qualification. Independently upstream the split-DFU race
fix and shared-flash feature; coordinate split messaging with upstream's
forward-split-message work. See `upstream/RMK-UPSTREAMING-PROPOSAL.md`.
