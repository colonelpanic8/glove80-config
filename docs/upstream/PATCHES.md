# Vendored RMK patch inventory

This inventory was generated from the current monorepo checkout at
`/home/imalison/Projects/glove80-zmk-config`, commit `2bac75ac`, with:

```sh
rg -n "GLOVE80 PATCH" rmk/vendor/rmk
```

The snapshot `b1ae2b2c` has 44 marker occurrences in 14 files. Current HEAD has
49 occurrences in 16 files because round-2 commit `7a56c997` added five nRF VBUS
hook markers. Every current occurrence is accounted for below. The fork branches
contain the original 44 sites' corresponding code but deliberately remove or
generalize the Glove80 marker comments; the five round-2 sites are not ported.

## `split-app-messages`

Bounded opaque application messages in both directions, loss-tolerant queues,
and split-link state. The peripheral raises link-up only after the first inbound
central message so BLE notifications cannot be emitted before the central's CCCD
subscription.

- `rmk/src/lib.rs:100`
- `rmk/src/split/driver.rs:114,141,173,193,223`
- `rmk/src/split/mod.rs:96`
- `rmk/src/split/peripheral.rs:81,145,156,277,303`
- `rmk/src/split_app_pipe.rs:1,129`

Marker count: 14. Fork rename: `split_app_pipe.rs` -> `split_app.rs`.

## `host-transport-hooks`

Opaque application-defined host transport over a dedicated USB raw-HID
interface and custom BLE GATT service.

- `rmk/src/lib.rs:84`
- `rmk/src/ble/ble_server.rs:23,28,49`
- `rmk/src/ble/mod.rs:275,347,380,737`
- `rmk/src/hid.rs:61,83`
- `rmk/src/host_proto_pipe.rs:1,63`
- `rmk/src/usb/mod.rs:23,226,271,306,393,428,456`

Marker count: 19. Fork rename: `host_proto_pipe.rs` ->
`vendor_transport.rs`, with the related symbol generalizations listed in
`RENAME-MAP.md`.

## `shared-flash`

Mutex-sharing of the singleton radio-safe nRF flash driver, a bounded
application request service, an address-window safety check, and a
`SharedFlash` adapter for RMK storage.

- `rmk/src/lib.rs:65`
- `rmk/src/config_flash.rs:1,261`
- `rmk-macro/src/codegen/chip/flash.rs:79`

Marker count: 4. Fork rename: `config_flash.rs` -> `shared_flash.rs`.

## `keymap-ops`

A one-operation-at-a-time channel into the Vial task so external keymap reads
and writes use the same conversion, ownership, and persistence path as Vial.

- `rmk/src/lib.rs:91`
- `rmk/src/host/via/mod.rs:243,264,270`
- `rmk/src/keymap_ops_pipe.rs:1,39`

Marker count: 6. Fork rename: `keymap_ops_pipe.rs` -> `keymap_ops.rs`.

## Keep local on `glove80`

- `rmk/src/lib.rs:70`: removes `#[cfg(feature = "dfu_split")]` from
  `pub mod crc32`. The Glove80 runtime-configuration slot headers reuse RMK's
  CRC-32 implementation even when split DFU is disabled. This is an
  integration-specific convenience, not a general RMK API proposal.

Marker count: 1. It is represented only by fork commit `165e4720` on the
`glove80` integration branch. All four feature branches and all four `-main`
PR branches retain the upstream `dfu_split` gate.

## Round 2: nRF VBUS state hook — not ported

Monorepo commit `7a56c997` (`firmware: wire conditional lighting state`) wraps
Embassy's nRF hardware VBUS detector so applications can observe VBUS edges via
`USB_VBUS_DETECTED`. It also updates both generated nRF USB-driver construction
paths to use `ReportingVbusDetect`.

- `rmk/src/usb/mod.rs:4,36,90`
- `rmk-macro/src/codegen/chip/comm.rs:68`
- `rmk-macro/src/codegen/split/peripheral.rs:357`

Marker count: 5. Fork branch: **none**. Per the verification scope, this is
enumerated as future round-2 work and was not ported, modified, or tested in the
fork.

## Totals and branch policy

| Disposition | Markers | Fork branch |
| --- | ---: | --- |
| Split application messages | 14 | `split-app-messages` |
| Host transport hooks | 19 | `host-transport-hooks` |
| Shared flash | 4 | `shared-flash` |
| Keymap operations | 6 | `keymap-ops` |
| CRC-32 ungating, keep local | 1 | `glove80` only |
| nRF VBUS state hook, round 2 | 5 | none; future work |
| **Current total** | **49** | |

Do not propose the `glove80` integration branch upstream. The upstreamable
changes are the four feature branches, using their `-main` rebased copies.
