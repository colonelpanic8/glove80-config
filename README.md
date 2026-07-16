# Glove80 ZMK Studio Config

Custom Glove80 firmware for Ivan's layout, generated from the MoErgo layout
`34957465-9943-4236-852c-d88044706dcb`.

This uses the current MoErgo Glove80 ZMK distribution with ZMK Studio enabled.
The keyboard remains fully functional with the generated keymap when Studio is
not connected, while Studio can edit and persist bindings at runtime over USB
or Bluetooth.

The repository carries a small Nix packaging patch that adds Studio's `nanopb`
and protocol-message dependencies—and makes nanopb's host-side generator usable
inside Nix—to MoErgo's build. It can be removed once upstream supports this.

## ZMK Studio

- Hold the `Magic` key and press the far-left key in the bottom row to unlock
  Studio configuration.
- USB Studio communication uses the CDC/ACM serial transport on the left half.
- Bluetooth Studio communication uses ZMK's GATT transport.
- Four empty layers are reserved so Studio can add layers without reflashing.

Open [ZMK Studio](https://zmk.studio/) after connecting and unlocking the
keyboard. If both USB and Bluetooth are connected, select the same keyboard
output transport that Studio is using.

Studio changes are stored on the keyboard. Later changes to the generated
`glove80.keymap` become the new stock configuration, but do not replace saved
Studio settings until **Restore Stock Settings** is used in Studio.

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

## Direction

See [`ROADMAP.md`](./ROADMAP.md) for the planned optional host integration,
including live Codex status lighting and keyboard-driven Codex actions.
