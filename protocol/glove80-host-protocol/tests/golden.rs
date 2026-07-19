//! Golden vector suite.
//!
//! The vector file `protocol/vectors/host-protocol-v1.json` is *generated*
//! from the message constructions in this test (run with
//! `GLOVE80_WRITE_VECTORS=1 cargo test --test golden` to regenerate) and
//! consumed by both this suite and the TypeScript suite
//! (`ui/src/lib/host-protocol.test.ts`), so the two codecs cannot drift.

use std::path::PathBuf;

use glove80_host_protocol::frame::write_frame;
use glove80_host_protocol::{
    decode_request, decode_response, encode_request, encode_response, BootTarget, Capabilities,
    CellState, CellWrite, Command, Effect, EffectKind, Request, Response, ResponsePayload, Status,
    BOOTLOADER_MAGIC, MAX_MESSAGE_LEN, PROTOCOL_VERSION_MAJOR, PROTOCOL_VERSION_MINOR,
};
use serde_json::{json, Map, Value};

fn vector_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vectors/host-protocol-v1.json")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    assert!(s.len() % 2 == 0, "odd hex length");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

// --- canonical JSON representation (mirrored by the TS codec/tests) -------

fn command_name(c: Command) -> &'static str {
    match c {
        Command::GetCapabilities => "getCapabilities",
        Command::Ping => "ping",
        Command::SetCells => "setCells",
        Command::UnsetCells => "unsetCells",
        Command::ClearOverlay => "clearOverlay",
        Command::ReadOverlay => "readOverlay",
        Command::ReplaceOverlay => "replaceOverlay",
        Command::GetBrightness => "getBrightness",
        Command::SetBrightness => "setBrightness",
        Command::GetToggle => "getToggle",
        Command::SetToggle => "setToggle",
        Command::EnterBootloader => "enterBootloader",
    }
}

fn status_name(s: Status) -> &'static str {
    match s {
        Status::Ok => "ok",
        Status::UnknownCommand => "unknownCommand",
        Status::Malformed => "malformed",
        Status::OutOfRange => "outOfRange",
        Status::CapacityExceeded => "capacityExceeded",
        Status::PartialApply => "partialApply",
        Status::Busy => "busy",
        Status::UnknownToggle => "unknownToggle",
        Status::BadMagic => "badMagic",
        Status::UnsupportedVersion => "unsupportedVersion",
    }
}

fn effect_json(e: &Effect) -> Value {
    let kind = match e.kind {
        EffectKind::Solid => "solid",
        EffectKind::Blink => "blink",
        EffectKind::Breathe => "breathe",
    };
    json!({
        "kind": kind,
        "r": e.r,
        "g": e.g,
        "b": e.b,
        "periodMs": e.period_ms,
        "phaseMs": e.phase_ms,
        "dutyPercent": e.duty_percent,
    })
}

fn cells_json(cells: &[CellWrite]) -> Value {
    Value::Array(
        cells
            .iter()
            .map(|c| json!({ "key": c.key, "effect": effect_json(&c.effect) }))
            .collect(),
    )
}

fn request_json(req: &Request) -> Value {
    let mut obj = Map::new();
    obj.insert("command".into(), command_name(req.command()).into());
    match req {
        Request::GetCapabilities { client_major, client_minor } => {
            obj.insert("clientMajor".into(), (*client_major).into());
            obj.insert("clientMinor".into(), (*client_minor).into());
        }
        Request::Ping { data } => {
            obj.insert("dataHex".into(), hex(data).into());
        }
        Request::SetCells { ttl_ms, cells } | Request::ReplaceOverlay { ttl_ms, cells } => {
            obj.insert("ttlMs".into(), (*ttl_ms).into());
            obj.insert("cells".into(), cells_json(cells));
        }
        Request::UnsetCells { keys } => {
            obj.insert("keys".into(), Value::Array(keys.iter().map(|k| (*k).into()).collect()));
        }
        Request::ClearOverlay | Request::ReadOverlay | Request::GetBrightness => {}
        Request::SetBrightness { level } => {
            obj.insert("level".into(), (*level).into());
        }
        Request::GetToggle { id } => {
            obj.insert("id".into(), (*id).into());
        }
        Request::SetToggle { id, state } => {
            obj.insert("id".into(), (*id).into());
            obj.insert("state".into(), (*state).into());
        }
        Request::EnterBootloader { magic, target } => {
            obj.insert("magic".into(), (*magic).into());
            obj.insert(
                "target".into(),
                match target {
                    BootTarget::Central => "central",
                    BootTarget::Peripheral => "peripheral",
                }
                .into(),
            );
        }
    }
    Value::Object(obj)
}

fn payload_json(p: &ResponsePayload) -> Value {
    match p {
        ResponsePayload::Empty => json!({ "type": "empty" }),
        ResponsePayload::Capabilities(c) => json!({
            "type": "capabilities",
            "protocolMajor": c.protocol_major,
            "protocolMinor": c.protocol_minor,
            "ledCountLeft": c.led_count_left,
            "ledCountRight": c.led_count_right,
            "layerCapacity": c.layer_capacity,
            "maxCellsPerOp": c.max_cells_per_op,
            "effectMask": c.effect_mask,
            "overlayCellCapacity": c.overlay_cell_capacity,
            "maxMessageLen": c.max_message_len,
            "featureBits": c.feature_bits,
        }),
        ResponsePayload::Echo { data } => json!({ "type": "echo", "dataHex": hex(data) }),
        ResponsePayload::OverlayAck { pending_keys } => json!({
            "type": "overlayAck",
            "pendingKeys": pending_keys.iter().copied().collect::<Vec<u8>>(),
        }),
        ResponsePayload::OverlayState { cells } => json!({
            "type": "overlayState",
            "cells": cells
                .iter()
                .map(|c| json!({
                    "key": c.key,
                    "effect": effect_json(&c.effect),
                    "remainingTtlMs": c.remaining_ttl_ms,
                }))
                .collect::<Vec<Value>>(),
        }),
        ResponsePayload::Brightness { level } => json!({ "type": "brightness", "level": level }),
        ResponsePayload::Toggle { id, state } => {
            json!({ "type": "toggle", "id": id, "state": state })
        }
    }
}

fn response_json(resp: &Response) -> Value {
    json!({
        "command": command_name(resp.command),
        "status": status_name(resp.status),
        "payload": payload_json(&resp.payload),
    })
}

// --- the vectors ----------------------------------------------------------

enum Message {
    Req(u8, Request),
    Resp(Response),
}

fn heapless_bytes<const N: usize>(data: &[u8]) -> heapless::Vec<u8, N> {
    heapless::Vec::from_slice(data).unwrap()
}

fn messages() -> Vec<(&'static str, Message)> {
    use Message::{Req, Resp};

    let blink = Effect::blink(255, 0, 64, 1000, 250, 50);
    let green = Effect::solid(0, 255, 0);
    let breathe = Effect::breathe(16, 32, 48, 3000, 0);

    let two_cells = heapless::Vec::from_slice(&[
        CellWrite { key: 12, effect: blink },
        CellWrite { key: 41, effect: green },
    ])
    .unwrap();
    let one_cell = heapless::Vec::from_slice(&[CellWrite { key: 0, effect: breathe }]).unwrap();

    let caps = Capabilities {
        protocol_major: PROTOCOL_VERSION_MAJOR,
        protocol_minor: PROTOCOL_VERSION_MINOR,
        led_count_left: 40,
        led_count_right: 40,
        layer_capacity: 8,
        max_cells_per_op: 80,
        effect_mask: 0b0000_0111,
        overlay_cell_capacity: 80,
        max_message_len: MAX_MESSAGE_LEN as u16,
        feature_bits: 0x3F,
    };

    vec![
        (
            "get_capabilities_request",
            Req(1, Request::GetCapabilities {
                client_major: PROTOCOL_VERSION_MAJOR,
                client_minor: PROTOCOL_VERSION_MINOR,
            }),
        ),
        (
            "get_capabilities_response",
            Resp(Response {
                request_id: 1,
                command: Command::GetCapabilities,
                status: Status::Ok,
                payload: ResponsePayload::Capabilities(caps),
            }),
        ),
        (
            "ping_request",
            Req(2, Request::Ping { data: heapless_bytes(&[0xDE, 0xAD, 0xBE, 0xEF]) }),
        ),
        (
            "ping_response",
            Resp(Response {
                request_id: 2,
                command: Command::Ping,
                status: Status::Ok,
                payload: ResponsePayload::Echo { data: heapless_bytes(&[0xDE, 0xAD, 0xBE, 0xEF]) },
            }),
        ),
        ("ping_empty_request", Req(255, Request::Ping { data: heapless::Vec::new() })),
        (
            "set_cells_request",
            Req(3, Request::SetCells { ttl_ms: 5000, cells: two_cells.clone() }),
        ),
        (
            "set_cells_response_ok",
            Resp(Response {
                request_id: 3,
                command: Command::SetCells,
                status: Status::Ok,
                payload: ResponsePayload::OverlayAck { pending_keys: heapless::Vec::new() },
            }),
        ),
        (
            "set_cells_response_partial_apply",
            Resp(Response {
                request_id: 4,
                command: Command::SetCells,
                status: Status::PartialApply,
                payload: ResponsePayload::OverlayAck { pending_keys: heapless_bytes(&[41, 42]) },
            }),
        ),
        ("unset_cells_request", Req(5, Request::UnsetCells { keys: heapless_bytes(&[12, 41]) })),
        (
            "unset_cells_response",
            Resp(Response {
                request_id: 5,
                command: Command::UnsetCells,
                status: Status::Ok,
                payload: ResponsePayload::OverlayAck { pending_keys: heapless::Vec::new() },
            }),
        ),
        ("clear_overlay_request", Req(6, Request::ClearOverlay)),
        (
            "clear_overlay_response",
            Resp(Response {
                request_id: 6,
                command: Command::ClearOverlay,
                status: Status::Ok,
                payload: ResponsePayload::OverlayAck { pending_keys: heapless::Vec::new() },
            }),
        ),
        ("read_overlay_request", Req(7, Request::ReadOverlay)),
        (
            "read_overlay_response",
            Resp(Response {
                request_id: 7,
                command: Command::ReadOverlay,
                status: Status::Ok,
                payload: ResponsePayload::OverlayState {
                    cells: heapless::Vec::from_slice(&[
                        CellState { key: 12, effect: blink, remaining_ttl_ms: 4200 },
                        CellState { key: 41, effect: green, remaining_ttl_ms: 0 },
                    ])
                    .unwrap(),
                },
            }),
        ),
        (
            "read_overlay_response_empty",
            Resp(Response {
                request_id: 8,
                command: Command::ReadOverlay,
                status: Status::Ok,
                payload: ResponsePayload::OverlayState { cells: heapless::Vec::new() },
            }),
        ),
        (
            "replace_overlay_request",
            Req(9, Request::ReplaceOverlay { ttl_ms: 0, cells: one_cell }),
        ),
        (
            "replace_overlay_response",
            Resp(Response {
                request_id: 9,
                command: Command::ReplaceOverlay,
                status: Status::Ok,
                payload: ResponsePayload::OverlayAck { pending_keys: heapless::Vec::new() },
            }),
        ),
        ("get_brightness_request", Req(10, Request::GetBrightness)),
        (
            "get_brightness_response",
            Resp(Response {
                request_id: 10,
                command: Command::GetBrightness,
                status: Status::Ok,
                payload: ResponsePayload::Brightness { level: 128 },
            }),
        ),
        ("set_brightness_request", Req(11, Request::SetBrightness { level: 192 })),
        (
            "set_brightness_response",
            Resp(Response {
                request_id: 11,
                command: Command::SetBrightness,
                status: Status::Ok,
                payload: ResponsePayload::Brightness { level: 192 },
            }),
        ),
        ("get_toggle_request", Req(12, Request::GetToggle { id: 2 })),
        (
            "get_toggle_response",
            Resp(Response {
                request_id: 12,
                command: Command::GetToggle,
                status: Status::Ok,
                payload: ResponsePayload::Toggle { id: 2, state: true },
            }),
        ),
        ("set_toggle_request", Req(13, Request::SetToggle { id: 2, state: false })),
        (
            "set_toggle_response",
            Resp(Response {
                request_id: 13,
                command: Command::SetToggle,
                status: Status::Ok,
                payload: ResponsePayload::Toggle { id: 2, state: false },
            }),
        ),
        (
            "set_toggle_response_unknown",
            Resp(Response {
                request_id: 14,
                command: Command::SetToggle,
                status: Status::UnknownToggle,
                payload: ResponsePayload::Empty,
            }),
        ),
        (
            "enter_bootloader_request",
            Req(15, Request::EnterBootloader {
                magic: BOOTLOADER_MAGIC,
                target: BootTarget::Peripheral,
            }),
        ),
        (
            "enter_bootloader_response_ok",
            Resp(Response {
                request_id: 15,
                command: Command::EnterBootloader,
                status: Status::Ok,
                payload: ResponsePayload::Empty,
            }),
        ),
        (
            "enter_bootloader_response_bad_magic",
            Resp(Response {
                request_id: 16,
                command: Command::EnterBootloader,
                status: Status::BadMagic,
                payload: ResponsePayload::Empty,
            }),
        ),
    ]
}

fn encode_message(m: &Message) -> Vec<u8> {
    let mut buf = [0u8; MAX_MESSAGE_LEN];
    let len = match m {
        Message::Req(id, req) => encode_request(*id, req, &mut buf).unwrap(),
        Message::Resp(resp) => encode_response(resp, &mut buf).unwrap(),
    };
    buf[..len].to_vec()
}

fn message_vectors() -> Vec<Value> {
    messages()
        .iter()
        .map(|(name, m)| {
            let bytes = encode_message(m);
            match m {
                Message::Req(id, req) => json!({
                    "name": name,
                    "kind": "request",
                    "requestId": id,
                    "message": request_json(req),
                    "hex": hex(&bytes),
                }),
                Message::Resp(resp) => json!({
                    "name": name,
                    "kind": "response",
                    "requestId": resp.request_id,
                    "message": response_json(resp),
                    "hex": hex(&bytes),
                }),
            }
        })
        .collect()
}

fn frame_vectors() -> Vec<Value> {
    // (name, message-name to frame, chunk size, pad to chunk size?)
    let plans = [
        ("set_cells_frames_hid32_padded", "set_cells_request", 32usize, true),
        ("set_cells_frames_ble20", "set_cells_request", 20, false),
        ("ping_frames_hid32_padded", "ping_request", 32, true),
    ];
    let all = messages();
    plans
        .iter()
        .map(|(name, source, chunk, pad)| {
            let (_, m) = all.iter().find(|(n, _)| n == source).unwrap();
            let message = encode_message(m);
            let count = glove80_host_protocol::frame::frame_count(message.len(), *chunk).unwrap();
            let frames: Vec<String> = (0..count)
                .map(|i| {
                    let mut out = vec![0u8; *chunk];
                    let used = write_frame(&message, *chunk, i, &mut out).unwrap();
                    if !*pad {
                        out.truncate(used);
                    }
                    hex(&out)
                })
                .collect();
            json!({
                "name": name,
                "sourceMessage": source,
                "chunkSize": chunk,
                "padded": pad,
                "messageHex": hex(&message),
                "framesHex": frames,
            })
        })
        .collect()
}

fn golden_doc() -> Value {
    json!({
        "protocol": { "major": PROTOCOL_VERSION_MAJOR, "minor": PROTOCOL_VERSION_MINOR },
        "generatedBy": "protocol/glove80-host-protocol tests/golden.rs (GLOVE80_WRITE_VECTORS=1 cargo test --test golden)",
        "messages": message_vectors(),
        "frames": frame_vectors(),
    })
}

fn load_file() -> Value {
    let path = vector_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

#[test]
fn golden_file_matches_generator() {
    let doc = golden_doc();
    if std::env::var("GLOVE80_WRITE_VECTORS").is_ok() {
        let path = vector_path();
        std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap() + "\n").unwrap();
        return;
    }
    assert_eq!(
        doc,
        load_file(),
        "vector file is stale; regenerate with GLOVE80_WRITE_VECTORS=1 cargo test --test golden"
    );
}

#[test]
fn golden_messages_decode() {
    let file = load_file();
    let constructed = messages();
    for entry in file["messages"].as_array().unwrap() {
        let name = entry["name"].as_str().unwrap();
        let bytes = unhex(entry["hex"].as_str().unwrap());
        let (_, expected) = constructed
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("vector {name} not constructed in this suite"));
        match expected {
            Message::Req(id, req) => {
                let (decoded_id, decoded) = decode_request(&bytes)
                    .unwrap_or_else(|e| panic!("decode {name}: {e:?}"));
                assert_eq!(decoded_id, *id, "{name}");
                assert_eq!(&decoded, req, "{name}");
            }
            Message::Resp(resp) => {
                let decoded =
                    decode_response(&bytes).unwrap_or_else(|e| panic!("decode {name}: {e:?}"));
                assert_eq!(&decoded, resp, "{name}");
            }
        }
    }
}

#[test]
fn golden_frames_reassemble() {
    let file = load_file();
    for entry in file["frames"].as_array().unwrap() {
        let name = entry["name"].as_str().unwrap();
        let message = unhex(entry["messageHex"].as_str().unwrap());
        let mut reassembler: glove80_host_protocol::frame::Reassembler<MAX_MESSAGE_LEN> =
            glove80_host_protocol::frame::Reassembler::new();
        let frames = entry["framesHex"].as_array().unwrap();
        for (i, f) in frames.iter().enumerate() {
            let chunk = unhex(f.as_str().unwrap());
            let out = reassembler.push(&chunk).unwrap_or_else(|e| panic!("push {name}: {e:?}"));
            if i == frames.len() - 1 {
                assert_eq!(out.expect("final frame yields message"), &message[..], "{name}");
            } else {
                assert!(out.is_none(), "{name}: message completed early");
            }
        }
    }
}
