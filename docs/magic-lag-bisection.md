# Magic-layer lag bisection

Symptom: Magic-layer lighting feels laggy/flaky on the **left (central) half** —
state-key presses (esp. `UG_OMODE`) sometimes show no feedback, and the
**release** of Magic is particularly slow to restore base lighting. Lower/Games
one-key indications through the same pipeline feel normal.

Method: strip the Magic scene to a single static cell, confirm it is snappy,
then reintroduce one candidate at a time, testing after each step. All steps
are runtime configuration (`just apply` / `moergo-control`) — no firmware
flashes. Candidates that code analysis "cleared" are still on the list; the
analysis is a prediction to falsify, not a verdict.

## Test protocol (run identically at every step)

Fix the environment per pass; only the scene changes within a pass:

- **Pass 1: USB-powered** (no wake edges, rail always on, no BLE typing link).
- **Pass 2 (if pass 1 stays snappy): battery/BLE** (Glove80).

Per step, ~10 trials each of:
1. **Entry**: tap-and-hold Magic → how fast does the scene appear?
2. **Release**: release Magic → how fast does base lighting return? (the
   headline symptom)
3. **State key**: hold Magic, press `UG_OMODE` deliberately → does the T
   indicator change *every* time, immediately? Note misses AND whether the
   color ever skips a state (a skip = press applied late, not lost).
4. **Rapid re-entry**: quick Magic tap-tap-hold → any hang or stale scene?
5. **Right canary**: does the right-half cell appear/disappear in sync with
   the left one, or trail it? (Passive replication-latency probe at every
   rung. If it *never* lights, the peripheral is rejecting sync entirely —
   e.g. a split-lighting wire-version mismatch between halves — which is a
   finding on its own; verify both halves' firmware versions before
   continuing.)

Record: board, power source, effects on/off, whether any host (rynkbench /
moergo-control) is attached. Keep hosts DISCONNECTED except for the step that
tests them.

## Comprehensive candidate list

Ordered roughly as the reintroduction ladder. "Cleared" = code analysis says
it can't cause this; reintroduce it anyway and let the board vote.

### A. Scene content (runtime scene table — the ladder itself)

| # | Candidate | Mechanism it would implicate | Code prediction |
|---|-----------|------------------------------|-----------------|
| A1 | Baseline: one static cell on a left-half LED + one static cell on a right-half LED (comms canary), layer-2 condition, nothing else | — | snappy; canary flips in sync |
| A2 | + blackout floor (~70 opaque black cells) | per-cell compositing cost; large frame deltas; occlusion of the animated band | cleared (µs-scale, band still ticks) |
| A3 | + static control-block colors (WASD cluster etc.) | rule count growth | cleared |
| A4 | + output-mode indicator (T) and effects indicator (E) | `output_mode` / `effects` conditional evaluation | cleared (live engine state) |
| A5 | + connection/BLE slot rules (per-slot bonded/selected/USB conditions) | BLE state queried during snapshot; connection-condition evaluation | cleared, least-inspected of the conditions |
| A6 | + advertising **blink** (first animated conditional) | animated scene → periodic renders while Magic held; timer-arm pressure | cleared (25 fps loop is mostly idle) |
| A7 | + battery gauges (battery + charge conditions, 5 LEDs/half) | battery condition evaluation; snapshot_changed churn from battery events | cleared (cached Cell read; coalescing signal) |
| A8 | + remainder to full scene (maint-lock R indicator, USER(12), USB key, etc.) | anything missed above | cleared |

### B. Policy axes (toggle independently at any rung)

| # | Candidate | Mechanism | Code prediction |
|---|-----------|-----------|-----------------|
| B1 | `wake_layers = [2]` vs `[]` | wake_active transitions; resume path | entry-side only |
| B2 | `output_mode`: powered-only vs always-on | dark↔lit transitions; brightness-0 frames; chain-power rail off/on with **120 ms settle** | entry-from-dark only; release should be fast |
| B3 | PaletteFX effects on vs off under the scene | with everything static, renders become purely event-driven (no 40 ms self-heal ticks) | cleared (newest event always survives the ring) |
| B4 | Breathing background mode on vs off | animation cadence | cleared |

### C. Loop / event machinery (mostly observed, not toggled)

| # | Candidate | Mechanism | Code prediction |
|---|-----------|-----------|-----------------|
| C1 | Light-action arm is last in the processor select | priority starvation of key actions | cleared at 25 fps |
| C2 | Lossy `LayerChangeEvent` ring (4-deep, 4 subs; LT taps publish too) | missed invalidation → stale scene | cleared (newest survives; fresh snapshot per render, service.rs:436) |
| C3 | Render deadline scheduling | overdue-frame pathology | cleared (now+delay, 1 ms clamp) |

### D. Split/replication side-effects on the central (runs even with central-only lighting)

| # | Candidate | Mechanism | How to test |
|---|-----------|-----------|-------------|
| D1 | Full replica snapshot on every lighting change (≥7 packets, 500 ms ack, 50 ms backoff) | central-side send churn after each state-key press | compare with right half detached / link down vs normal |
| D2 | ContextUpdate on every layer change (one-in-flight gating) | send churn on Magic press AND release | same |
| D3 | EffectHit per keypress over the link | wire churn while typing | effects off vs on |

### E. Host traffic

| # | Candidate | Mechanism | How to test |
|---|-----------|-----------|-------------|
| E1 | rynkbench live view / moergo-control attached | mailbox arm outranks key actions; frame streaming bursts | attach vs detach at a fixed rung |

### F. Input path (physical)

| # | Candidate | Mechanism | How to test |
|---|-----------|-----------|-------------|
| F1 | Same-hand claw chord actuation | marginal presses never register | bind UG_OMODE to an unchorded Base key; also Rynk readback of output mode after a "dead" press |
| F2 | Debounce eating short taps | scan-level loss | deliberate slow presses vs quick flicks |

### G. System-level

| # | Candidate | Mechanism | How to test |
|---|-----------|-----------|-------------|
| G1 | Flash write/erase CPU stalls (nrf-mpsl chunked) | frozen scan/loop during storage activity | force storage churn (repeated host applies) while hammering a key |
| G2 | SPI/DMA/PPI/TWI contention (LED SPI vs split serial TIMER2/PPI vs trackpads) | slow/failed presents, 50 ms retry each | only visible via measurement (Rynk frame streaming timestamps or instrumented build) |

## Ladder order

A1 → (B-axes sanity: confirm snappy with current wake/output settings) → A2 →
A3 → A4 → A5 → A6 → A7 → A8 → B3/B4 flips → E1 → D1/D2 (link down) → F/G only
if everything above stays snappy.

At the first rung that turns laggy: flip that one item off/on twice to confirm
it reproduces, then stop — that's the isolated cause and we go back into the
code with a specific target.

## Bookkeeping

- Save the original scene sections before editing so every rung is restorable:
  `just show` export + git stash/branch of the config TOML.
- `just diff` before each apply; keep per-rung results in this file.

## Results

| Rung | Board | Power | Entry | Release | State key | Canary sync | Notes |
|------|-------|-------|-------|---------|-----------|-------------|-------|
| A1 | Glove80 | USB | snappy | snappy | n/a (no indicators) | ok | effects off, background off; wake_layers/powered-only as configured |
