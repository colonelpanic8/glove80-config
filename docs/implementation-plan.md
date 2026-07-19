# Implementation plan

How we get from the working RMK spike to the full system in
[`design-goals.md`](./design-goals.md) and
[`lighting-design.md`](./lighting-design.md). Phases are ordered by
dependency; each has a crisp exit. Keep the ZMK images flashable until the
final phase.

## Already done (spike, 2026-07-18)

- RMK port of both halves: boots via stock bootloader, USB + BLE typing,
  wireless split, Vial editing (USB), battery. `rmk/glove80/`
- Minimum lighting engine: WS2812 driver with hard 80% clamp, power-button
  PWM, event-driven frame task, layer color on one key.
- Vial-over-BLE ruled out (BlueZ bug, documented) — wireless config goes
  through our own protocol.

## Working decisions

- **Cross-half animation phase**: per-half for v1. Revisit shared-epoch sync
  only if visible drift annoys in practice.
- **Layer lighting scope**: the sparse model supports both accents and
  full scenes; no schema decision needed. Lightbench starts with per-key
  editing either way.
- **RMK vendoring**: stay on the pinned git dependency until we must patch
  RMK internals (expected at split lighting transfer). At that point vendor
  as a git subtree and carry patches there.
- **Old ZMK-era host code** (`protocol/proto`, `host-lighting/`, the
  protobuf/Studio parts of `ui/`): kept for reference, replaced by the new
  protocol; deleted at cutover.

## Phase 1 — Compositor core (firmware)

The heart of the lighting contract, built as pure logic first.

- `rmk/glove80/src/compositor/`: cell type (transparent | color+effect),
  record + activation predicate (always / layer / toggle / host / status),
  priority-ordered composition into a `Frame`.
- Effects: static, blink (period/phase/duty), breathe (period/phase).
  Ticker exists only while animated cells are visible; recompute animated
  cells only.
- Global runtime brightness scalar (driver clamp stays the ceiling).
- Host-overlay slot with per-cell optional TTL (firmware timer).
- Replaces the stage-5 frame source; layer-accent default config so
  behavior is visibly richer than one key.
- Pure-logic parts unit-tested on the host (std test crate).
- Exit: both halves render base + layer + a hardcoded host-overlay test
  cell with blink/breathe on real hardware; typing unaffected.

## Phase 2 — Host protocol + codec

One protocol, three transports, one codec.

- Versioned command set (capability query first): set/unset cells, clear,
  read-back, atomic replace, TTL, brightness, toggles, bootloader entry.
- Framing: 32-byte-report friendly (USB raw HID) and GATT characteristic
  friendly (custom service; Web Bluetooth reachable). Same payload codec.
- Rust codec crate shared by firmware and CLI (`no_std` + `std`);
  TypeScript codec for Lightbench with golden test vectors shared across
  both.
- Firmware: custom GATT service (new UUID, not HID) + USB vendor interface,
  feeding the compositor's host overlay.
- Exit: CLI sets/clears/replaces overlay cells over USB and BLE;
  Lightbench does the same from the browser (WebHID + Web Bluetooth).

## Phase 3 — Split lighting transfer

- Vendor RMK as a subtree; add a bounded application-message hook to the
  split protocol (aim upstreamable).
- Forward host-overlay batches and toggle/brightness state to the
  peripheral; peripheral runs the same compositor locally.
- Exit: a host write lights the correct key on the right half over the
  split link; key latency unchanged under lighting load.

## Phase 4 — Persistent lighting + canonical config

- Persist base/layer/toggle lighting records in RMK storage; boot loads
  them into the compositor.
- Extend the canonical schema (`tools/glove80-control`, building on
  `runtime_manifest.rs`) to cover lighting records, activation predicates,
  toggles, and stable layer references.
- Transactional apply: complete-config import lands atomically or not at
  all; export round-trips.
- Exit: reboot restores configured lighting; interrupted apply leaves the
  previous config; export → import → export is byte-stable.

## Phase 5 — Tooling completion

- Lightbench: persistent lighting-layer editing on the new protocol (USB +
  BLE), TTL/brightness controls, capability-driven UI.
- Nice-to-have: additive FRAME_READ command exposing the final composed
  frame, enabling a true live-preview panel in Lightbench.
- CLI: full verb set (validate/apply/export/restore/watch), shared codec.
- Optional background service for app-state lighting (Codex states) as a
  thin overlay client.
- Exit: every lighting-design.md host operation is exercisable from both
  Lightbench and the CLI.

## Phase 6 — Qualification and cutover

- Run the full checklist in design-goals.md on both halves.
- Fix stragglers (battery-idle behavior, bootloader entry from host, etc.).
- Cut daily use over to RMK; keep ZMK recovery images archived; retire the
  ZMK-era host code.
- Exit: checklist all green; ZMK tree no longer needed for daily use.
