# BLE split peripheral drops its proactive DFU firmware-hash announcement

## Reproducer

Tested on a MoErgo Glove80 nRF52840 split keyboard with RMK commit
`1156f82b` and Rust 1.97.0. Both halves were built and flashed with BLE split
DFU enabled:

```toml
rmk = { features = ["nrf52840_ble", "split", "dfu_nrf", "dfu_split"] }
```

The keyboard configuration used BLE for the split connection and configured
the normal central/peripheral BLE addresses.

1. Flash both halves with that configuration.
2. Boot both halves and allow the split BLE link to connect or reconnect.
3. Capture the central and peripheral trace from connection startup.
4. Observe that the proactive firmware-hash announcement does not arrive at
   the central in the tested session.

The trace shape is: the peripheral attempts its startup
`FirmwareHashResponse` before the central subscribes to the notify
characteristic; the central then completes the CCCD subscription and sends its
initial `ConnectionStatus`, but no proactive `FirmwareHashResponse` appears in
the central's receive loop.

## Root cause

`SplitPeripheral::run` writes `FirmwareHashResponse` immediately at session
startup. On the tested nRF52840 BLE connection, that write occurs before the
central has subscribed to the peripheral's notify characteristic by writing
its CCCD.

RMK pins TrouBLE at `c21b1239`. In that revision,
[`Characteristic::notify_raw`](https://github.com/embassy-rs/trouble/blob/c21b1239fb86de712fc99da803def88378d81177/host/src/attribute.rs#L1019-L1035)
returns `Ok(())` when `should_notify` is false, without transmitting a
notification. RMK therefore observes a successful write even though this
pre-subscription announcement was dropped in the captured session.

## Proposed fix

For BLE only, defer the one-shot proactive hash response until the first
successfully decoded message arrives from the central. The central sends its
initial connection status after subscribing, so that message provides the
readiness barrier. Preserve the immediate announcement on serial split links,
and mark the BLE announcement complete only after a successful driver write so
transient errors remain retryable.

A minimal fix with fake-driver regression coverage is ready. I will link the
pull request here once it is opened.
