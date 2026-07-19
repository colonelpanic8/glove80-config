# Vial-over-BLE on RMK + BlueZ: root cause and recommendation

Date: 2026-07-18. Host: NixOS, BlueZ 5.86, kernel 7.1.2. Device: Glove80 running RMK
(pin 1156f82), bonded as ED:2C:1F:40:C2:A2. RMK source clone: `scratchpad/rmk/`.

## TL;DR

- **Root cause: a genuine BlueZ bug** in `profiles/input/hog-lib.c::find_report()`.
  It decides whether output reports are "numbered" from `hog->flags` — the HID
  Information characteristic's flags byte (RemoteWake|NormallyConnectable = 0x03 on
  RMK) — instead of `hog->uhid_flags` (the kernel's UHID_START dev_flags, correctly
  0x0 for RMK's unnumbered Vial report map). `0x03 & UHID_DEV_NUMBERED_OUTPUT_REPORTS
  (0x02)` is true, so BlueZ treats the unnumbered 32-byte Vial output report as
  numbered, uses the packet's first byte as a report ID, finds no report with id
  0x01 (the real report has id 0), and **silently drops the packet** before any GATT
  write. Present in 5.86 and current master; no fixing commit upstream.
- **The RMK side works end-to-end.** When the lookup happens to match (first byte
  0x00), BlueZ writes the packet to handle 0x0038, RMK's `VialService` processes it,
  and the reply arrives as a 32-byte notification on 0x0034. The two-HID-instance
  layout is handled fine by BlueZ (two uhid devices, correct descriptors, CCCDs
  subscribed, reports discovered).
- **Recommendation: don't fix Vial-over-BLE; document USB-only Vial** and proceed
  with the already-planned custom GATT service for the host protocol (Lightbench
  speaks Web Bluetooth GATT directly). Separately, report the one-line bug to BlueZ
  upstream; optionally carry a local NixOS bluez patch if stock-Vial-over-BLE is
  wanted in the interim.

## Evidence chain

### 1. BlueZ code path (source: 5.86, identical in master)

`profiles/input/hog-lib.c`:

- `info_read_cb()` (line ~1103): `hog->flags = value[3]` — the **HID Information**
  flags byte. RMK advertises `[0x01, 0x01, 0x00, 0x03]` (bcdHID 1.01, country 0,
  flags 0x03 = RemoteWake|NormallyConnectable) — hardcoded in
  `rmk/src/ble/ble_server.rs` `VialService::hid_info`, and confirmed over the air
  (btmon read of handle 0x002c → `Flags: 0x03`).
- `start_flags()` (line ~805): `hog->uhid_flags = ev->u.start.dev_flags` — the
  kernel-computed numbered-report flags. Verified 0x0 for the Vial instance.
- `find_report()` (line ~695): sets `cmp.numbered` from **`hog->flags`** &
  `UHID_DEV_NUMBERED_{FEATURE,OUTPUT,INPUT}_REPORTS` (0x01/0x02/0x04). This is the
  bug: it should use `hog->uhid_flags`, as `set_numbered()` (line ~787) correctly
  does. HID-Info flag bits (RemoteWake=0x01, NormallyConnectable=0x02) have nothing
  to do with report numbering.
- `forward_report()` (line ~746): looks up
  `find_report_by_rtype(hog, rtype, ev->u.output.data[0])`; on NULL it returns
  **silently** — no log, no ATT traffic.

Consequence for RMK's Vial instance (unnumbered, Report Reference [id=0, type=2]):
`cmp.numbered=true, cmp.id=<first data byte>`. Any VIA command (first byte 0x01,
0x02, …) → id mismatch → drop. First byte 0x00 → id 0 matches → forwarded.

### 2. Live confirmation (gdb + btmon)

gdb breakpoints on `forward_report`/`write_char` in the running bluetoothd
(bluez 5.86, PID 887524), writing through the Vial hidraw device (char 244:8):

```
== forward_report hit ==          # packet 01 00 00 ...
ev.type=6 data0=0x01 rtype=1 size=32
hog=... uhid_flags=0x0 hidinfo_flags=0x03
report ...: numbered=0 id=0 type=1 value_handle=0x0034 props=0x12   # input
report ...: numbered=0 id=0 type=2 value_handle=0x0038 props=0x0e   # output
                                   -> NO write_char, NO ATT (btmon silent)

== forward_report hit ==          # packet 00 00 00 ...
ev.type=6 data0=0x00 ...          -> write_char hit
```

btmon for the 0x00 packet (`scratchpad/btmon-real.txt`):

```
ATT: Write Request (0x12) len 34   Handle: 0x0038   Data[32]: 00 00 ...
ATT: Write Response (0x13)
ATT: Handle Value Notification (0x1b) len 34  Handle: 0x0034  Data[32]: ff 00 ...
```

The `ff` reply is RMK's unknown-command response — firmware Vial processing over
BLE demonstrably works. The only failure is BlueZ's report-ID lookup.

### 3. Ruled out

- **CCCD not enabled**: reconnect capture (`reconnect-btmon.txt`) shows BlueZ
  writing CCCD=notify on 0x0035 (Vial input) at connect. Not the issue.
- **uhid layer**: the kernel delivers UHID_OUTPUT (type 6) events to bluetoothd
  correctly (strace: `read(fd41, "\6\0\0\0\1...")`), and the Vial uhid device gets
  the right descriptor (usage page 0xFF60/0x61, unnumbered 32-byte in/out).
- **Multi-instance discovery**: both HID service instances are discovered, get their
  own `bt_hog` + `bt_uhid` + hidraw, and output routing for the *primary* (numbered)
  instance works: writing `[0x01, 0x00]` (LED report, id 1) to the composite hidraw
  produced an ATT write to handle 0x001c with the id stripped. (For numbered maps the
  buggy flags test coincidentally yields the correct answer, which is why virtually
  every normal BLE keyboard works and this bug survives: it only bites HOG devices
  with *unnumbered* reports and a nonzero HID-Info flags byte.)
- **NotAuthorized on D-Bus GATT writes**: expected BlueZ policy (HOG-claimed
  service), not evidence of a firmware problem.

### Investigation hazard note (affects earlier findings)

Partway through, `/dev/hidraw8` on this machine became a **regular file** (an
artifact of earlier debugging; likely a root shell redirection while the node was
absent during a reconnect). Writes to it "succeeded" while reaching no kernel
device, which produced several misleading null results (no uhid events, dead-looking
watches). The decisive tests above were re-run through a fresh device node
(`mknod c 244 8`). **Cleanup needed**: `sudo rm /dev/hidraw8` then
`sudo udevadm trigger -s hidraw -c add` (or just cycle the BLE connection) so udev
recreates the real node. Until then, anything opening `/dev/hidraw8` (including
Vial) talks to a plain file.

## Who owns the bug

**BlueZ.** One-line class of fix: `find_report()` must consult `hog->uhid_flags`
(and probably fall back to per-report `report->numbered`, already maintained by
`set_numbered()`), not the HID Information flags in `hog->flags`. RMK's GATT layout
is spec-conformant; the kernel uhid layer is correct.

## Fix options

### (a) RMK: fold Vial into the primary HID service under a distinct report ID

Mechanics: extend `BleCompositeReport` (`rmk/src/hid.rs`) with the 0xFF60/0x61
collection under `report_id = 0x05`, add input/output Report characteristics with
Report Reference `[5,1]`/`[5,2]` to `HidService` (`rmk/src/ble/ble_server.rs`),
route in `gatt_events_task`/`host/ble.rs`, bump the pinned 178-byte map length and
the `ble_report_map_matches_service_definition` test.

Verdict: **feasible but breaks the point of doing it.** The VIA/Vial desktop
protocol assumes *unnumbered* 32-byte reports (hidapi `write(b"\x00" + msg)`), so a
numbered BLE Vial report would be unreachable by the stock Vial app — the client
would have to prepend id 0x05, i.e. a patched Vial. It also dodges rather than fixes
the BlueZ bug, and mixing an unnumbered pair into the otherwise-numbered map is not
possible (report id 0 cannot coexist with numbered reports in one device). Medium
effort, high compatibility risk, kills Android-multi-instance concerns but breaks
Linux/mac/Windows stock Vial over BLE anyway. Not recommended.

### (b) BlueZ fix

A patch changing `find_report()` to use `uhid_flags` is small, obviously correct,
and testable on this machine (NixOS `bluez.overrideAttrs` patch). Upstreamable:
report to BlueZ (linux-bluetooth@vger.kernel.org or github.com/bluez/bluez issues)
with the analysis above; affected: any HOG device with unnumbered reports and
HID-Info flags != 0 — includes every VIA/Vial-style vendor collection exposed as a
separate unnumbered HID service instance.

Caveat even after the fix: stock Vial writes 33 bytes (0x00 prefix + 32). usbhid
strips the leading zero for unnumbered devices; uhid/BlueZ do **not**, so the GATT
write could arrive as 33 bytes and TrouBLE may reject it against the `[u8; 32]`
characteristic (untested — D-Bus writes are blocked while HOG owns the service).
So "BlueZ fixed" does not guarantee stock Vial works over BLE without further
verification; budget a follow-up test with the patched bluez.

Effort: patch itself trivial; upstream latency months; local overlay immediate.
Risk: low.

### (c) Sidestep: custom GATT service + USB-only Vial (recommended)

The project already plans a custom host transport (docs/rmk-evaluation.md "a
versioned command set carried by raw HID/Vial" and the custom-host-interface
direction; docs/desired-system.md "Host interfaces"), and Lightbench will speak Web
Bluetooth GATT directly — Web Bluetooth cannot see HID-over-GATT services at all
(blocklisted/claimed by the OS HID stack), so the custom service is required for
Lightbench regardless of whether Vial-over-BLE works. Vial then matters only as the
interim manual editor, and it works today over USB unaffected.

## Recommendation

1. **Adopt (c)**: document Vial as USB-only for BLE-connected operation; keep RMK's
   current two-instance layout (harmless on Linux; the Android single-instance
   caveat only affects the Vial instance, which Android could never use anyway).
   Build the host protocol on the planned custom GATT service.
2. **Report (b) upstream** to BlueZ — it is a real, unfixed, easily-stated bug with
   a clean reproduction: "HOG: output reports silently dropped for devices with
   unnumbered reports when HID Information flags byte is nonzero;
   `find_report()` tests `hog->flags` (HID Info) against `UHID_DEV_NUMBERED_*`
   instead of `hog->uhid_flags`." Attach the gdb/btmon differential (first byte
   0x00 forwarded, 0x01 dropped). No urgency for this project once (c) is adopted.
3. Do **not** pursue (a).

## Machine cleanup / state notes

- `/dev/hidraw8` is currently a stale regular file (see hazard note) — remove it and
  retrigger udev or cycle the connection.
- The keyboard remains bonded and connected; three disconnect/reconnect cycles and
  one BLE reconnect were performed during testing; no bond changes, no repo changes.
- Artifacts in this scratchpad: `btmon-real.txt`, `gdb-real.txt` (decisive test),
  `reconnect-btmon.txt` (CCCD evidence), `hog-lib-5.86.c` / `hog-lib-master.c`
  (source), `fr3.gdb` (probe script), `strace-out.txt` (uhid delivery proof from
  the earlier session).
