# Transport: add opaque vendor protocol pipes over USB and BLE

## Motivation

Keyboard firmware may need a private configuration or telemetry protocol in
addition to Via/Vial. RMK currently owns the USB and BLE transport tasks, so an
application cannot add a host channel without patching those loops.

This was developed for a split keyboard port with a firmware-defined host
protocol. The RMK layer intentionally has no knowledge of framing or command
semantics; it only moves opaque chunks between transports and application
queues.

## Design

### USB

- Adds a separate 32-byte raw-HID interface under the existing `host` feature.
- Uses an example vendor usage page/usage (`0xFF88` / `0x01`) distinct from
  Via/Vial's raw-HID interface.
- Adds bounded RX/TX queues and a session pump alongside `run_usb_host`.
- Clears stale TX replies after each interface (re)configuration.
- Expands the USB configuration descriptor buffer to 256 bytes when `host` is
  enabled, because the additional HID interface exceeds the former 128-byte
  allocation.

### BLE

- Adds a custom non-HID GATT service so browser clients are not blocked by
  Web Bluetooth's HID restrictions.
- Accepts encrypted write-without-response request chunks up to 257 bytes and
  emits variable-length response notifications.
- Records the negotiated ATT payload (`MTU - 3`, minimum 20) when a request is
  received so the application protocol can size response chunks.
- Uses bounded RX/TX queues and clears replies left by a dead connection.
- Treats the included `fc55...` UUIDs as example defaults that a consumer may
  replace.

The application remains responsible for framing across USB reports or BLE
chunks, request/response correlation, authorization beyond the encrypted-peer
check, retries, and readiness. RMK remains protocol-agnostic.

## Usage

With `host` enabled, a firmware protocol task can consume
`vendor_transport::VENDOR_USB_RX` or `VENDOR_BLE_RX` and enqueue replies on the
matching TX channel. USB reports are exactly `USB_REPORT_LEN` bytes. BLE
consumers use `BleChunk::{len,data}` and should bound output by
`VENDOR_BLE_ATT_PAYLOAD` and `BLE_MAX_CHUNK_LEN`.

Applications should not enqueue a one-shot BLE response until the client has
subscribed to the response characteristic. The pinned trouble-host revision
silently drops notifications for an unsubscribed peer; that separate upstream
issue is described in `BUG-trouble-host.md`.

## Testing

The branch passed the repository harness on Rust 1.97.0: three `rmk-types`
nextest configurations, twelve `rmk` no-default-feature combinations covering
split/serial/BLE/Vial/storage/async matrix/passkey/steno variants, and two
`rmk-types` doctest commands. See `test-logs/host-transport-hooks.log` in the
verification workspace.

Upstream branch: `host-transport-hooks-main` at `a85d4265`, rebased on upstream
`main` at `5feaf8b1`.
