//! Transport-independent wire codec for the Glove80 host protocol.
//!
//! Byte-level spec: `PROTOCOL.md` next to this crate. Golden vectors shared
//! with the TypeScript codec: `protocol/vectors/host-protocol-v1.json`.
//!
//! The crate is `no_std` (core + `heapless`) so the firmware can embed it;
//! the `std` feature only adds `std::error::Error` impls.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod error;
pub mod frame;
mod io;
mod message;

pub use error::{DecodeError, EncodeError, FrameError};
pub use message::{
    decode_request, decode_response, encode_request, encode_response, feature, BootTarget,
    Capabilities, CellState, CellWrite, Command, Effect, EffectKind, Request, Response,
    ResponsePayload, Status,
};

/// Protocol major version. A major bump is a breaking change.
pub const PROTOCOL_VERSION_MAJOR: u8 = 1;
/// Protocol minor version. Minor bumps are additive.
pub const PROTOCOL_VERSION_MINOR: u8 = 0;

/// Bit 7 of the opcode byte marks a response.
pub const RESPONSE_FLAG: u8 = 0x80;

/// Request header: opcode, request_id, payload_len (u16 LE).
pub const REQUEST_HEADER_LEN: usize = 4;
/// Response header: opcode|0x80, request_id, status, payload_len (u16 LE).
pub const RESPONSE_HEADER_LEN: usize = 5;

/// Upper bound on a whole message (header + payload).
pub const MAX_MESSAGE_LEN: usize = 1536;
/// Codec-side bound on cells/keys per message. Devices advertise their own
/// (possibly smaller) `max_cells_per_op` in the capability response.
pub const MAX_CELLS_PER_MESSAGE: usize = 80;
/// Maximum PING/echo payload.
pub const MAX_PING_LEN: usize = 64;

/// Required magic for `ENTER_BOOTLOADER`.
pub const BOOTLOADER_MAGIC: u32 = 0xB007_10AD;
