// ===== GLOVE80 PATCH (host-protocol keymap ops) =====
//! Keymap operation pipe for the Glove80 host protocol (Phase 6).
//!
//! This module is a Glove80-local addition to RMK (carried in the vendored
//! subtree, see `rmk/vendor` in the parent repo). It lets the firmware
//! crate's host-protocol handler read and write keymap positions through the
//! **same owner and path Vial uses**: operations are serviced inside
//! `VialService::run` (`host/via/mod.rs`), which converts with
//! `to_via_keycode`/`from_via_keycode` and persists through
//! `KeyboardContext::set_action` — so Vial edits and host-protocol edits are
//! interchangeable and never race (one task owns both).
//!
//! Discipline: exactly one operation in flight. The single client (the
//! central's lighting task, while handling one host-protocol request) sends
//! on [`KEYMAP_OPS`] and then awaits [`KEYMAP_OP_RESULTS`]; nothing else
//! touches either channel. On binaries with no Vial service (the split
//! peripheral) nothing sends here.

use embassy_sync::channel::Channel;

use crate::RawMutex;

/// One keymap operation. Keycodes are the VIA/Vial 16-bit encoding — the
/// caller never sees `KeyAction`; conversion happens at the service side.
pub enum KeymapOp {
    /// Read the action at (layer, row, col) as a VIA keycode.
    Get { layer: u8, row: u8, col: u8 },
    /// Write a VIA keycode at (layer, row, col): decoded with
    /// `from_via_keycode`, applied to the live keymap and persisted to
    /// storage exactly like Vial's `DynamicKeymapSetKeyCode`.
    Set { layer: u8, row: u8, col: u8, keycode: u16 },
}

/// Operations from the host-protocol handler to the Vial service task.
pub static KEYMAP_OPS: Channel<RawMutex, KeymapOp, 1> = Channel::new();
/// One result per operation: the canonical VIA keycode now stored at the
/// position (for `Set`, re-read after the write — lossy mappings visible).
pub static KEYMAP_OP_RESULTS: Channel<RawMutex, u16, 1> = Channel::new();
// ===== END GLOVE80 PATCH =====
