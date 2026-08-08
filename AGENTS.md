# Glove80 configuration workflow

## Apply runtime configuration before considering a firmware flash

- Ordinary keymap, layer-binding, default-layer, brightness, and durable
  per-layer lighting-scene changes are runtime configuration. Apply them to
  the connected keyboard through `./bin/glove80-control` and verify them with
  the corresponding read command. Do not flash firmware just to deliver
  those changes.
- Keep `config/glove80.toml`, `config/firmware.toml`, and the live keyboard
  state aligned. `config/glove80.toml` uses Rynk/VIA keycode names, while
  `config/firmware.toml` provides the equivalent compiled defaults for a
  fresh or reset keyboard.
- Use `just diff` before runtime mutations and `just apply` to write and verify
  the source TOML. Use `just show` for a read-only canonical export. `just
  pull` intentionally rewrites `config/glove80.toml` from live persistent
  state and does not preserve comments, so inspect the resulting diff.
- Flash both halves only when firmware code, dependencies, protocol behavior,
  hardware support, or another compiled-only setting changed. A firmware
  update does not reliably replace persisted runtime keymap or lighting state,
  so apply and read back any requested runtime changes separately even after
  a necessary flash.
- Before flashing, identify the connected firmware version and validate the
  correct left/right UF2 artifacts. After flashing, verify the reported
  version and right-half connection.

## Direct Cargo firmware builds must use the repository configuration

- When building without Nix, do not run the nested firmware `cargo`/`xtask`
  command with its default environment. Set `KEYBOARD_TOML_PATH` to the
  absolute path of `config/firmware.toml`, and set
  `GLOVE80_CONFIG_GIT_COMMIT` and `GLOVE80_CONFIG_GIT_DIRTY` from this outer
  repository before invoking `cargo +1.97.0 run -p xtask -- dist` inside
  `dependencies/glove80-rmk`.
- The crate-local `crates/glove80-rmk/keyboard.toml` is not an interchangeable
  fallback for this keyboard. A UF2 built against it can pass host tests and
  size checks yet watchdog-loop during hardware initialization. Treat a
  firmware artifact built without the explicit outer TOML as invalid, even
  when its source commit and RMK pin are otherwise correct.
- Compare the produced UF2 address ranges with the last known-good bundle
  before flashing. A surprising range change is a build-input warning, not
  proof of RAM exhaustion. Qualify the left/central half first and keep a
  known-good recovery UF2 available.
