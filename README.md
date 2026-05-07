# Glove80 ZMK Config

Custom Glove80 firmware for Ivan's layout, generated from the MoErgo layout
`34957465-9943-4236-852c-d88044706dcb`.

This uses `darknao/zmk` on the `darknao/rgb-dts` branch for the experimental
`zmk,underglow-layer` support. The normal MoErgo Layout Editor key decorations
are editor-only; this repo adds physical RGB maps for selected layers.

## RGB Layer Maps

- All layers: Ctrl, Shift, Alt, and GUI modifier keys are green.
- `Games`: W/A/S/D and the backspace-position space key are blue.
- `Mac_Hyper`: the main right-thumb Hyper key is red.
- `Games_Mac_Hyper`: generated locally as a transparent conditional layer so
  the two independently toggled layer maps appear together when both layers are
  active.

The base modifier lights are included in every generated RGB layer map because
the current ZMK RGB layer effect renders one active layer map at a time.

The RGB layer effect must be selected on the keyboard. If it is not already
active after flashing, use your Magic layer RGB effect key to cycle to the layer
effect. In this fork it appears after swirl.

## Build

```sh
nix run .#generate-keymap
nix build .#firmware
```

The combined firmware is written to:

```sh
result/glove80.uf2
```

Flash that same `.uf2` to both halves.

## Updating From MoErgo

1. Export or fetch the MoErgo layout JSON.
2. Replace `config/moergo-layout.json`.
3. Run `nix run .#generate-keymap`.
4. Commit the regenerated `config/glove80.keymap`.
