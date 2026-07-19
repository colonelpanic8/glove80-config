//! Transport-independent client for the RMK host protocol.
//!
//! Wraps a [`Transport`] with the `glove80-host-protocol` codec: message
//! encoding, frame segmentation/reassembly, request-id correlation, and
//! capability-driven client-side validation. Capabilities are queried once
//! per connection and cached; nothing is assumed that the device did not
//! advertise (PROTOCOL.md requirement).

use std::fmt;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use glove80_host_protocol::frame::{frame_count, write_frame, Reassembler};
use glove80_host_protocol::{
    encode_request, feature, BootTarget, Capabilities, CellState, CellWrite, EffectKind, Request,
    Response, ResponsePayload, Status, BOOTLOADER_MAGIC, MAX_CELLS_PER_MESSAGE, MAX_MESSAGE_LEN,
    MAX_PING_LEN, PROTOCOL_VERSION_MAJOR, PROTOCOL_VERSION_MINOR,
};

use crate::transport::Transport;

/// Marker error: the device never answered. `ENTER_BOOTLOADER` treats this
/// as success-ish (the device resets before responding).
#[derive(Debug)]
pub struct NoResponse;

impl fmt::Display for NoResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the keyboard did not respond in time")
    }
}

impl std::error::Error for NoResponse {}

/// Result of an overlay write, batches merged.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyOutcome {
    /// At least one batch returned `PARTIAL_APPLY`.
    pub partial: bool,
    /// Keys accepted on the central but still pending on the peripheral.
    pub pending_keys: Vec<u8>,
}

pub fn status_name(status: Status) -> &'static str {
    match status {
        Status::Ok => "OK",
        Status::UnknownCommand => "UNKNOWN_COMMAND (firmware does not understand this opcode)",
        Status::Malformed => "MALFORMED (payload failed to parse)",
        Status::OutOfRange => "OUT_OF_RANGE (key index or value outside advertised capacity)",
        Status::CapacityExceeded => "CAPACITY_EXCEEDED (batch too large or overlay full)",
        Status::PartialApply => "PARTIAL_APPLY",
        Status::Busy => "BUSY (try again)",
        Status::UnknownToggle => "UNKNOWN_TOGGLE (toggle id not configured)",
        Status::BadMagic => "BAD_MAGIC (bootloader entry without the magic constant)",
        Status::UnsupportedVersion => "UNSUPPORTED_VERSION (client protocol major not supported)",
    }
}

fn status_error(operation: &str, status: Status) -> anyhow::Error {
    anyhow!("keyboard rejected {operation}: {}", status_name(status))
}

pub struct HostClient {
    transport: Box<dyn Transport>,
    next_request_id: u8,
    capabilities: Option<Capabilities>,
    reassembler: Reassembler<MAX_MESSAGE_LEN>,
    response_timeout: Duration,
}

impl HostClient {
    pub fn new(transport: Box<dyn Transport>) -> HostClient {
        HostClient {
            transport,
            next_request_id: 1,
            capabilities: None,
            reassembler: Reassembler::new(),
            response_timeout: Duration::from_secs(3),
        }
    }

    pub fn transport_description(&self) -> String {
        self.transport.description()
    }

    /// Send one request and wait for its correlated response. Responses
    /// with a different request id or command are ignored.
    fn call(&mut self, request: &Request) -> Result<Response> {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);

        let mut message = [0u8; MAX_MESSAGE_LEN];
        let length = encode_request(request_id, request, &mut message)
            .context("could not encode the request")?;
        let chunk_len = self.transport.chunk_len();
        let frames = frame_count(length, chunk_len).context("could not frame the request")?;
        let mut chunk = vec![0u8; chunk_len];
        for index in 0..frames {
            chunk.fill(0);
            let used = write_frame(&message[..length], chunk_len, index, &mut chunk)
                .context("could not frame the request")?;
            let outgoing = if self.transport.pads_chunks() { &chunk[..] } else { &chunk[..used] };
            self.transport.send_chunk(outgoing)?;
        }

        self.reassembler.reset();
        let deadline = Instant::now() + self.response_timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(anyhow::Error::new(NoResponse));
            }
            let Some(incoming) = self.transport.recv_chunk(deadline - now)? else {
                return Err(anyhow::Error::new(NoResponse));
            };
            let complete = match self.reassembler.push(&incoming) {
                Ok(Some(message)) => message.to_vec(),
                Ok(None) => continue,
                // Reassembly errors reset the reassembler; treat the chunk
                // as stray traffic and keep waiting.
                Err(_) => continue,
            };
            match glove80_host_protocol::decode_response(&complete) {
                Ok(response)
                    if response.request_id == request_id
                        && response.command == request.command() =>
                {
                    return Ok(response);
                }
                _ => continue,
            }
        }
    }

    /// Cached capability query; sent once per connection, always first.
    pub fn capabilities(&mut self) -> Result<Capabilities> {
        if let Some(capabilities) = self.capabilities {
            return Ok(capabilities);
        }
        let response = self.call(&Request::GetCapabilities {
            client_major: PROTOCOL_VERSION_MAJOR,
            client_minor: PROTOCOL_VERSION_MINOR,
        })?;
        let capabilities = match (response.status, response.payload) {
            (Status::Ok, ResponsePayload::Capabilities(capabilities)) => capabilities,
            (status, _) => return Err(status_error("the capability query", status)),
        };
        if capabilities.protocol_major != PROTOCOL_VERSION_MAJOR {
            bail!(
                "keyboard speaks host protocol v{}.{}, this CLI speaks v{}.{}",
                capabilities.protocol_major,
                capabilities.protocol_minor,
                PROTOCOL_VERSION_MAJOR,
                PROTOCOL_VERSION_MINOR
            );
        }
        self.capabilities = Some(capabilities);
        Ok(capabilities)
    }

    fn require_feature(&mut self, bit: u32, name: &str) -> Result<Capabilities> {
        let capabilities = self.capabilities()?;
        if capabilities.feature_bits & bit == 0 {
            bail!("the keyboard does not advertise {name}");
        }
        Ok(capabilities)
    }

    pub fn key_count(&mut self) -> Result<u16> {
        let capabilities = self.capabilities()?;
        Ok(u16::from(capabilities.led_count_left) + u16::from(capabilities.led_count_right))
    }

    fn validate_keys<'k>(&mut self, keys: impl IntoIterator<Item = &'k u8>) -> Result<()> {
        let key_count = self.key_count()?;
        for &key in keys {
            if u16::from(key) >= key_count {
                bail!("key {key} is out of range (device has keys 0..{})", key_count - 1);
            }
        }
        Ok(())
    }

    fn validate_cells(&mut self, cells: &[CellWrite], ttl_ms: u32) -> Result<()> {
        let capabilities = self.capabilities()?;
        self.validate_keys(cells.iter().map(|cell| &cell.key))?;
        for cell in cells {
            let kind = cell.effect.kind;
            if capabilities.effect_mask & (1 << (kind as u16)) == 0 {
                bail!("the keyboard does not advertise the {} effect", effect_name(kind));
            }
            if cell.effect.duty_percent > 100 {
                bail!("duty must be between 0 and 100 percent");
            }
        }
        if ttl_ms > 0 && capabilities.feature_bits & feature::TTL == 0 {
            bail!("the keyboard does not advertise per-write TTL");
        }
        Ok(())
    }

    fn max_batch(&mut self) -> Result<usize> {
        let capabilities = self.capabilities()?;
        let advertised = capabilities.max_cells_per_op as usize;
        if advertised == 0 {
            bail!("keyboard advertised max_cells_per_op = 0");
        }
        Ok(advertised.min(MAX_CELLS_PER_MESSAGE))
    }

    fn overlay_write(&mut self, operation: &str, request: &Request) -> Result<ApplyOutcome> {
        let response = self.call(request)?;
        match (response.status, response.payload) {
            (Status::Ok, ResponsePayload::OverlayAck { pending_keys }) => Ok(ApplyOutcome {
                partial: false,
                pending_keys: pending_keys.to_vec(),
            }),
            (Status::PartialApply, ResponsePayload::OverlayAck { pending_keys }) => {
                Ok(ApplyOutcome { partial: true, pending_keys: pending_keys.to_vec() })
            }
            (status, _) => Err(status_error(operation, status)),
        }
    }

    pub fn ping(&mut self, data: &[u8]) -> Result<Duration> {
        if data.len() > MAX_PING_LEN {
            bail!("ping payload must be at most {MAX_PING_LEN} bytes");
        }
        self.capabilities()?;
        let payload = heapless::Vec::from_slice(data)
            .map_err(|_| anyhow!("ping payload must be at most {MAX_PING_LEN} bytes"))?;
        let started = Instant::now();
        let response = self.call(&Request::Ping { data: payload })?;
        let elapsed = started.elapsed();
        match (response.status, response.payload) {
            (Status::Ok, ResponsePayload::Echo { data: echoed }) => {
                if echoed.as_slice() != data {
                    bail!("ping echo did not match the sent payload");
                }
                Ok(elapsed)
            }
            (status, _) => Err(status_error("the ping", status)),
        }
    }

    /// SET_CELLS, batched by the advertised `max_cells_per_op`.
    pub fn set_cells(&mut self, ttl_ms: u32, cells: &[CellWrite]) -> Result<ApplyOutcome> {
        self.validate_cells(cells, ttl_ms)?;
        let batch_size = self.max_batch()?;
        let mut outcome = ApplyOutcome::default();
        for batch in cells.chunks(batch_size) {
            let cells = heapless::Vec::from_slice(batch).expect("batch fits codec capacity");
            let partial = self.overlay_write("the cell write", &Request::SetCells { ttl_ms, cells })?;
            outcome.partial |= partial.partial;
            outcome.pending_keys.extend(partial.pending_keys);
        }
        Ok(outcome)
    }

    /// UNSET_CELLS, batched by the advertised `max_cells_per_op`.
    pub fn unset_cells(&mut self, keys: &[u8]) -> Result<ApplyOutcome> {
        self.validate_keys(keys.iter())?;
        let batch_size = self.max_batch()?;
        let mut outcome = ApplyOutcome::default();
        for batch in keys.chunks(batch_size) {
            let keys = heapless::Vec::from_slice(batch).expect("batch fits codec capacity");
            let partial = self.overlay_write("the cell unset", &Request::UnsetCells { keys })?;
            outcome.partial |= partial.partial;
            outcome.pending_keys.extend(partial.pending_keys);
        }
        Ok(outcome)
    }

    pub fn clear_overlay(&mut self) -> Result<ApplyOutcome> {
        self.capabilities()?;
        self.overlay_write("the overlay clear", &Request::ClearOverlay)
    }

    /// REPLACE_OVERLAY: atomic, so never batched — the whole overlay must
    /// fit one operation.
    pub fn replace_overlay(&mut self, ttl_ms: u32, cells: &[CellWrite]) -> Result<ApplyOutcome> {
        let capabilities = self.require_feature(feature::ATOMIC_REPLACE, "atomic replace")?;
        self.validate_cells(cells, ttl_ms)?;
        let batch_size = self.max_batch()?;
        if cells.len() > batch_size {
            bail!(
                "replace is atomic and cannot be batched: {} cells exceed the \
                 advertised limit of {batch_size} per operation",
                cells.len()
            );
        }
        if cells.len() > capabilities.overlay_cell_capacity as usize {
            bail!(
                "{} cells exceed the advertised overlay capacity of {}",
                cells.len(),
                capabilities.overlay_cell_capacity
            );
        }
        let cells = heapless::Vec::from_slice(cells).expect("bounded by max_batch");
        self.overlay_write("the overlay replace", &Request::ReplaceOverlay { ttl_ms, cells })
    }

    pub fn read_overlay(&mut self) -> Result<Vec<CellState>> {
        self.require_feature(feature::OVERLAY_READBACK, "overlay read-back")?;
        let response = self.call(&Request::ReadOverlay)?;
        match (response.status, response.payload) {
            (Status::Ok, ResponsePayload::OverlayState { cells }) => Ok(cells.to_vec()),
            (status, _) => Err(status_error("the overlay read", status)),
        }
    }

    /// GET_BRIGHTNESS / SET_BRIGHTNESS; returns the level now in effect.
    pub fn brightness(&mut self, level: Option<u8>) -> Result<u8> {
        self.capabilities()?;
        let request = match level {
            Some(level) => Request::SetBrightness { level },
            None => Request::GetBrightness,
        };
        let response = self.call(&request)?;
        match (response.status, response.payload) {
            (Status::Ok, ResponsePayload::Brightness { level }) => Ok(level),
            (status, _) => Err(status_error("the brightness request", status)),
        }
    }

    /// GET_TOGGLE / SET_TOGGLE; returns `(id, state)` now in effect.
    pub fn toggle(&mut self, id: u8, state: Option<bool>) -> Result<(u8, bool)> {
        self.require_feature(feature::TOGGLES, "toggles")?;
        let request = match state {
            Some(state) => Request::SetToggle { id, state },
            None => Request::GetToggle { id },
        };
        let response = self.call(&request)?;
        match (response.status, response.payload) {
            (Status::Ok, ResponsePayload::Toggle { id, state }) => Ok((id, state)),
            (status, _) => Err(status_error("the toggle request", status)),
        }
    }

    /// ENTER_BOOTLOADER. Returns `true` if the device acknowledged, `false`
    /// if it reset before answering (which the protocol allows).
    pub fn enter_bootloader(&mut self, target: BootTarget) -> Result<bool> {
        self.require_feature(feature::BOOTLOADER_ENTRY, "programmatic bootloader entry")?;
        let request = Request::EnterBootloader { magic: BOOTLOADER_MAGIC, target };
        match self.call(&request) {
            Ok(response) => match response.status {
                Status::Ok => Ok(true),
                status => Err(status_error("the bootloader request", status)),
            },
            Err(error) if error.is::<NoResponse>() => Ok(false),
            Err(error) => Err(error),
        }
    }
}

pub fn effect_name(kind: EffectKind) -> &'static str {
    match kind {
        EffectKind::Solid => "solid",
        EffectKind::Blink => "blink",
        EffectKind::Breathe => "breathe",
    }
}
