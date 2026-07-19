//! Message types and the transport-independent encode/decode.
//!
//! Byte layouts are specified exhaustively in `PROTOCOL.md`.

use heapless::Vec;

use crate::error::{DecodeError, EncodeError};
use crate::io::{Reader, Writer};
use crate::{
    MAX_CELLS_PER_MESSAGE, MAX_PING_LEN, REQUEST_HEADER_LEN, RESPONSE_FLAG, RESPONSE_HEADER_LEN,
};

/// Feature bits advertised in [`Capabilities::feature_bits`].
pub mod feature {
    /// Per-write TTL supported.
    pub const TTL: u32 = 1 << 0;
    /// Toggle overlays reachable via GET/SET_TOGGLE.
    pub const TOGGLES: u32 = 1 << 1;
    /// Programmatic bootloader entry.
    pub const BOOTLOADER_ENTRY: u32 = 1 << 2;
    /// REPLACE_OVERLAY supported.
    pub const ATOMIC_REPLACE: u32 = 1 << 3;
    /// READ_OVERLAY supported.
    pub const OVERLAY_READBACK: u32 = 1 << 4;
    /// PARTIAL_APPLY reporting (peripheral offline is reported, not hidden).
    pub const PARTIAL_APPLY: u32 = 1 << 5;
}

/// Command opcodes (always < 0x80; responses set [`RESPONSE_FLAG`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    GetCapabilities = 0x01,
    Ping = 0x02,
    SetCells = 0x10,
    UnsetCells = 0x11,
    ClearOverlay = 0x12,
    ReadOverlay = 0x13,
    ReplaceOverlay = 0x14,
    GetBrightness = 0x20,
    SetBrightness = 0x21,
    GetToggle = 0x30,
    SetToggle = 0x31,
    EnterBootloader = 0x7F,
}

impl Command {
    pub fn opcode(self) -> u8 {
        self as u8
    }

    pub fn from_opcode(op: u8) -> Option<Command> {
        Some(match op {
            0x01 => Command::GetCapabilities,
            0x02 => Command::Ping,
            0x10 => Command::SetCells,
            0x11 => Command::UnsetCells,
            0x12 => Command::ClearOverlay,
            0x13 => Command::ReadOverlay,
            0x14 => Command::ReplaceOverlay,
            0x20 => Command::GetBrightness,
            0x21 => Command::SetBrightness,
            0x30 => Command::GetToggle,
            0x31 => Command::SetToggle,
            0x7F => Command::EnterBootloader,
            _ => return None,
        })
    }

    /// The four overlay-write commands that ack with an overlay ack and may
    /// report PARTIAL_APPLY.
    pub fn is_overlay_write(self) -> bool {
        matches!(
            self,
            Command::SetCells | Command::UnsetCells | Command::ClearOverlay | Command::ReplaceOverlay
        )
    }
}

/// Response status codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    Ok = 0x00,
    UnknownCommand = 0x01,
    Malformed = 0x02,
    OutOfRange = 0x03,
    CapacityExceeded = 0x04,
    PartialApply = 0x05,
    Busy = 0x06,
    UnknownToggle = 0x07,
    BadMagic = 0x08,
    UnsupportedVersion = 0x09,
}

impl Status {
    pub fn from_u8(v: u8) -> Option<Status> {
        Some(match v {
            0x00 => Status::Ok,
            0x01 => Status::UnknownCommand,
            0x02 => Status::Malformed,
            0x03 => Status::OutOfRange,
            0x04 => Status::CapacityExceeded,
            0x05 => Status::PartialApply,
            0x06 => Status::Busy,
            0x07 => Status::UnknownToggle,
            0x08 => Status::BadMagic,
            0x09 => Status::UnsupportedVersion,
            _ => return None,
        })
    }
}

/// Effect kinds; bit positions in [`Capabilities::effect_mask`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectKind {
    Solid = 0,
    Blink = 1,
    Breathe = 2,
}

impl EffectKind {
    pub fn from_u8(v: u8) -> Option<EffectKind> {
        Some(match v {
            0 => EffectKind::Solid,
            1 => EffectKind::Blink,
            2 => EffectKind::Breathe,
            _ => return None,
        })
    }
}

/// A fixed 10-byte effect record. Fields not applicable to `kind` should be
/// encoded as 0 but round-trip verbatim either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    pub kind: EffectKind,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub period_ms: u16,
    pub phase_ms: u16,
    pub duty_percent: u8,
}

impl Effect {
    pub const ENCODED_LEN: usize = 10;

    pub fn solid(r: u8, g: u8, b: u8) -> Effect {
        Effect { kind: EffectKind::Solid, r, g, b, period_ms: 0, phase_ms: 0, duty_percent: 0 }
    }

    pub fn blink(r: u8, g: u8, b: u8, period_ms: u16, phase_ms: u16, duty_percent: u8) -> Effect {
        Effect { kind: EffectKind::Blink, r, g, b, period_ms, phase_ms, duty_percent }
    }

    pub fn breathe(r: u8, g: u8, b: u8, period_ms: u16, phase_ms: u16) -> Effect {
        Effect { kind: EffectKind::Breathe, r, g, b, period_ms, phase_ms, duty_percent: 0 }
    }

    fn write(&self, w: &mut Writer<'_>) -> Result<(), EncodeError> {
        w.u8(self.kind as u8)?;
        w.u8(self.r)?;
        w.u8(self.g)?;
        w.u8(self.b)?;
        w.u16(self.period_ms)?;
        w.u16(self.phase_ms)?;
        w.u8(self.duty_percent)?;
        w.u8(0) // reserved
    }

    fn read(r: &mut Reader<'_>) -> Result<Effect, DecodeError> {
        let kind_byte = r.u8()?;
        let kind = EffectKind::from_u8(kind_byte).ok_or(DecodeError::UnknownEffectKind(kind_byte))?;
        let red = r.u8()?;
        let green = r.u8()?;
        let blue = r.u8()?;
        let period_ms = r.u16()?;
        let phase_ms = r.u16()?;
        let duty_percent = r.u8()?;
        let _reserved = r.u8()?; // ignored for forward compatibility
        Ok(Effect { kind, r: red, g: green, b: blue, period_ms, phase_ms, duty_percent })
    }
}

/// One cell in a SET/REPLACE batch: 11 bytes on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellWrite {
    pub key: u8,
    pub effect: Effect,
}

/// One cell in a READ_OVERLAY response: 15 bytes on the wire.
/// `remaining_ttl_ms == 0` means the cell has no TTL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellState {
    pub key: u8,
    pub effect: Effect,
    pub remaining_ttl_ms: u32,
}

/// Capability response payload (16 bytes). Tools must never assume
/// capacities; everything they rely on is advertised here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub protocol_major: u8,
    pub protocol_minor: u8,
    pub led_count_left: u8,
    pub led_count_right: u8,
    pub layer_capacity: u8,
    pub max_cells_per_op: u8,
    /// Bit n set ⇔ effect kind n supported.
    pub effect_mask: u16,
    pub overlay_cell_capacity: u16,
    pub max_message_len: u16,
    pub feature_bits: u32,
}

/// Bootloader entry target half.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BootTarget {
    Central = 0,
    Peripheral = 1,
}

impl BootTarget {
    pub fn from_u8(v: u8) -> Option<BootTarget> {
        Some(match v {
            0 => BootTarget::Central,
            1 => BootTarget::Peripheral,
            _ => return None,
        })
    }
}

/// A request message (payload part; `request_id` travels alongside).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    GetCapabilities { client_major: u8, client_minor: u8 },
    Ping { data: Vec<u8, MAX_PING_LEN> },
    SetCells { ttl_ms: u32, cells: Vec<CellWrite, MAX_CELLS_PER_MESSAGE> },
    UnsetCells { keys: Vec<u8, MAX_CELLS_PER_MESSAGE> },
    ClearOverlay,
    ReadOverlay,
    ReplaceOverlay { ttl_ms: u32, cells: Vec<CellWrite, MAX_CELLS_PER_MESSAGE> },
    GetBrightness,
    SetBrightness { level: u8 },
    GetToggle { id: u8 },
    SetToggle { id: u8, state: bool },
    EnterBootloader { magic: u32, target: BootTarget },
}

impl Request {
    pub fn command(&self) -> Command {
        match self {
            Request::GetCapabilities { .. } => Command::GetCapabilities,
            Request::Ping { .. } => Command::Ping,
            Request::SetCells { .. } => Command::SetCells,
            Request::UnsetCells { .. } => Command::UnsetCells,
            Request::ClearOverlay => Command::ClearOverlay,
            Request::ReadOverlay => Command::ReadOverlay,
            Request::ReplaceOverlay { .. } => Command::ReplaceOverlay,
            Request::GetBrightness => Command::GetBrightness,
            Request::SetBrightness { .. } => Command::SetBrightness,
            Request::GetToggle { .. } => Command::GetToggle,
            Request::SetToggle { .. } => Command::SetToggle,
            Request::EnterBootloader { .. } => Command::EnterBootloader,
        }
    }
}

/// Response payload, discriminated by command + status on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponsePayload {
    /// Error statuses and ENTER_BOOTLOADER ok.
    Empty,
    Capabilities(Capabilities),
    Echo { data: Vec<u8, MAX_PING_LEN> },
    /// Ack for the four overlay writes; `pending_keys` lists keys accepted on
    /// the central but not yet applied on the peripheral.
    OverlayAck { pending_keys: Vec<u8, MAX_CELLS_PER_MESSAGE> },
    OverlayState { cells: Vec<CellState, MAX_CELLS_PER_MESSAGE> },
    Brightness { level: u8 },
    Toggle { id: u8, state: bool },
}

/// A full response message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub request_id: u8,
    pub command: Command,
    pub status: Status,
    pub payload: ResponsePayload,
}

fn write_cells(
    w: &mut Writer<'_>,
    ttl_ms: u32,
    cells: &[CellWrite],
) -> Result<(), EncodeError> {
    w.u32(ttl_ms)?;
    w.u8(cells.len() as u8)?;
    for cell in cells {
        w.u8(cell.key)?;
        cell.effect.write(w)?;
    }
    Ok(())
}

fn read_cells(
    r: &mut Reader<'_>,
) -> Result<(u32, Vec<CellWrite, MAX_CELLS_PER_MESSAGE>), DecodeError> {
    let ttl_ms = r.u32()?;
    let count = r.u8()? as usize;
    let mut cells = Vec::new();
    for _ in 0..count {
        let key = r.u8()?;
        let effect = Effect::read(r)?;
        cells.push(CellWrite { key, effect }).map_err(|_| DecodeError::CapacityExceeded)?;
    }
    Ok((ttl_ms, cells))
}

/// Encode a request into `out`; returns the number of bytes written.
pub fn encode_request(request_id: u8, req: &Request, out: &mut [u8]) -> Result<usize, EncodeError> {
    let mut w = Writer::new(out);
    w.u8(req.command().opcode())?;
    w.u8(request_id)?;
    w.u16(0)?; // payload_len, patched below
    match req {
        Request::GetCapabilities { client_major, client_minor } => {
            w.u8(*client_major)?;
            w.u8(*client_minor)?;
        }
        Request::Ping { data } => w.bytes(data)?,
        Request::SetCells { ttl_ms, cells } | Request::ReplaceOverlay { ttl_ms, cells } => {
            write_cells(&mut w, *ttl_ms, cells)?;
        }
        Request::UnsetCells { keys } => {
            w.u8(keys.len() as u8)?;
            w.bytes(keys)?;
        }
        Request::ClearOverlay | Request::ReadOverlay | Request::GetBrightness => {}
        Request::SetBrightness { level } => w.u8(*level)?,
        Request::GetToggle { id } => w.u8(*id)?,
        Request::SetToggle { id, state } => {
            w.u8(*id)?;
            w.u8(*state as u8)?;
        }
        Request::EnterBootloader { magic, target } => {
            w.u32(*magic)?;
            w.u8(*target as u8)?;
        }
    }
    let len = w.pos();
    w.patch_u16(2, (len - REQUEST_HEADER_LEN) as u16);
    Ok(len)
}

/// Decode a complete request message. Returns `(request_id, request)`.
pub fn decode_request(bytes: &[u8]) -> Result<(u8, Request), DecodeError> {
    let mut r = Reader::new(bytes);
    let opcode = r.u8()?;
    let command = Command::from_opcode(opcode).ok_or(DecodeError::UnknownOpcode(opcode))?;
    let request_id = r.u8()?;
    let payload_len = r.u16()? as usize;
    if r.remaining() != payload_len {
        return Err(DecodeError::LengthMismatch);
    }
    let request = match command {
        Command::GetCapabilities => {
            let client_major = r.u8()?;
            let client_minor = r.u8()?;
            Request::GetCapabilities { client_major, client_minor }
        }
        Command::Ping => {
            if payload_len > MAX_PING_LEN {
                return Err(DecodeError::CapacityExceeded);
            }
            let mut data = Vec::new();
            data.extend_from_slice(r.bytes(payload_len)?)
                .map_err(|_| DecodeError::CapacityExceeded)?;
            Request::Ping { data }
        }
        Command::SetCells => {
            let (ttl_ms, cells) = read_cells(&mut r)?;
            Request::SetCells { ttl_ms, cells }
        }
        Command::ReplaceOverlay => {
            let (ttl_ms, cells) = read_cells(&mut r)?;
            Request::ReplaceOverlay { ttl_ms, cells }
        }
        Command::UnsetCells => {
            let count = r.u8()? as usize;
            if count > MAX_CELLS_PER_MESSAGE {
                return Err(DecodeError::CapacityExceeded);
            }
            let mut keys = Vec::new();
            keys.extend_from_slice(r.bytes(count)?)
                .map_err(|_| DecodeError::CapacityExceeded)?;
            Request::UnsetCells { keys }
        }
        Command::ClearOverlay => Request::ClearOverlay,
        Command::ReadOverlay => Request::ReadOverlay,
        Command::GetBrightness => Request::GetBrightness,
        Command::SetBrightness => Request::SetBrightness { level: r.u8()? },
        Command::GetToggle => Request::GetToggle { id: r.u8()? },
        Command::SetToggle => {
            let id = r.u8()?;
            let state = match r.u8()? {
                0 => false,
                1 => true,
                v => return Err(DecodeError::BadToggleState(v)),
            };
            Request::SetToggle { id, state }
        }
        Command::EnterBootloader => {
            let magic = r.u32()?;
            let target_byte = r.u8()?;
            let target =
                BootTarget::from_u8(target_byte).ok_or(DecodeError::UnknownBootTarget(target_byte))?;
            Request::EnterBootloader { magic, target }
        }
    };
    r.finish()?;
    Ok((request_id, request))
}

fn payload_matches(command: Command, status: Status, payload: &ResponsePayload) -> bool {
    match status {
        Status::Ok => match (command, payload) {
            (Command::GetCapabilities, ResponsePayload::Capabilities(_)) => true,
            (Command::Ping, ResponsePayload::Echo { .. }) => true,
            (c, ResponsePayload::OverlayAck { .. }) if c.is_overlay_write() => true,
            (Command::ReadOverlay, ResponsePayload::OverlayState { .. }) => true,
            (Command::GetBrightness | Command::SetBrightness, ResponsePayload::Brightness { .. }) => {
                true
            }
            (Command::GetToggle | Command::SetToggle, ResponsePayload::Toggle { .. }) => true,
            (Command::EnterBootloader, ResponsePayload::Empty) => true,
            _ => false,
        },
        Status::PartialApply => {
            command.is_overlay_write() && matches!(payload, ResponsePayload::OverlayAck { .. })
        }
        _ => matches!(payload, ResponsePayload::Empty),
    }
}

/// Encode a response into `out`; returns the number of bytes written.
pub fn encode_response(resp: &Response, out: &mut [u8]) -> Result<usize, EncodeError> {
    if !payload_matches(resp.command, resp.status, &resp.payload) {
        return Err(EncodeError::PayloadMismatch);
    }
    let mut w = Writer::new(out);
    w.u8(resp.command.opcode() | RESPONSE_FLAG)?;
    w.u8(resp.request_id)?;
    w.u8(resp.status as u8)?;
    w.u16(0)?; // payload_len, patched below
    match &resp.payload {
        ResponsePayload::Empty => {}
        ResponsePayload::Capabilities(c) => {
            w.u8(c.protocol_major)?;
            w.u8(c.protocol_minor)?;
            w.u8(c.led_count_left)?;
            w.u8(c.led_count_right)?;
            w.u8(c.layer_capacity)?;
            w.u8(c.max_cells_per_op)?;
            w.u16(c.effect_mask)?;
            w.u16(c.overlay_cell_capacity)?;
            w.u16(c.max_message_len)?;
            w.u32(c.feature_bits)?;
        }
        ResponsePayload::Echo { data } => w.bytes(data)?,
        ResponsePayload::OverlayAck { pending_keys } => {
            w.u8(pending_keys.len() as u8)?;
            w.bytes(pending_keys)?;
        }
        ResponsePayload::OverlayState { cells } => {
            w.u8(cells.len() as u8)?;
            for cell in cells {
                w.u8(cell.key)?;
                cell.effect.write(&mut w)?;
                w.u32(cell.remaining_ttl_ms)?;
            }
        }
        ResponsePayload::Brightness { level } => w.u8(*level)?,
        ResponsePayload::Toggle { id, state } => {
            w.u8(*id)?;
            w.u8(*state as u8)?;
        }
    }
    let len = w.pos();
    w.patch_u16(3, (len - RESPONSE_HEADER_LEN) as u16);
    Ok(len)
}

/// Decode a complete response message.
pub fn decode_response(bytes: &[u8]) -> Result<Response, DecodeError> {
    let mut r = Reader::new(bytes);
    let opcode = r.u8()?;
    if opcode & RESPONSE_FLAG == 0 {
        return Err(DecodeError::UnknownOpcode(opcode));
    }
    let command = Command::from_opcode(opcode & !RESPONSE_FLAG)
        .ok_or(DecodeError::UnknownOpcode(opcode))?;
    let request_id = r.u8()?;
    let status_byte = r.u8()?;
    let status = Status::from_u8(status_byte).ok_or(DecodeError::UnknownStatus(status_byte))?;
    let payload_len = r.u16()? as usize;
    if r.remaining() != payload_len {
        return Err(DecodeError::LengthMismatch);
    }
    let payload = match status {
        Status::Ok => match command {
            Command::GetCapabilities => ResponsePayload::Capabilities(Capabilities {
                protocol_major: r.u8()?,
                protocol_minor: r.u8()?,
                led_count_left: r.u8()?,
                led_count_right: r.u8()?,
                layer_capacity: r.u8()?,
                max_cells_per_op: r.u8()?,
                effect_mask: r.u16()?,
                overlay_cell_capacity: r.u16()?,
                max_message_len: r.u16()?,
                feature_bits: r.u32()?,
            }),
            Command::Ping => {
                if payload_len > MAX_PING_LEN {
                    return Err(DecodeError::CapacityExceeded);
                }
                let mut data = Vec::new();
                data.extend_from_slice(r.bytes(payload_len)?)
                    .map_err(|_| DecodeError::CapacityExceeded)?;
                ResponsePayload::Echo { data }
            }
            c if c.is_overlay_write() => read_overlay_ack(&mut r)?,
            Command::ReadOverlay => {
                let count = r.u8()? as usize;
                let mut cells = Vec::new();
                for _ in 0..count {
                    let key = r.u8()?;
                    let effect = Effect::read(&mut r)?;
                    let remaining_ttl_ms = r.u32()?;
                    cells
                        .push(CellState { key, effect, remaining_ttl_ms })
                        .map_err(|_| DecodeError::CapacityExceeded)?;
                }
                ResponsePayload::OverlayState { cells }
            }
            Command::GetBrightness | Command::SetBrightness => {
                ResponsePayload::Brightness { level: r.u8()? }
            }
            Command::GetToggle | Command::SetToggle => {
                let id = r.u8()?;
                let state = match r.u8()? {
                    0 => false,
                    1 => true,
                    v => return Err(DecodeError::BadToggleState(v)),
                };
                ResponsePayload::Toggle { id, state }
            }
            Command::EnterBootloader => ResponsePayload::Empty,
            // All commands are covered above; this arm is unreachable.
            _ => return Err(DecodeError::InvalidStatusForCommand),
        },
        Status::PartialApply => {
            if !command.is_overlay_write() {
                return Err(DecodeError::InvalidStatusForCommand);
            }
            read_overlay_ack(&mut r)?
        }
        _ => ResponsePayload::Empty,
    };
    r.finish()?;
    Ok(Response { request_id, command, status, payload })
}

fn read_overlay_ack(r: &mut Reader<'_>) -> Result<ResponsePayload, DecodeError> {
    let count = r.u8()? as usize;
    if count > MAX_CELLS_PER_MESSAGE {
        return Err(DecodeError::CapacityExceeded);
    }
    let mut pending_keys = Vec::new();
    pending_keys
        .extend_from_slice(r.bytes(count)?)
        .map_err(|_| DecodeError::CapacityExceeded)?;
    Ok(ResponsePayload::OverlayAck { pending_keys })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BOOTLOADER_MAGIC, MAX_MESSAGE_LEN};

    fn roundtrip_request(req: Request) {
        let mut buf = [0u8; MAX_MESSAGE_LEN];
        let len = encode_request(0x42, &req, &mut buf).unwrap();
        let (id, decoded) = decode_request(&buf[..len]).unwrap();
        assert_eq!(id, 0x42);
        assert_eq!(decoded, req);
    }

    fn roundtrip_response(resp: Response) {
        let mut buf = [0u8; MAX_MESSAGE_LEN];
        let len = encode_response(&resp, &mut buf).unwrap();
        assert_eq!(decode_response(&buf[..len]).unwrap(), resp);
    }

    fn sample_cells(n: usize) -> Vec<CellWrite, MAX_CELLS_PER_MESSAGE> {
        let mut cells = Vec::new();
        for i in 0..n {
            cells
                .push(CellWrite {
                    key: i as u8,
                    effect: Effect::blink(i as u8, 0x80, 0xFF - i as u8, 500, 100, 50),
                })
                .unwrap();
        }
        cells
    }

    #[test]
    fn requests_roundtrip() {
        roundtrip_request(Request::GetCapabilities { client_major: 1, client_minor: 0 });
        roundtrip_request(Request::Ping { data: Vec::from_slice(&[1, 2, 3]).unwrap() });
        roundtrip_request(Request::Ping { data: Vec::new() });
        roundtrip_request(Request::SetCells { ttl_ms: 12345, cells: sample_cells(3) });
        roundtrip_request(Request::SetCells { ttl_ms: 0, cells: Vec::new() });
        roundtrip_request(Request::UnsetCells { keys: Vec::from_slice(&[0, 40, 79]).unwrap() });
        roundtrip_request(Request::ClearOverlay);
        roundtrip_request(Request::ReadOverlay);
        roundtrip_request(Request::ReplaceOverlay { ttl_ms: 0, cells: sample_cells(80) });
        roundtrip_request(Request::GetBrightness);
        roundtrip_request(Request::SetBrightness { level: 200 });
        roundtrip_request(Request::GetToggle { id: 7 });
        roundtrip_request(Request::SetToggle { id: 7, state: true });
        roundtrip_request(Request::EnterBootloader {
            magic: BOOTLOADER_MAGIC,
            target: BootTarget::Peripheral,
        });
    }

    #[test]
    fn responses_roundtrip() {
        roundtrip_response(Response {
            request_id: 1,
            command: Command::GetCapabilities,
            status: Status::Ok,
            payload: ResponsePayload::Capabilities(Capabilities {
                protocol_major: 1,
                protocol_minor: 0,
                led_count_left: 40,
                led_count_right: 40,
                layer_capacity: 8,
                max_cells_per_op: 80,
                effect_mask: 0b111,
                overlay_cell_capacity: 80,
                max_message_len: 1536,
                feature_bits: 0x3F,
            }),
        });
        roundtrip_response(Response {
            request_id: 2,
            command: Command::Ping,
            status: Status::Ok,
            payload: ResponsePayload::Echo { data: Vec::from_slice(&[9, 8, 7]).unwrap() },
        });
        roundtrip_response(Response {
            request_id: 3,
            command: Command::SetCells,
            status: Status::PartialApply,
            payload: ResponsePayload::OverlayAck {
                pending_keys: Vec::from_slice(&[41, 42]).unwrap(),
            },
        });
        roundtrip_response(Response {
            request_id: 4,
            command: Command::ClearOverlay,
            status: Status::Ok,
            payload: ResponsePayload::OverlayAck { pending_keys: Vec::new() },
        });
        let mut cells = Vec::new();
        cells
            .push(CellState { key: 12, effect: Effect::solid(1, 2, 3), remaining_ttl_ms: 0 })
            .unwrap();
        cells
            .push(CellState {
                key: 60,
                effect: Effect::breathe(0, 0, 255, 3000, 1500),
                remaining_ttl_ms: 4200,
            })
            .unwrap();
        roundtrip_response(Response {
            request_id: 5,
            command: Command::ReadOverlay,
            status: Status::Ok,
            payload: ResponsePayload::OverlayState { cells },
        });
        roundtrip_response(Response {
            request_id: 6,
            command: Command::SetBrightness,
            status: Status::Ok,
            payload: ResponsePayload::Brightness { level: 128 },
        });
        roundtrip_response(Response {
            request_id: 7,
            command: Command::SetToggle,
            status: Status::Ok,
            payload: ResponsePayload::Toggle { id: 2, state: false },
        });
        roundtrip_response(Response {
            request_id: 8,
            command: Command::EnterBootloader,
            status: Status::BadMagic,
            payload: ResponsePayload::Empty,
        });
    }

    #[test]
    fn rejects_length_mismatch() {
        let mut buf = [0u8; 64];
        let len = encode_request(1, &Request::GetBrightness, &mut buf).unwrap();
        // Trailing garbage.
        assert_eq!(decode_request(&buf[..len + 1]), Err(DecodeError::LengthMismatch));
        // Truncated header.
        assert_eq!(decode_request(&buf[..2]), Err(DecodeError::Truncated));
        // Payload_len larger than buffer.
        assert_eq!(
            decode_request(&[0x12u8, 0x01, 0x05, 0x00]),
            Err(DecodeError::LengthMismatch)
        );
        // Inner count disagrees with payload length: says 3 keys, has 1.
        assert_eq!(
            decode_request(&[0x11, 0x01, 0x02, 0x00, 0x03, 0x05]),
            Err(DecodeError::Truncated)
        );
    }

    #[test]
    fn rejects_unknown_discriminants() {
        assert_eq!(decode_request(&[0x77, 0, 0, 0]), Err(DecodeError::UnknownOpcode(0x77)));
        // Response flag missing on a response decode.
        assert_eq!(decode_response(&[0x12, 0, 0, 0, 0]), Err(DecodeError::UnknownOpcode(0x12)));
        assert_eq!(
            decode_response(&[0x92, 0, 0xEE, 0, 0]),
            Err(DecodeError::UnknownStatus(0xEE))
        );
        // Unknown effect kind inside a cell.
        let mut buf = [0u8; 64];
        let mut cells = Vec::new();
        cells.push(CellWrite { key: 0, effect: Effect::solid(0, 0, 0) }).unwrap();
        let len = encode_request(1, &Request::SetCells { ttl_ms: 0, cells }, &mut buf).unwrap();
        buf[10] = 9; // effect kind byte of the first cell (4 header + 4 ttl + 1 count + 1 key)
        assert_eq!(decode_request(&buf[..len]), Err(DecodeError::UnknownEffectKind(9)));
    }

    #[test]
    fn rejects_mismatched_response_payload() {
        let resp = Response {
            request_id: 1,
            command: Command::Ping,
            status: Status::Ok,
            payload: ResponsePayload::Brightness { level: 1 },
        };
        let mut buf = [0u8; 64];
        assert_eq!(encode_response(&resp, &mut buf), Err(EncodeError::PayloadMismatch));
        // Error statuses must carry an empty payload.
        let resp = Response {
            request_id: 1,
            command: Command::Ping,
            status: Status::Busy,
            payload: ResponsePayload::Echo { data: Vec::new() },
        };
        assert_eq!(encode_response(&resp, &mut buf), Err(EncodeError::PayloadMismatch));
        // PartialApply only on overlay writes.
        assert_eq!(
            decode_response(&[0xA0, 1, 0x05, 1, 0, 0]),
            Err(DecodeError::InvalidStatusForCommand)
        );
    }

    #[test]
    fn rejects_small_buffers() {
        let mut buf = [0u8; 3];
        assert_eq!(
            encode_request(1, &Request::GetBrightness, &mut buf),
            Err(EncodeError::BufferTooSmall)
        );
    }

    #[test]
    fn max_batch_fits_in_max_message_len() {
        let mut buf = [0u8; MAX_MESSAGE_LEN];
        let len = encode_request(
            1,
            &Request::ReplaceOverlay { ttl_ms: u32::MAX, cells: sample_cells(80) },
            &mut buf,
        )
        .unwrap();
        assert!(len <= MAX_MESSAGE_LEN);
        // Full read-back with TTLs also fits.
        let mut cells = Vec::new();
        for i in 0..80u8 {
            cells
                .push(CellState {
                    key: i,
                    effect: Effect::solid(i, i, i),
                    remaining_ttl_ms: u32::MAX,
                })
                .unwrap();
        }
        let resp = Response {
            request_id: 1,
            command: Command::ReadOverlay,
            status: Status::Ok,
            payload: ResponsePayload::OverlayState { cells },
        };
        let len = encode_response(&resp, &mut buf).unwrap();
        assert!(len <= MAX_MESSAGE_LEN);
    }
}
