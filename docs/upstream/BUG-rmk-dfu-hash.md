# RMK split DFU hash announcement can be dropped before BLE subscription

## Affected code

In `rmk/src/split/peripheral.rs`, `SplitPeripheral::run` proactively writes a
`SplitMessage::FirmwareHashResponse` at session start when `dfu_split` is
enabled:

```rust
let hash = crate::dfu::read_embedded_firmware_hash();
self.split_driver
    .write(&SplitMessage::FirmwareHashResponse(hash))
    .await
    .ok();
```

The intent is useful: announce the active firmware hash even if the central
booted first and already stopped waiting for a query response. On BLE,
however, `SplitPeripheral::run` begins as soon as the peripheral accepts the
connection. The central has not necessarily discovered the split service and
written the notification CCCD yet.

At RMK's pinned trouble-host revision `c21b1239`,
`Characteristic::notify_raw` returns `Ok(())` for an unsubscribed peer while
sending nothing (see `BUG-trouble-host.md`). `BleSplitPeripheralDriver::write`
therefore also returns success, and the proactive hash is silently lost.

## Reproduction narrative

1. Enable `dfu_split` on a BLE split keyboard.
2. Reconnect the peripheral and central under timing where the peripheral task
   starts before central-side service discovery and CCCD subscription finish.
3. The peripheral immediately writes `FirmwareHashResponse`.
4. trouble-host observes notifications disabled, emits no notification, and
   returns success.
5. The central subscribes afterward and never sees the proactive response.

The ordinary `FirmwareHashQuery` response path can still work after the link is
ready, so the symptom may be an intermittent missed proactive detection rather
than complete DFU failure.

## Proposed fix

Gate the proactive announcement on the first inbound split message, using the
same readiness rule introduced by `split-app-messages`:

- Do not send `FirmwareHashResponse` at the top of `SplitPeripheral::run`.
- On receipt of the first central-to-peripheral `SplitMessage`, first send the
  proactive hash once for that session, then process the inbound message.
- Keep the existing direct response to `FirmwareHashQuery`.

The central subscribes before starting `PeripheralManager`, and that manager
immediately writes its `ConnectionStatus` snapshot. Thus first inbound traffic
means the BLE notification CCCD has already been processed. The same gate is
safe for serial split, where subscription does not exist and the first inbound
message simply establishes that the session is bidirectionally live.

An alternative is to make the transport expose subscription readiness or
return `NotSubscribed` and retry. First-inbound-message gating is already
implemented and explained in `split-app-messages`, requires no new transport
API, and directly closes the observed startup window.
