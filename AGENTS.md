# MoErgo configuration workflow

## Treat Glove80 and Go60 as peer targets

- Shared firmware behavior belongs in the nested repository's
  `crates/moergo-rmk`; board crates should contain only hardware-specific
  entry points and drivers.
- Runtime and compiled configuration are intentionally separate. Personal
  bindings, layer names, behavior records, Bluetooth names, pointing policy,
  and lighting policy belong only in `config/glove80.toml` and
  `config/go60.toml`.
- The pinned product repository's board configurations are canonical compiled
  defaults. `config/go60-firmware.toml` must match its stock board file
  semantically; `config/firmware.toml` may differ only by marking the twelve
  Glove80 thumb keys bilateral for runtime `opposite_hand_hold` behavior. Run
  `just firmware-config-check` and do not add personal compiled defaults.
- Use `./bin/moergo-control` for new documentation and automation.
  `./bin/glove80-control` is a compatibility shim.
- Run both configuration validations for shared model changes and build both
  firmware bundles for shared embedded changes.

## Apply runtime configuration before considering a firmware flash

- Ordinary keymap, layer-binding, default-layer, brightness, and durable
  per-layer lighting-scene changes are runtime configuration. Apply them to
  the connected keyboard through `./bin/moergo-control` and verify them with
  the corresponding read command. Do not flash firmware just to deliver
  those changes.
- Keep each runtime TOML and the corresponding live keyboard state aligned.
  The firmware TOMLs intentionally retain stock defaults; a fresh or reset
  keyboard needs `just apply` or `just go60-apply <device>` to restore the
  personal runtime configuration.
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
  `MOERGO_CONFIG_GIT_COMMIT` and `MOERGO_CONFIG_GIT_DIRTY` from this outer
  repository before invoking `cargo +1.97.0 run -p xtask -- dist` inside
  `dependencies/moergo-rmk`.
- Keep using the explicit outer Glove80 TOML: it carries the approved bilateral
  thumb metadata and is covered by the source-parity check. Treat an artifact
  built without that configuration and outer-repository provenance as invalid.
- Compare the produced UF2 address ranges with the last known-good bundle
  before flashing. A surprising range change is a build-input warning, not
  proof of RAM exhaustion. Qualify the left/central half first and keep a
  known-good recovery UF2 available.
