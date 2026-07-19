# Keymap: route external operations through the Vial service

## Motivation

A firmware-defined configuration interface may need to read or update the live
keymap. Calling the keymap context directly from another task would duplicate
Via/Vial conversion rules, risk racing the Vial task, and potentially bypass
the persistence behavior used by dynamic-keymap commands.

This was developed for a split keyboard port with a second configuration
transport.

## Design

- Adds a bounded `KeymapOp` channel under the `vial` feature.
- Supports `Get` and `Set` at `(layer, row, col)` using VIA/Vial's 16-bit
  keycode encoding.
- Makes `VialService::run` select between ordinary host packets and external
  operations, preserving one owner for live-keymap mutation.
- Reuses `to_via_keycode`, `from_via_keycode`, `get_action`, and the async
  `set_action` persistence path.
- Returns the canonical value read back after each operation. For `Set`, this
  makes lossy keycode conversions visible to the caller.
- Uses capacity-one request and result channels; exactly one external client
  and one in-flight operation are supported.

The channel is an internal coordination API, not a new wire protocol. The
external transport remains responsible for request validation, authorization,
and response encoding.

## Usage

With `vial` enabled, a single client sends an operation and awaits its matching
result:

```rust
use rmk::keymap_ops::{KEYMAP_OP_RESULTS, KEYMAP_OPS, KeymapOp};

KEYMAP_OPS
    .send(KeymapOp::Set { layer, row, col, keycode })
    .await;
let canonical_keycode = KEYMAP_OP_RESULTS.receive().await;
```

Do not issue a second operation until the first result has been consumed. A
binary without a running Vial service must not use the channels.

## Testing

The branch passed the repository harness on Rust 1.97.0: three `rmk-types`
nextest configurations, twelve `rmk` no-default-feature combinations covering
split/serial/BLE/Vial/storage/async matrix/passkey/steno variants, and two
`rmk-types` doctest commands. See `test-logs/keymap-ops.log` in the verification
workspace.

Upstream branch: `keymap-ops-main` at `cb0e03a5`, rebased on upstream `main` at
`5feaf8b1`.
