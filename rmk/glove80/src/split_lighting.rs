//! Split lighting transfer (Phase 3 of docs/implementation-plan.md): the
//! firmware glue between the pure-logic sync layer
//! (`glove80_compositor::sync`) and the vendored split application pipe
//! (`rmk::split_app_pipe`, a GLOVE80 PATCH).
//!
//! Roles (both compiled into `lighting.rs`'s single loop; the binary picks
//! its role in `central.rs` / `peripheral.rs`):
//!
//! - **Central** ([`CentralSplit`]): owns the authoritative store for the
//!   right half's host-overlay cells ([`RemoteOverlay`], protocol keys
//!   `40..80` remapped to local `0..40`), including all TTL bookkeeping.
//!   Mutations are mirrored to the peripheral as bounded [`SyncMessage`]
//!   deltas via `try_send` — NEVER a blocking send, so lighting can never
//!   stall key traffic. If the queue overflows, or on every link-up edge,
//!   the central schedules a full **resync** (clear + every live cell +
//!   shared state), which is idempotent and therefore safe to repeat.
//! - **Peripheral** ([`PeripheralSplit`]): applies received messages to its
//!   own compositor's host overlay (the lighting task stays the compositor's
//!   single owner — messages are handed to it, never applied elsewhere).
//!   Link-loss policy: when the central link drops, the peripheral clears
//!   its host overlay after [`LINK_LOSS_GRACE_MS`] — the TTL/authority for
//!   those cells is gone, so they must not outlive it; the grace period
//!   avoids visible flicker across transient reconnects (which end in a
//!   resync anyway). Brightness/ceiling/toggles are kept across link loss,
//!   like the synced layer state.

use glove80_compositor::sync::{MAX_SYNC_PAYLOAD, RemoteOverlay, SyncCells, SyncKeys, SyncMessage};
use glove80_compositor::{Cell, Compositor};
use rmk::split_app_pipe::{SPLIT_APP_MSG_MAX, SPLIT_APP_TX, SplitAppData};

use crate::lighting::NUM_LEDS;

// The sync codec's payload bound and the vendored pipe's buffer size must
// agree; both are deliberately small (they size every split transfer).
const _: () = assert!(MAX_SYNC_PAYLOAD == SPLIT_APP_MSG_MAX);

/// How long the peripheral keeps host-overlay cells lit after losing the
/// central. Long enough to ride out a routine reconnect without flicker,
/// short enough that authority-less indicators cannot linger.
pub const LINK_LOSS_GRACE_MS: u64 = 5_000;

/// Retry cadence for a resync that could not be queued in one go.
const RESYNC_RETRY_MS: u64 = 50;

/// Encode and queue one message; `false` if the (bounded) queue is full.
fn try_queue(msg: &SyncMessage) -> bool {
    let mut buf = [0u8; MAX_SYNC_PAYLOAD];
    let len = msg.encode(&mut buf);
    // `new` cannot fail: len <= MAX_SYNC_PAYLOAD == SPLIT_APP_MSG_MAX.
    let Some(data) = SplitAppData::new(&buf[..len]) else {
        return false;
    };
    SPLIT_APP_TX.try_send(data).is_ok()
}

/// Central-side split lighting state. Owned by the lighting task alongside
/// the compositor; see the module docs for the model.
pub struct CentralSplit {
    remote: RemoteOverlay<NUM_LEDS>,
    link_up: bool,
    /// `Some(t)` = a full resync is owed and should run at/after `t`
    /// (link-up edge or delta-queue overflow). While owed, delta queueing is
    /// suppressed — the resync will carry the final state.
    resync_at_ms: Option<u64>,
}

impl CentralSplit {
    // Constructed via `SplitRole::central()`, which only the central binary
    // calls; the peripheral binary compiles this as dead code.
    #[allow(dead_code)]
    pub const fn new() -> Self {
        Self { remote: RemoteOverlay::new(), link_up: false, resync_at_ms: None }
    }

    /// Live right-half cells as `(local key, cell, absolute expiry)`.
    pub fn remote_cells(&self) -> impl Iterator<Item = (u8, Cell, Option<u64>)> + '_ {
        self.remote.cells()
    }

    fn mark_resync(&mut self, at_ms: u64) {
        self.resync_at_ms = Some(match self.resync_at_ms {
            Some(cur) => cur.min(at_ms),
            None => at_ms,
        });
    }

    /// Whether deltas can be queued right now (link up, no resync owed).
    fn deltas_flow(&self) -> bool {
        self.link_up && self.resync_at_ms.is_none()
    }

    /// Queue a batch of cell writes; on overflow fall back to a resync.
    /// Returns `false` if the cells did not go out as deltas.
    fn queue_cells<'a>(&mut self, cells: impl Iterator<Item = &'a (u8, Cell)>, now_ms: u64) -> bool {
        if !self.deltas_flow() {
            return false;
        }
        let mut batch = SyncCells::new();
        for &(key, cell) in cells {
            batch.push(key, cell);
            if batch.is_full() {
                if !try_queue(&SyncMessage::SetCells(batch)) {
                    self.mark_resync(now_ms + RESYNC_RETRY_MS);
                    return false;
                }
                batch = SyncCells::new();
            }
        }
        if !batch.is_empty() && !try_queue(&SyncMessage::SetCells(batch)) {
            self.mark_resync(now_ms + RESYNC_RETRY_MS);
            return false;
        }
        true
    }

    /// Queue a batch of unsets; same overflow fallback as [`queue_cells`].
    fn queue_unsets<'a>(&mut self, keys: impl Iterator<Item = &'a u8>, now_ms: u64) -> bool {
        if !self.deltas_flow() {
            return false;
        }
        let mut batch = SyncKeys::new();
        for &key in keys {
            batch.push(key);
            if batch.is_full() {
                if !try_queue(&SyncMessage::UnsetKeys(batch)) {
                    self.mark_resync(now_ms + RESYNC_RETRY_MS);
                    return false;
                }
                batch = SyncKeys::new();
            }
        }
        if !batch.is_empty() && !try_queue(&SyncMessage::UnsetKeys(batch)) {
            self.mark_resync(now_ms + RESYNC_RETRY_MS);
            return false;
        }
        true
    }

    /// Store + forward right-half cell writes (`cells` in LOCAL keys, shared
    /// `ttl_ms` per the protocol). Returns `true` when the write reached a
    /// connected peripheral (⇒ protocol `OK`); `false` means the cells are
    /// held authoritatively here and will land via resync (⇒ `PARTIAL_APPLY`).
    pub fn write_cells(&mut self, cells: &[(u8, Cell)], ttl_ms: Option<u32>, now_ms: u64) -> bool {
        for &(key, cell) in cells {
            self.remote.set(key, cell, ttl_ms, now_ms);
        }
        self.queue_cells(cells.iter(), now_ms)
    }

    /// Store + forward right-half unsets (LOCAL keys).
    pub fn unset_keys(&mut self, keys: &[u8], now_ms: u64) -> bool {
        for &key in keys {
            self.remote.unset(key);
        }
        self.queue_unsets(keys.iter(), now_ms)
    }

    /// Clear the right half's overlay (store + forward).
    pub fn clear(&mut self, now_ms: u64) -> bool {
        self.remote.clear();
        if !self.deltas_flow() {
            return false;
        }
        if !try_queue(&SyncMessage::Clear) {
            self.mark_resync(now_ms + RESYNC_RETRY_MS);
            return false;
        }
        true
    }

    /// Atomically replace the right half's overlay with `cells` (store +
    /// forward as clear-then-set).
    pub fn replace_cells(&mut self, cells: &[(u8, Cell)], ttl_ms: Option<u32>, now_ms: u64) -> bool {
        self.remote.clear();
        for &(key, cell) in cells {
            self.remote.set(key, cell, ttl_ms, now_ms);
        }
        if !self.deltas_flow() {
            return false;
        }
        if !try_queue(&SyncMessage::Clear) {
            self.mark_resync(now_ms + RESYNC_RETRY_MS);
            return false;
        }
        self.queue_cells(cells.iter(), now_ms)
    }

    /// Best-effort forward of the shared state snapshot (brightness,
    /// effective ceiling, toggle bitmap). Falls back to resync on overflow.
    pub fn notify_state(&mut self, comp: &Compositor<NUM_LEDS>, now_ms: u64) {
        if !self.deltas_flow() {
            return; // resync (or the next link-up resync) carries it
        }
        let msg = SyncMessage::State {
            brightness: comp.brightness(),
            ceiling: comp.ceiling(),
            toggles: comp.toggles_mask(),
        };
        if !try_queue(&msg) {
            self.mark_resync(now_ms + RESYNC_RETRY_MS);
        }
    }

    /// React to a split-link edge. A `false → true` edge schedules the
    /// reconnect resync immediately.
    pub fn on_link_change(&mut self, up: bool, now_ms: u64) {
        self.link_up = up;
        self.resync_at_ms = if up { Some(now_ms) } else { None };
    }

    /// The next moment this state machine needs the loop to wake: a pending
    /// right-half TTL expiry or an owed resync.
    pub fn next_deadline(&self) -> Option<u64> {
        match (self.remote.next_expiry(), self.resync_at_ms) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Deadline housekeeping: expire right-half TTLs (forwarding the unsets
    /// — expiry authority lives here) and run an owed resync.
    pub fn service(&mut self, comp: &Compositor<NUM_LEDS>, now_ms: u64) {
        let expired = self.remote.expire(now_ms);
        if !expired.as_slice().is_empty() {
            self.queue_unsets(expired.as_slice().iter(), now_ms);
        }
        if matches!(self.resync_at_ms, Some(at) if at <= now_ms) && self.link_up {
            self.resync(comp, now_ms);
        }
    }

    /// Push the complete right-half picture: clear, every live cell, shared
    /// state. Idempotent; re-queued in full on any overflow.
    fn resync(&mut self, comp: &Compositor<NUM_LEDS>, now_ms: u64) {
        self.resync_at_ms = None;
        let mut ok = try_queue(&SyncMessage::Clear);
        let mut batch = SyncCells::new();
        for (key, cell, expires_at) in self.remote.cells() {
            if !ok {
                break;
            }
            if matches!(expires_at, Some(at) if at <= now_ms) {
                continue; // expired while disconnected; expire() will purge
            }
            batch.push(key, cell);
            if batch.is_full() {
                ok = try_queue(&SyncMessage::SetCells(batch));
                batch = SyncCells::new();
            }
        }
        if ok && !batch.is_empty() {
            ok = try_queue(&SyncMessage::SetCells(batch));
        }
        if ok {
            ok = try_queue(&SyncMessage::State {
                brightness: comp.brightness(),
                ceiling: comp.ceiling(),
                toggles: comp.toggles_mask(),
            });
        }
        if !ok {
            defmt::debug!("split-lighting: resync overflowed the queue, retrying");
            self.mark_resync(now_ms + RESYNC_RETRY_MS);
        }
    }
}

/// Peripheral-side split lighting state (see the module docs for the
/// link-loss policy).
pub struct PeripheralSplit {
    /// When set, the host overlay is cleared at/after this time unless the
    /// central link comes back first.
    clear_at_ms: Option<u64>,
}

impl PeripheralSplit {
    // See CentralSplit::new: only the peripheral binary constructs this.
    #[allow(dead_code)]
    pub const fn new() -> Self {
        Self { clear_at_ms: None }
    }

    pub fn on_link_change(&mut self, up: bool, now_ms: u64) {
        self.clear_at_ms = if up { None } else { Some(now_ms + LINK_LOSS_GRACE_MS) };
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.clear_at_ms
    }

    /// Deadline housekeeping: drop the authority-less host overlay once the
    /// link-loss grace expires.
    pub fn service(&mut self, comp: &mut Compositor<NUM_LEDS>, now_ms: u64) {
        if matches!(self.clear_at_ms, Some(at) if at <= now_ms) {
            defmt::info!("split-lighting: central link lost, clearing host overlay");
            comp.host_clear();
            self.clear_at_ms = None;
        }
    }

    /// Apply one received sync message to the local compositor. Cells carry
    /// no TTL by design; expiry arrives as an unset from the central.
    pub fn apply(&mut self, comp: &mut Compositor<NUM_LEDS>, payload: &[u8], now_ms: u64) {
        match SyncMessage::decode(payload) {
            Ok(SyncMessage::SetCells(cells)) => {
                for &(key, cell) in cells.entries() {
                    match cell {
                        // Transparent means "reveal what is below" — same as
                        // not having a host cell at all.
                        Cell::Transparent => comp.host_unset(key),
                        // Cannot overflow: one slot per key, keys < NUM_LEDS
                        // == the overlay capacity; guarded anyway.
                        cell => {
                            if comp.host_set(key, cell, None, now_ms).is_err() {
                                defmt::warn!("split-lighting: host overlay full, dropping key {}", key);
                            }
                        }
                    }
                }
            }
            Ok(SyncMessage::UnsetKeys(keys)) => {
                for &key in keys.keys() {
                    comp.host_unset(key);
                }
            }
            Ok(SyncMessage::Clear) => comp.host_clear(),
            Ok(SyncMessage::State { brightness, ceiling, toggles }) => {
                comp.set_brightness(brightness);
                // set_ceiling re-clamps to this half's compiled CHANNEL_CEILING.
                comp.set_ceiling(ceiling);
                comp.set_toggles_mask(toggles);
            }
            // Unknown version/tag: a newer central talking to an older
            // peripheral — ignore by contract. Anything else is a framing
            // bug worth a log line; dropping is always safe (state heals on
            // the next resync).
            Err(e) => defmt::warn!("split-lighting: dropped message: {}", defmt::Debug2Format(&e)),
        }
    }
}

/// Which side of the split this binary is, plus that side's lighting-sync
/// state. Owned by [`crate::lighting::LightingProcessor`] so the compositor
/// keeps exactly one owner.
// Each binary constructs exactly one variant (in `central.rs` /
// `peripheral.rs`), so the other variant and its constructor are dead code
// in that binary by design.
#[allow(dead_code)]
pub enum SplitRole {
    Central(CentralSplit),
    Peripheral(PeripheralSplit),
}

impl SplitRole {
    #[allow(dead_code)] // see the enum note
    pub const fn central() -> Self {
        SplitRole::Central(CentralSplit::new())
    }

    #[allow(dead_code)] // see the enum note
    pub const fn peripheral() -> Self {
        SplitRole::Peripheral(PeripheralSplit::new())
    }

    /// The central state, when this is the central (used by the host
    /// protocol semantics; `None` on the peripheral, which never receives
    /// host requests).
    pub fn central_mut(&mut self) -> Option<&mut CentralSplit> {
        match self {
            SplitRole::Central(c) => Some(c),
            SplitRole::Peripheral(_) => None,
        }
    }

    pub fn as_central(&self) -> Option<&CentralSplit> {
        match self {
            SplitRole::Central(c) => Some(c),
            SplitRole::Peripheral(_) => None,
        }
    }

    pub fn on_link_change(&mut self, up: bool, now_ms: u64) {
        match self {
            SplitRole::Central(c) => c.on_link_change(up, now_ms),
            SplitRole::Peripheral(p) => p.on_link_change(up, now_ms),
        }
    }

    /// The next self-driven wake this role needs (merged with the
    /// compositor's `next_wake_ms` by the lighting loop).
    pub fn next_deadline(&self) -> Option<u64> {
        match self {
            SplitRole::Central(c) => c.next_deadline(),
            SplitRole::Peripheral(p) => p.next_deadline(),
        }
    }

    /// Deadline housekeeping for either role.
    pub fn service(&mut self, comp: &mut Compositor<NUM_LEDS>, now_ms: u64) {
        match self {
            SplitRole::Central(c) => c.service(comp, now_ms),
            SplitRole::Peripheral(p) => p.service(comp, now_ms),
        }
    }

    /// Apply one received split application message (peripheral only; the
    /// central's inbox never fills, so this is unreachable there).
    pub fn apply_message(&mut self, comp: &mut Compositor<NUM_LEDS>, payload: &[u8], now_ms: u64) {
        match self {
            SplitRole::Central(_) => {
                defmt::warn!("split-lighting: unexpected app message on the central");
            }
            SplitRole::Peripheral(p) => p.apply(comp, payload, now_ms),
        }
    }
}
