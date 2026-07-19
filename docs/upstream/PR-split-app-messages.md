# Split: add a bounded application-message side channel

## Motivation

Some split keyboards need to synchronize small pieces of firmware-owned state
that do not belong in RMK's built-in key, pointing, battery, display, or DFU
messages. Without a generic extension point, each keyboard must either fork the
split loop or encode application state into unrelated protocol messages.

This was developed for a split keyboard port that needs loss-tolerant
side-band state synchronization. RMK does not interpret the payload.

## Design

- Adds `SplitMessage::Application(SplitAppData)` as the final enum variant, so
  existing postcard discriminants remain stable.
- Bounds one payload at 26 bytes. `SplitAppData` serializes only `data[..len]`,
  keeping the maximum split transfer within trouble-host's 32-byte array limit.
- Provides bounded channels for central-to-peripheral transmission,
  peripheral-to-central transmission, and a symmetric receive inbox.
- Inserts application traffic as the lowest-priority outgoing arm. Normal split
  reads and key events retain priority.
- Uses `try_send` on receive paths so a slow application consumer cannot stall
  key processing. Applications must tolerate loss and resynchronize.
- Exposes split link state through a `Watch`. Drop guards lower the state even
  when connection-manager futures are cancelled.
- On the BLE peripheral, link-up is delayed until the first inbound central
  message. The central subscribes before starting `PeripheralManager`, so this
  avoids sending application notifications during the pre-CCCD window.

The initial API assumes one split peripheral. A future multi-peripheral API
would need to associate messages and link state with a peripheral ID.

## Usage

Create a bounded payload and enqueue it without waiting on a full application
queue:

```rust
use rmk::split_app::{SPLIT_APP_TX, SplitAppData};

if let Some(message) = SplitAppData::new(&payload) {
    let _ = SPLIT_APP_TX.try_send(message);
}
```

The peripheral uses `SPLIT_APP_PERIPH_TX` for the reverse direction. Both sides
receive from `SPLIT_APP_RX` and observe `SPLIT_APP_LINK` to trigger an
idempotent resync after a `false -> true` transition.

Producers should use `try_send`, keep messages small, and treat the channel as
loss-tolerant. Critical application protocols should add their own sequence,
acknowledgement, or full-state resync semantics.

## Testing

The branch passed the repository harness on Rust 1.97.0: three `rmk-types`
nextest configurations, twelve `rmk` no-default-feature combinations covering
split/serial/BLE/Vial/storage/async matrix/passkey/steno variants, and two
`rmk-types` doctest commands. See `test-logs/split-app-messages.log` in the
verification workspace.

Upstream branch: `split-app-messages-main` at `565a5d05`, rebased on upstream
`main` at `5feaf8b1`.
