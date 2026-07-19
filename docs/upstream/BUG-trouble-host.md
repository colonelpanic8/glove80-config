# trouble-host: notify silently succeeds when the peer is not subscribed

## Affected revision

RMK pins trouble-host revision
`c21b1239fb86de712fc99da803def88378d81177` in `rmk/Cargo.toml`.

At that revision, `host/src/attribute.rs`,
`Characteristic::notify_raw`, contains:

```rust
if !server.should_notify(conn, cccd_handle) {
    // No reason to fail?
    return Ok(());
}
```

The method documentation says an unsubscribed peer will not be notified, but
the successful result makes a dropped notification indistinguishable from a
notification queued for transmission. Callers that consume a one-shot value
after `Ok(())` lose it permanently.

## Reproduction narrative

1. A BLE split peripheral accepts a connection and starts its application
   session immediately.
2. Before the central completes service discovery and writes the notification
   characteristic's CCCD, the peripheral announces a one-shot state message.
3. RMK calls `Characteristic::notify` / `notify_raw`.
4. `should_notify` is false because the CCCD write has not happened.
5. trouble-host sends no ATT notification but returns `Ok(())`.
6. RMK treats the write as successful and discards the message.
7. The central subscribes moments later, but the announcement is already gone.

This is timing-dependent and especially visible during split reconnects: the
peripheral task starts at connection establishment, while the central must
discover characteristics and subscribe before notifications are deliverable.

## Proposed API behavior

Add an explicit error, for example `Error::NotSubscribed`, and return it when
the relevant CCCD bit is disabled:

```rust
if !server.should_notify(conn, cccd_handle) {
    return Err(Error::NotSubscribed);
}
```

The same policy should be considered for `indicate_raw`, which has the same
`Ok(())` behavior under `!should_indicate` at this revision.

An explicit result lets callers choose the correct policy: retain/retry a
one-shot notification, wait for readiness, or intentionally drop it. It also
prevents an API success value from asserting delivery eligibility when no PDU
was sent. This does not promise remote receipt; it only distinguishes “not sent
because the local subscription prerequisite is absent” from “accepted for
transmission.”

## Workaround in this fork

`split-app-messages` does not raise the peripheral application's link-up state
at bare BLE connection time. It waits for the first message from the central.
RMK's central subscribes first and then starts `PeripheralManager`, whose first
write is a `ConnectionStatus` snapshot. ATT bearer ordering therefore makes
that first inbound message a practical readiness gate for peripheral-to-central
application notifications.
