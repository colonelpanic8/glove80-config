//! Split lighting transfer (Phase 3 of `docs/implementation-plan.md`):
//! the pure-logic half of forwarding host-overlay lighting from the split
//! central to the peripheral.
//!
//! Two things live here, both host-tested:
//!
//! - [`SyncMessage`]: the bounded, versioned payload codec for the split
//!   application channel (`rmk::split_app_pipe` in the vendored tree). Every
//!   encoded payload fits [`MAX_SYNC_PAYLOAD`] bytes; keys are LOCAL chain
//!   indices on the receiving half (the central remaps protocol keys 40..80
//!   to 0..40 before encoding). Messages are absolute state ("cell k is now
//!   X", "the toggle bitmap is now Y"), so receiving any of them twice — or
//!   receiving a full resync after deltas — is harmless.
//! - [`RemoteOverlay`]: the central's authoritative store for the
//!   peripheral's host-overlay cells, including TTL bookkeeping. TTL expiry
//!   authority stays on the central: cells forwarded to the peripheral carry
//!   no TTL; when a cell expires here the central sends the unset.
//!
//! Versioning: every payload starts with `[SYNC_VERSION, tag]`. New message
//! kinds are new tags (old receivers must ignore unknown tags); breaking
//! layout changes bump the version byte (receivers must ignore other
//! versions). Both cases decode to a distinct error so firmware can drop
//! them silently-but-logged.

use crate::Cell;

/// Version byte carried by every sync payload.
pub const SYNC_VERSION: u8 = 1;

/// Upper bound of one encoded sync payload. Must match the vendored split
/// pipe's `SPLIT_APP_MSG_MAX` (asserted in the firmware crate); kept small
/// because every split transfer, key events included, is sized by the
/// largest split message (which itself must stay ≤ 32 bytes — see the pipe).
pub const MAX_SYNC_PAYLOAD: usize = 26;

/// Cells per [`SyncMessage::SetCells`] batch:
/// `2 (header) + 1 (count) + 2 * 10 (cell entries) == 23 ≤ 26`.
pub const MAX_CELLS_PER_SYNC: usize = 2;

/// Keys per [`SyncMessage::UnsetKeys`] batch (3 + 16 = 19 ≤ 33).
pub const MAX_UNSETS_PER_SYNC: usize = 16;

const TAG_SET_CELLS: u8 = 0x01;
const TAG_UNSET_KEYS: u8 = 0x02;
const TAG_CLEAR: u8 = 0x03;
const TAG_STATE: u8 = 0x04;

/// Bytes of one `key + cell` entry on the wire.
const CELL_ENTRY_LEN: usize = 10;

/// Fixed-capacity cell batch for [`SyncMessage::SetCells`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SyncCells {
    len: u8,
    cells: [(u8, Cell); MAX_CELLS_PER_SYNC],
}

impl Default for SyncCells {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncCells {
    pub const fn new() -> Self {
        Self { len: 0, cells: [(0, Cell::Transparent); MAX_CELLS_PER_SYNC] }
    }

    /// Append an entry; `false` when full (entry not added).
    pub fn push(&mut self, key: u8, cell: Cell) -> bool {
        if (self.len as usize) == MAX_CELLS_PER_SYNC {
            return false;
        }
        self.cells[self.len as usize] = (key, cell);
        self.len += 1;
        true
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len as usize == MAX_CELLS_PER_SYNC
    }

    pub fn entries(&self) -> &[(u8, Cell)] {
        &self.cells[..self.len as usize]
    }
}

/// Fixed-capacity key batch for [`SyncMessage::UnsetKeys`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SyncKeys {
    len: u8,
    keys: [u8; MAX_UNSETS_PER_SYNC],
}

impl Default for SyncKeys {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncKeys {
    pub const fn new() -> Self {
        Self { len: 0, keys: [0; MAX_UNSETS_PER_SYNC] }
    }

    /// Append a key; `false` when full (key not added).
    pub fn push(&mut self, key: u8) -> bool {
        if (self.len as usize) == MAX_UNSETS_PER_SYNC {
            return false;
        }
        self.keys[self.len as usize] = key;
        self.len += 1;
        true
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len as usize == MAX_UNSETS_PER_SYNC
    }

    pub fn keys(&self) -> &[u8] {
        &self.keys[..self.len as usize]
    }
}

/// One central → peripheral lighting sync message. All state is absolute and
/// idempotent; keys are local chain indices on the receiving half.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SyncMessage {
    /// Set (or, for [`Cell::Transparent`], unset) host-overlay cells. Cells
    /// carry no TTL — TTL authority stays with the central, which sends
    /// [`SyncMessage::UnsetKeys`] on expiry.
    SetCells(SyncCells),
    /// Remove host-overlay cells.
    UnsetKeys(SyncKeys),
    /// Clear the whole host overlay.
    Clear,
    /// Shared lighting state snapshot: brightness scalar, effective ceiling
    /// (still bounded by the receiver's compiled `CHANNEL_CEILING`), and the
    /// full toggle bitmap.
    State { brightness: u8, ceiling: u8, toggles: u32 },
}

/// Why a payload failed to decode. `UnsupportedVersion` / `UnknownTag` are
/// the forward-compatibility cases receivers must silently ignore.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SyncDecodeError {
    UnsupportedVersion(u8),
    UnknownTag(u8),
    UnknownCellKind(u8),
    /// Payload shorter or longer than the tag's layout requires.
    BadLength,
}

fn put_u16(out: &mut [u8], at: usize, v: u16) {
    out[at] = (v & 0xff) as u8;
    out[at + 1] = (v >> 8) as u8;
}

fn get_u16(bytes: &[u8], at: usize) -> u16 {
    bytes[at] as u16 | ((bytes[at + 1] as u16) << 8)
}

/// Encode one `key + cell` entry (10 bytes) at `at`.
fn put_cell(out: &mut [u8], at: usize, key: u8, cell: &Cell) {
    let (kind, color, period_ms, phase_ms, duty) = match *cell {
        Cell::Solid { color } => (0u8, color, 0, 0, 0),
        Cell::Blink { color, period_ms, phase_ms, duty_pct } => (1, color, period_ms, phase_ms, duty_pct),
        Cell::Breathe { color, period_ms, phase_ms } => (2, color, period_ms, phase_ms, 0),
        Cell::Transparent => (3, crate::Rgb::OFF, 0, 0, 0),
    };
    out[at] = key;
    out[at + 1] = kind;
    out[at + 2] = color.r;
    out[at + 3] = color.g;
    out[at + 4] = color.b;
    put_u16(out, at + 5, period_ms);
    put_u16(out, at + 7, phase_ms);
    out[at + 9] = duty;
}

/// Decode one `key + cell` entry at `at` (bounds already checked).
fn get_cell(bytes: &[u8], at: usize) -> Result<(u8, Cell), SyncDecodeError> {
    let key = bytes[at];
    let color = crate::Rgb::new(bytes[at + 2], bytes[at + 3], bytes[at + 4]);
    let period_ms = get_u16(bytes, at + 5);
    let phase_ms = get_u16(bytes, at + 7);
    let duty_pct = bytes[at + 9];
    let cell = match bytes[at + 1] {
        0 => Cell::Solid { color },
        1 => Cell::Blink { color, period_ms, phase_ms, duty_pct },
        2 => Cell::Breathe { color, period_ms, phase_ms },
        3 => Cell::Transparent,
        k => return Err(SyncDecodeError::UnknownCellKind(k)),
    };
    Ok((key, cell))
}

impl SyncMessage {
    /// Encode into `out`; returns the encoded length (≤ [`MAX_SYNC_PAYLOAD`]).
    pub fn encode(&self, out: &mut [u8; MAX_SYNC_PAYLOAD]) -> usize {
        out[0] = SYNC_VERSION;
        match self {
            SyncMessage::SetCells(cells) => {
                out[1] = TAG_SET_CELLS;
                out[2] = cells.len;
                for (i, (key, cell)) in cells.entries().iter().enumerate() {
                    put_cell(out, 3 + i * CELL_ENTRY_LEN, *key, cell);
                }
                3 + cells.entries().len() * CELL_ENTRY_LEN
            }
            SyncMessage::UnsetKeys(keys) => {
                out[1] = TAG_UNSET_KEYS;
                out[2] = keys.len;
                out[3..3 + keys.keys().len()].copy_from_slice(keys.keys());
                3 + keys.keys().len()
            }
            SyncMessage::Clear => {
                out[1] = TAG_CLEAR;
                2
            }
            SyncMessage::State { brightness, ceiling, toggles } => {
                out[1] = TAG_STATE;
                out[2] = *brightness;
                out[3] = *ceiling;
                out[4] = (*toggles & 0xff) as u8;
                out[5] = ((*toggles >> 8) & 0xff) as u8;
                out[6] = ((*toggles >> 16) & 0xff) as u8;
                out[7] = ((*toggles >> 24) & 0xff) as u8;
                8
            }
        }
    }

    /// Decode one payload. Known tags require exactly their layout's length
    /// (additive evolution uses new tags, never trailing bytes).
    pub fn decode(bytes: &[u8]) -> Result<SyncMessage, SyncDecodeError> {
        if bytes.len() < 2 {
            return Err(SyncDecodeError::BadLength);
        }
        if bytes[0] != SYNC_VERSION {
            return Err(SyncDecodeError::UnsupportedVersion(bytes[0]));
        }
        match bytes[1] {
            TAG_SET_CELLS => {
                if bytes.len() < 3 {
                    return Err(SyncDecodeError::BadLength);
                }
                let count = bytes[2] as usize;
                if count > MAX_CELLS_PER_SYNC || bytes.len() != 3 + count * CELL_ENTRY_LEN {
                    return Err(SyncDecodeError::BadLength);
                }
                let mut cells = SyncCells::new();
                for i in 0..count {
                    let (key, cell) = get_cell(bytes, 3 + i * CELL_ENTRY_LEN)?;
                    cells.push(key, cell);
                }
                Ok(SyncMessage::SetCells(cells))
            }
            TAG_UNSET_KEYS => {
                if bytes.len() < 3 {
                    return Err(SyncDecodeError::BadLength);
                }
                let count = bytes[2] as usize;
                if count > MAX_UNSETS_PER_SYNC || bytes.len() != 3 + count {
                    return Err(SyncDecodeError::BadLength);
                }
                let mut keys = SyncKeys::new();
                for &key in &bytes[3..3 + count] {
                    keys.push(key);
                }
                Ok(SyncMessage::UnsetKeys(keys))
            }
            TAG_CLEAR => {
                if bytes.len() != 2 {
                    return Err(SyncDecodeError::BadLength);
                }
                Ok(SyncMessage::Clear)
            }
            TAG_STATE => {
                if bytes.len() != 8 {
                    return Err(SyncDecodeError::BadLength);
                }
                let toggles = bytes[4] as u32
                    | ((bytes[5] as u32) << 8)
                    | ((bytes[6] as u32) << 16)
                    | ((bytes[7] as u32) << 24);
                Ok(SyncMessage::State { brightness: bytes[2], ceiling: bytes[3], toggles })
            }
            tag => Err(SyncDecodeError::UnknownTag(tag)),
        }
    }
}

/// The central's authoritative store for the peripheral half's host-overlay
/// cells, indexed by LOCAL key (`0..N`). Mirrors the semantics of the
/// compositor's own host overlay (set replaces, TTL expiry reverts to
/// transparent) without rendering anything.
pub struct RemoteOverlay<const N: usize> {
    /// `cell, absolute expiry (now_ms scale)` per local key.
    cells: [Option<(Cell, Option<u64>)>; N],
}

impl<const N: usize> Default for RemoteOverlay<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Keys removed by [`RemoteOverlay::expire`].
pub struct ExpiredKeys<const N: usize> {
    len: usize,
    keys: [u8; N],
}

impl<const N: usize> ExpiredKeys<N> {
    pub fn as_slice(&self) -> &[u8] {
        &self.keys[..self.len]
    }
}

impl<const N: usize> RemoteOverlay<N> {
    pub const fn new() -> Self {
        Self { cells: [None; N] }
    }

    /// Set or replace the cell for local `key`, optionally expiring `ttl_ms`
    /// after `now_ms`. Out-of-range keys are ignored (`false`).
    pub fn set(&mut self, key: u8, cell: Cell, ttl_ms: Option<u32>, now_ms: u64) -> bool {
        match self.cells.get_mut(key as usize) {
            Some(slot) => {
                *slot = Some((cell, ttl_ms.map(|t| now_ms + t as u64)));
                true
            }
            None => false,
        }
    }

    pub fn unset(&mut self, key: u8) {
        if let Some(slot) = self.cells.get_mut(key as usize) {
            *slot = None;
        }
    }

    pub fn clear(&mut self) {
        self.cells = [None; N];
    }

    /// Live entries as `(local key, cell, absolute expiry)`. Entries past
    /// their expiry may linger until [`expire`](Self::expire) runs; callers
    /// comparing against a clock must filter, as the compositor's read-back
    /// path does.
    pub fn cells(&self) -> impl Iterator<Item = (u8, Cell, Option<u64>)> + '_ {
        self.cells
            .iter()
            .enumerate()
            .filter_map(|(k, slot)| slot.map(|(cell, exp)| (k as u8, cell, exp)))
    }

    /// Drop every cell whose expiry has passed and return their keys (so the
    /// central can forward the unsets).
    pub fn expire(&mut self, now_ms: u64) -> ExpiredKeys<N> {
        let mut expired = ExpiredKeys { len: 0, keys: [0; N] };
        for (k, slot) in self.cells.iter_mut().enumerate() {
            if matches!(slot, Some((_, Some(at))) if *at <= now_ms) {
                *slot = None;
                expired.keys[expired.len] = k as u8;
                expired.len += 1;
            }
        }
        expired
    }

    /// The earliest pending expiry, or `None` when nothing can expire.
    pub fn next_expiry(&self) -> Option<u64> {
        self.cells.iter().flatten().filter_map(|(_, exp)| *exp).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rgb;

    const RED: Rgb = Rgb::new(200, 0, 0);

    fn solid(color: Rgb) -> Cell {
        Cell::Solid { color }
    }

    fn roundtrip(msg: SyncMessage) {
        let mut buf = [0u8; MAX_SYNC_PAYLOAD];
        let len = msg.encode(&mut buf);
        assert!(len <= MAX_SYNC_PAYLOAD);
        assert_eq!(SyncMessage::decode(&buf[..len]), Ok(msg));
    }

    #[test]
    fn messages_roundtrip() {
        let mut cells = SyncCells::new();
        assert!(cells.push(17, Cell::Blink { color: RED, period_ms: 500, phase_ms: 100, duty_pct: 25 }));
        assert!(cells.push(39, Cell::Breathe { color: Rgb::new(0, 0, 255), period_ms: 3000, phase_ms: 1500 }));
        assert!(cells.is_full());
        assert!(!cells.push(5, solid(RED)), "capacity enforced");
        roundtrip(SyncMessage::SetCells(cells));
        roundtrip(SyncMessage::SetCells(SyncCells::new()));

        let mut transparent = SyncCells::new();
        transparent.push(4, Cell::Transparent);
        roundtrip(SyncMessage::SetCells(transparent));

        let mut keys = SyncKeys::new();
        for k in 0..MAX_UNSETS_PER_SYNC {
            assert!(keys.push(k as u8));
        }
        assert!(!keys.push(99), "capacity enforced");
        roundtrip(SyncMessage::UnsetKeys(keys));

        roundtrip(SyncMessage::Clear);
        roundtrip(SyncMessage::State { brightness: 128, ceiling: 204, toggles: 0xA5A5_5A5A });
    }

    #[test]
    fn full_batch_fits_the_payload() {
        let mut cells = SyncCells::new();
        for k in 0..MAX_CELLS_PER_SYNC {
            cells.push(k as u8, solid(RED));
        }
        let mut buf = [0u8; MAX_SYNC_PAYLOAD];
        let len = SyncMessage::SetCells(cells).encode(&mut buf);
        assert_eq!(len, 3 + MAX_CELLS_PER_SYNC * 10);
        assert!(len <= MAX_SYNC_PAYLOAD);
        // The largest of the other kinds also fits.
        let mut keys = SyncKeys::new();
        for k in 0..MAX_UNSETS_PER_SYNC {
            keys.push(k as u8);
        }
        assert!(SyncMessage::UnsetKeys(keys).encode(&mut buf) <= MAX_SYNC_PAYLOAD);
    }

    #[test]
    fn decode_rejects_foreign_and_malformed_payloads() {
        // Version and tag are the tolerated forward-compat rejections.
        assert_eq!(
            SyncMessage::decode(&[SYNC_VERSION + 1, TAG_CLEAR]),
            Err(SyncDecodeError::UnsupportedVersion(SYNC_VERSION + 1))
        );
        assert_eq!(SyncMessage::decode(&[SYNC_VERSION, 0x7F]), Err(SyncDecodeError::UnknownTag(0x7F)));
        // Length must match the tag's layout exactly.
        assert_eq!(SyncMessage::decode(&[SYNC_VERSION]), Err(SyncDecodeError::BadLength));
        assert_eq!(SyncMessage::decode(&[SYNC_VERSION, TAG_CLEAR, 0]), Err(SyncDecodeError::BadLength));
        assert_eq!(SyncMessage::decode(&[SYNC_VERSION, TAG_SET_CELLS, 1, 0, 0]), Err(SyncDecodeError::BadLength));
        assert_eq!(
            SyncMessage::decode(&[SYNC_VERSION, TAG_SET_CELLS, 3]),
            Err(SyncDecodeError::BadLength),
            "count above MAX_CELLS_PER_SYNC"
        );
        assert_eq!(SyncMessage::decode(&[SYNC_VERSION, TAG_STATE, 0, 0]), Err(SyncDecodeError::BadLength));
        // Unknown cell kind inside an otherwise valid batch.
        let mut cells = SyncCells::new();
        cells.push(0, solid(RED));
        let mut buf = [0u8; MAX_SYNC_PAYLOAD];
        let len = SyncMessage::SetCells(cells).encode(&mut buf);
        buf[4] = 9; // kind byte of entry 0
        assert_eq!(SyncMessage::decode(&buf[..len]), Err(SyncDecodeError::UnknownCellKind(9)));
    }

    #[test]
    fn remote_overlay_set_unset_clear() {
        let mut r = RemoteOverlay::<40>::new();
        assert!(r.set(3, solid(RED), None, 0));
        assert!(r.set(3, Cell::Transparent, None, 0), "set replaces by key");
        assert!(r.set(39, solid(RED), Some(100), 0));
        assert!(!r.set(40, solid(RED), None, 0), "out of range ignored");
        let mut got: Vec<_> = r.cells().collect();
        got.sort_by_key(|(k, _, _)| *k);
        assert_eq!(got, vec![(3, Cell::Transparent, None), (39, solid(RED), Some(100))]);

        r.unset(3);
        assert_eq!(r.cells().count(), 1);
        r.clear();
        assert_eq!(r.cells().count(), 0);
        assert_eq!(r.next_expiry(), None);
    }

    #[test]
    fn remote_overlay_ttl_expiry() {
        let mut r = RemoteOverlay::<40>::new();
        r.set(1, solid(RED), Some(1000), 0);
        r.set(2, solid(RED), Some(500), 0);
        r.set(3, solid(RED), None, 0);
        assert_eq!(r.next_expiry(), Some(500));

        assert_eq!(r.expire(499).as_slice(), &[] as &[u8]);
        assert_eq!(r.expire(500).as_slice(), &[2]);
        assert_eq!(r.next_expiry(), Some(1000));
        assert_eq!(r.expire(5000).as_slice(), &[1]);
        assert_eq!(r.next_expiry(), None, "TTL-less cell never expires");
        assert_eq!(r.cells().count(), 1);
    }
}
