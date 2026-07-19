// ===== GLOVE80 PATCH (host protocol transport) =====
//! Raw transport pipes for the Glove80 host protocol.
//!
//! This module is a Glove80-local addition to RMK (carried in the vendored
//! subtree, see `rmk/vendor` in the parent repo). It contains no protocol
//! knowledge: it only moves opaque frame chunks between the USB/BLE transport
//! tasks inside RMK and the firmware crate's protocol pump, which does all
//! framing/encode/decode with `glove80-host-protocol`.
//!
//! Wire shape (PROTOCOL.md "Frame layer"):
//! - USB: fixed 32-byte raw-HID reports on the vendor interface
//!   ([`crate::hid::HostProtocolReport`], usage page `0xFF88`).
//! - BLE: one chunk per ATT write-without-response / notification on the
//!   host-protocol GATT service (`ble::ble_server::HostProtoService`).
//!   Chunks are variable length, at most [`BLE_MAX_CHUNK_LEN`] bytes.

use core::sync::atomic::AtomicU16;

use embassy_sync::channel::Channel;

use crate::RawMutex;

/// USB raw-HID report length on the vendor interface.
pub const USB_REPORT_LEN: usize = 32;

/// Upper bound on one BLE chunk: 2-byte frame header + the frame layer's
/// 255-byte max chunk payload (`chunk_payload_len` is a u8).
pub const BLE_MAX_CHUNK_LEN: usize = 257;

/// One variable-length BLE chunk (an ATT write's payload, or the payload of
/// a notification about to be sent). Only `data[..len]` is meaningful.
#[derive(Clone)]
pub struct BleChunk {
    pub len: u16,
    pub data: [u8; BLE_MAX_CHUNK_LEN],
}

impl BleChunk {
    pub const fn empty() -> Self {
        Self {
            len: 0,
            data: [0; BLE_MAX_CHUNK_LEN],
        }
    }
}

/// Host → keyboard: 32-byte OUT reports received on the USB vendor interface.
pub static HOSTP_USB_RX: Channel<RawMutex, [u8; USB_REPORT_LEN], 4> = Channel::new();
/// Keyboard → host: 32-byte IN reports to send on the USB vendor interface.
/// Sized so one full response message (≤ 1536 bytes, 52 reports of 30-byte
/// payload) never blocks the producer while the interface is being drained.
pub static HOSTP_USB_TX: Channel<RawMutex, [u8; USB_REPORT_LEN], 52> = Channel::new();

/// Host → keyboard: writes to the host-protocol GATT request characteristic.
pub static HOSTP_BLE_RX: Channel<RawMutex, BleChunk, 2> = Channel::new();
/// Keyboard → host: chunks to notify on the response characteristic.
pub static HOSTP_BLE_TX: Channel<RawMutex, BleChunk, 4> = Channel::new();

/// Usable ATT payload (negotiated MTU - 3) of the live host BLE connection,
/// updated whenever a request write arrives (by which time the client's MTU
/// exchange has completed). The protocol pump sizes response chunks with it.
pub static HOSTP_BLE_ATT_PAYLOAD: AtomicU16 = AtomicU16::new(20);
// ===== END GLOVE80 PATCH =====
