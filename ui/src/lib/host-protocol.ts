// Glove80 host protocol v1 — TypeScript codec.
//
// Byte-level spec: protocol/glove80-host-protocol/PROTOCOL.md.
// This mirrors the Rust codec (protocol/glove80-host-protocol); both are
// pinned to the shared golden vectors in
// protocol/vectors/host-protocol-v1.json. All integers little-endian.

export const PROTOCOL_VERSION_MAJOR = 1;
export const PROTOCOL_VERSION_MINOR = 0;
export const RESPONSE_FLAG = 0x80;
export const REQUEST_HEADER_LEN = 4;
export const RESPONSE_HEADER_LEN = 5;
export const MAX_MESSAGE_LEN = 1536;
export const MAX_CELLS_PER_MESSAGE = 80;
export const MAX_PING_LEN = 64;
export const BOOTLOADER_MAGIC = 0xb00710ad;

export type CommandName =
  | "getCapabilities"
  | "ping"
  | "setCells"
  | "unsetCells"
  | "clearOverlay"
  | "readOverlay"
  | "replaceOverlay"
  | "getBrightness"
  | "setBrightness"
  | "getToggle"
  | "setToggle"
  | "enterBootloader";

export const OPCODES: Record<CommandName, number> = {
  getCapabilities: 0x01,
  ping: 0x02,
  setCells: 0x10,
  unsetCells: 0x11,
  clearOverlay: 0x12,
  readOverlay: 0x13,
  replaceOverlay: 0x14,
  getBrightness: 0x20,
  setBrightness: 0x21,
  getToggle: 0x30,
  setToggle: 0x31,
  enterBootloader: 0x7f,
};

const COMMAND_BY_OPCODE = new Map<number, CommandName>(
  (Object.entries(OPCODES) as [CommandName, number][]).map(([name, op]) => [op, name]),
);

const OVERLAY_WRITE_COMMANDS: ReadonlySet<CommandName> = new Set([
  "setCells",
  "unsetCells",
  "clearOverlay",
  "replaceOverlay",
]);

export type StatusName =
  | "ok"
  | "unknownCommand"
  | "malformed"
  | "outOfRange"
  | "capacityExceeded"
  | "partialApply"
  | "busy"
  | "unknownToggle"
  | "badMagic"
  | "unsupportedVersion";

const STATUS_VALUES: Record<StatusName, number> = {
  ok: 0x00,
  unknownCommand: 0x01,
  malformed: 0x02,
  outOfRange: 0x03,
  capacityExceeded: 0x04,
  partialApply: 0x05,
  busy: 0x06,
  unknownToggle: 0x07,
  badMagic: 0x08,
  unsupportedVersion: 0x09,
};

const STATUS_BY_VALUE = new Map<number, StatusName>(
  (Object.entries(STATUS_VALUES) as [StatusName, number][]).map(([name, v]) => [v, name]),
);

export type EffectKind = "solid" | "blink" | "breathe";

const EFFECT_KIND_VALUES: Record<EffectKind, number> = { solid: 0, blink: 1, breathe: 2 };
const EFFECT_KIND_BY_VALUE = new Map<number, EffectKind>(
  (Object.entries(EFFECT_KIND_VALUES) as [EffectKind, number][]).map(([name, v]) => [v, name]),
);

/** Fixed 10-byte effect record. Fields not applicable to `kind` should be 0
 * but round-trip verbatim either way. */
export interface Effect {
  kind: EffectKind;
  r: number;
  g: number;
  b: number;
  periodMs: number;
  phaseMs: number;
  dutyPercent: number;
}

export const EFFECT_ENCODED_LEN = 10;

export interface CellWrite {
  key: number;
  effect: Effect;
}

export interface CellState {
  key: number;
  effect: Effect;
  /** 0 = no TTL on this cell. */
  remainingTtlMs: number;
}

export interface Capabilities {
  protocolMajor: number;
  protocolMinor: number;
  ledCountLeft: number;
  ledCountRight: number;
  layerCapacity: number;
  maxCellsPerOp: number;
  /** Bit n set ⇔ effect kind n supported. */
  effectMask: number;
  overlayCellCapacity: number;
  maxMessageLen: number;
  featureBits: number;
}

export const FEATURE_TTL = 1 << 0;
export const FEATURE_TOGGLES = 1 << 1;
export const FEATURE_BOOTLOADER_ENTRY = 1 << 2;
export const FEATURE_ATOMIC_REPLACE = 1 << 3;
export const FEATURE_OVERLAY_READBACK = 1 << 4;
export const FEATURE_PARTIAL_APPLY = 1 << 5;

export type BootTarget = "central" | "peripheral";

export type Request =
  | { command: "getCapabilities"; clientMajor: number; clientMinor: number }
  | { command: "ping"; data: Uint8Array }
  | { command: "setCells"; ttlMs: number; cells: CellWrite[] }
  | { command: "unsetCells"; keys: number[] }
  | { command: "clearOverlay" }
  | { command: "readOverlay" }
  | { command: "replaceOverlay"; ttlMs: number; cells: CellWrite[] }
  | { command: "getBrightness" }
  | { command: "setBrightness"; level: number }
  | { command: "getToggle"; id: number }
  | { command: "setToggle"; id: number; state: boolean }
  | { command: "enterBootloader"; magic: number; target: BootTarget };

export type ResponsePayload =
  | { type: "empty" }
  | ({ type: "capabilities" } & Capabilities)
  | { type: "echo"; data: Uint8Array }
  | { type: "overlayAck"; pendingKeys: number[] }
  | { type: "overlayState"; cells: CellState[] }
  | { type: "brightness"; level: number }
  | { type: "toggle"; id: number; state: boolean };

export interface Response {
  requestId: number;
  command: CommandName;
  status: StatusName;
  payload: ResponsePayload;
}

export class ProtocolError extends Error {}

// --- little-endian cursor helpers ----------------------------------------

class Writer {
  private buf = new Uint8Array(MAX_MESSAGE_LEN);
  pos = 0;

  private ensure(n: number): void {
    if (this.pos + n > this.buf.length) {
      throw new ProtocolError(`message exceeds MAX_MESSAGE_LEN (${MAX_MESSAGE_LEN})`);
    }
  }

  u8(v: number): void {
    this.ensure(1);
    this.buf[this.pos++] = v & 0xff;
  }

  u16(v: number): void {
    this.u8(v);
    this.u8(v >>> 8);
  }

  u32(v: number): void {
    this.u16(v);
    this.u16(v >>> 16);
  }

  bytes(src: Uint8Array | number[]): void {
    this.ensure(src.length);
    this.buf.set(src instanceof Uint8Array ? src : Uint8Array.from(src), this.pos);
    this.pos += src.length;
  }

  patchU16(at: number, v: number): void {
    this.buf[at] = v & 0xff;
    this.buf[at + 1] = (v >>> 8) & 0xff;
  }

  finish(): Uint8Array {
    return this.buf.slice(0, this.pos);
  }
}

class ReaderCursor {
  pos = 0;
  constructor(private buf: Uint8Array) {}

  get remaining(): number {
    return this.buf.length - this.pos;
  }

  u8(): number {
    if (this.remaining < 1) throw new ProtocolError("message truncated");
    return this.buf[this.pos++];
  }

  u16(): number {
    return this.u8() | (this.u8() << 8);
  }

  u32(): number {
    return (this.u16() | (this.u16() << 16)) >>> 0;
  }

  bytes(n: number): Uint8Array {
    if (this.remaining < n) throw new ProtocolError("message truncated");
    const out = this.buf.slice(this.pos, this.pos + n);
    this.pos += n;
    return out;
  }

  finish(): void {
    if (this.remaining !== 0) {
      throw new ProtocolError("length field disagrees with buffer");
    }
  }
}

// --- effect / cell records ------------------------------------------------

function writeEffect(w: Writer, e: Effect): void {
  w.u8(EFFECT_KIND_VALUES[e.kind]);
  w.u8(e.r);
  w.u8(e.g);
  w.u8(e.b);
  w.u16(e.periodMs);
  w.u16(e.phaseMs);
  w.u8(e.dutyPercent);
  w.u8(0); // reserved
}

function readEffect(r: ReaderCursor): Effect {
  const kindByte = r.u8();
  const kind = EFFECT_KIND_BY_VALUE.get(kindByte);
  if (kind === undefined) throw new ProtocolError(`unknown effect kind ${kindByte}`);
  const effect: Effect = {
    kind,
    r: r.u8(),
    g: r.u8(),
    b: r.u8(),
    periodMs: r.u16(),
    phaseMs: r.u16(),
    dutyPercent: r.u8(),
  };
  r.u8(); // reserved, ignored
  return effect;
}

function writeCells(w: Writer, ttlMs: number, cells: CellWrite[]): void {
  if (cells.length > MAX_CELLS_PER_MESSAGE) {
    throw new ProtocolError(`too many cells (max ${MAX_CELLS_PER_MESSAGE})`);
  }
  w.u32(ttlMs);
  w.u8(cells.length);
  for (const cell of cells) {
    w.u8(cell.key);
    writeEffect(w, cell.effect);
  }
}

function readCells(r: ReaderCursor): { ttlMs: number; cells: CellWrite[] } {
  const ttlMs = r.u32();
  const count = r.u8();
  if (count > MAX_CELLS_PER_MESSAGE) throw new ProtocolError("count exceeds codec capacity");
  const cells: CellWrite[] = [];
  for (let i = 0; i < count; i++) {
    cells.push({ key: r.u8(), effect: readEffect(r) });
  }
  return { ttlMs, cells };
}

// --- request codec --------------------------------------------------------

export function encodeRequest(requestId: number, request: Request): Uint8Array {
  const w = new Writer();
  w.u8(OPCODES[request.command]);
  w.u8(requestId);
  w.u16(0); // payload_len, patched below
  switch (request.command) {
    case "getCapabilities":
      w.u8(request.clientMajor);
      w.u8(request.clientMinor);
      break;
    case "ping":
      if (request.data.length > MAX_PING_LEN) {
        throw new ProtocolError(`ping payload exceeds ${MAX_PING_LEN} bytes`);
      }
      w.bytes(request.data);
      break;
    case "setCells":
    case "replaceOverlay":
      writeCells(w, request.ttlMs, request.cells);
      break;
    case "unsetCells":
      if (request.keys.length > MAX_CELLS_PER_MESSAGE) {
        throw new ProtocolError(`too many keys (max ${MAX_CELLS_PER_MESSAGE})`);
      }
      w.u8(request.keys.length);
      w.bytes(request.keys);
      break;
    case "clearOverlay":
    case "readOverlay":
    case "getBrightness":
      break;
    case "setBrightness":
      w.u8(request.level);
      break;
    case "getToggle":
      w.u8(request.id);
      break;
    case "setToggle":
      w.u8(request.id);
      w.u8(request.state ? 1 : 0);
      break;
    case "enterBootloader":
      w.u32(request.magic);
      w.u8(request.target === "peripheral" ? 1 : 0);
      break;
  }
  w.patchU16(2, w.pos - REQUEST_HEADER_LEN);
  return w.finish();
}

export interface DecodedRequest {
  requestId: number;
  request: Request;
}

export function decodeRequest(bytes: Uint8Array): DecodedRequest {
  const r = new ReaderCursor(bytes);
  const opcode = r.u8();
  const command = COMMAND_BY_OPCODE.get(opcode);
  if (command === undefined) {
    throw new ProtocolError(`unknown opcode 0x${opcode.toString(16)}`);
  }
  const requestId = r.u8();
  const payloadLen = r.u16();
  if (r.remaining !== payloadLen) {
    throw new ProtocolError("length field disagrees with buffer");
  }
  let request: Request;
  switch (command) {
    case "getCapabilities":
      request = { command, clientMajor: r.u8(), clientMinor: r.u8() };
      break;
    case "ping":
      if (payloadLen > MAX_PING_LEN) throw new ProtocolError("ping payload too long");
      request = { command, data: r.bytes(payloadLen) };
      break;
    case "setCells":
    case "replaceOverlay":
      request = { command, ...readCells(r) };
      break;
    case "unsetCells": {
      const count = r.u8();
      if (count > MAX_CELLS_PER_MESSAGE) throw new ProtocolError("count exceeds codec capacity");
      request = { command, keys: [...r.bytes(count)] };
      break;
    }
    case "clearOverlay":
    case "readOverlay":
    case "getBrightness":
      request = { command };
      break;
    case "setBrightness":
      request = { command, level: r.u8() };
      break;
    case "getToggle":
      request = { command, id: r.u8() };
      break;
    case "setToggle": {
      const id = r.u8();
      const stateByte = r.u8();
      if (stateByte > 1) throw new ProtocolError(`toggle state must be 0 or 1, got ${stateByte}`);
      request = { command, id, state: stateByte === 1 };
      break;
    }
    case "enterBootloader": {
      const magic = r.u32();
      const targetByte = r.u8();
      if (targetByte > 1) throw new ProtocolError(`unknown boot target ${targetByte}`);
      request = { command, magic, target: targetByte === 1 ? "peripheral" : "central" };
      break;
    }
  }
  r.finish();
  return { requestId, request };
}

// --- response codec -------------------------------------------------------

function payloadMatches(command: CommandName, status: StatusName, payload: ResponsePayload): boolean {
  if (status === "ok") {
    switch (payload.type) {
      case "capabilities":
        return command === "getCapabilities";
      case "echo":
        return command === "ping";
      case "overlayAck":
        return OVERLAY_WRITE_COMMANDS.has(command);
      case "overlayState":
        return command === "readOverlay";
      case "brightness":
        return command === "getBrightness" || command === "setBrightness";
      case "toggle":
        return command === "getToggle" || command === "setToggle";
      case "empty":
        return command === "enterBootloader";
    }
  }
  if (status === "partialApply") {
    return OVERLAY_WRITE_COMMANDS.has(command) && payload.type === "overlayAck";
  }
  return payload.type === "empty";
}

export function encodeResponse(response: Response): Uint8Array {
  const { command, status, payload } = response;
  if (!payloadMatches(command, status, payload)) {
    throw new ProtocolError("response payload does not match command/status");
  }
  const w = new Writer();
  w.u8(OPCODES[command] | RESPONSE_FLAG);
  w.u8(response.requestId);
  w.u8(STATUS_VALUES[status]);
  w.u16(0); // payload_len, patched below
  switch (payload.type) {
    case "empty":
      break;
    case "capabilities":
      w.u8(payload.protocolMajor);
      w.u8(payload.protocolMinor);
      w.u8(payload.ledCountLeft);
      w.u8(payload.ledCountRight);
      w.u8(payload.layerCapacity);
      w.u8(payload.maxCellsPerOp);
      w.u16(payload.effectMask);
      w.u16(payload.overlayCellCapacity);
      w.u16(payload.maxMessageLen);
      w.u32(payload.featureBits);
      break;
    case "echo":
      if (payload.data.length > MAX_PING_LEN) {
        throw new ProtocolError(`echo payload exceeds ${MAX_PING_LEN} bytes`);
      }
      w.bytes(payload.data);
      break;
    case "overlayAck":
      if (payload.pendingKeys.length > MAX_CELLS_PER_MESSAGE) {
        throw new ProtocolError(`too many pending keys (max ${MAX_CELLS_PER_MESSAGE})`);
      }
      w.u8(payload.pendingKeys.length);
      w.bytes(payload.pendingKeys);
      break;
    case "overlayState":
      if (payload.cells.length > MAX_CELLS_PER_MESSAGE) {
        throw new ProtocolError(`too many cells (max ${MAX_CELLS_PER_MESSAGE})`);
      }
      w.u8(payload.cells.length);
      for (const cell of payload.cells) {
        w.u8(cell.key);
        writeEffect(w, cell.effect);
        w.u32(cell.remainingTtlMs);
      }
      break;
    case "brightness":
      w.u8(payload.level);
      break;
    case "toggle":
      w.u8(payload.id);
      w.u8(payload.state ? 1 : 0);
      break;
  }
  w.patchU16(3, w.pos - RESPONSE_HEADER_LEN);
  return w.finish();
}

export function decodeResponse(bytes: Uint8Array): Response {
  const r = new ReaderCursor(bytes);
  const opcode = r.u8();
  if ((opcode & RESPONSE_FLAG) === 0) {
    throw new ProtocolError(`not a response opcode 0x${opcode.toString(16)}`);
  }
  const command = COMMAND_BY_OPCODE.get(opcode & ~RESPONSE_FLAG);
  if (command === undefined) {
    throw new ProtocolError(`unknown opcode 0x${opcode.toString(16)}`);
  }
  const requestId = r.u8();
  const statusByte = r.u8();
  const status = STATUS_BY_VALUE.get(statusByte);
  if (status === undefined) {
    throw new ProtocolError(`unknown status 0x${statusByte.toString(16)}`);
  }
  const payloadLen = r.u16();
  if (r.remaining !== payloadLen) {
    throw new ProtocolError("length field disagrees with buffer");
  }
  let payload: ResponsePayload;
  if (status === "ok") {
    switch (command) {
      case "getCapabilities":
        payload = {
          type: "capabilities",
          protocolMajor: r.u8(),
          protocolMinor: r.u8(),
          ledCountLeft: r.u8(),
          ledCountRight: r.u8(),
          layerCapacity: r.u8(),
          maxCellsPerOp: r.u8(),
          effectMask: r.u16(),
          overlayCellCapacity: r.u16(),
          maxMessageLen: r.u16(),
          featureBits: r.u32(),
        };
        break;
      case "ping":
        if (payloadLen > MAX_PING_LEN) throw new ProtocolError("echo payload too long");
        payload = { type: "echo", data: r.bytes(payloadLen) };
        break;
      case "setCells":
      case "unsetCells":
      case "clearOverlay":
      case "replaceOverlay":
        payload = readOverlayAck(r);
        break;
      case "readOverlay": {
        const count = r.u8();
        if (count > MAX_CELLS_PER_MESSAGE) throw new ProtocolError("count exceeds codec capacity");
        const cells: CellState[] = [];
        for (let i = 0; i < count; i++) {
          cells.push({ key: r.u8(), effect: readEffect(r), remainingTtlMs: r.u32() });
        }
        payload = { type: "overlayState", cells };
        break;
      }
      case "getBrightness":
      case "setBrightness":
        payload = { type: "brightness", level: r.u8() };
        break;
      case "getToggle":
      case "setToggle": {
        const id = r.u8();
        const stateByte = r.u8();
        if (stateByte > 1) throw new ProtocolError(`toggle state must be 0 or 1, got ${stateByte}`);
        payload = { type: "toggle", id, state: stateByte === 1 };
        break;
      }
      case "enterBootloader":
        payload = { type: "empty" };
        break;
    }
  } else if (status === "partialApply") {
    if (!OVERLAY_WRITE_COMMANDS.has(command)) {
      throw new ProtocolError("partialApply is only valid on overlay writes");
    }
    payload = readOverlayAck(r);
  } else {
    payload = { type: "empty" };
  }
  r.finish();
  return { requestId, command, status, payload };
}

function readOverlayAck(r: ReaderCursor): ResponsePayload {
  const count = r.u8();
  if (count > MAX_CELLS_PER_MESSAGE) throw new ProtocolError("count exceeds codec capacity");
  return { type: "overlayAck", pendingKeys: [...r.bytes(count)] };
}

// --- frame layer (per-transport segmentation) -----------------------------

export const FRAME_HEADER_LEN = 2;
export const FRAME_FINAL_FLAG = 0x80;
export const FRAME_SEQ_MASK = 0x7f;
export const MAX_FRAMES_PER_MESSAGE = 128;
export const MIN_CHUNK_LEN = FRAME_HEADER_LEN + 1;

function payloadPerFrame(chunkLen: number): number {
  if (chunkLen < MIN_CHUNK_LEN) throw new ProtocolError("chunk size below minimum (3)");
  return Math.min(chunkLen - FRAME_HEADER_LEN, 255);
}

/**
 * Split an encoded message into transport chunks. With `pad`, every frame is
 * zero-padded to `chunkLen` (USB HID fixed-size reports); without, frames
 * are exactly header + payload (BLE GATT writes).
 */
export function splitFrames(message: Uint8Array, chunkLen: number, pad = false): Uint8Array[] {
  if (message.length === 0) throw new ProtocolError("cannot frame an empty message");
  const per = payloadPerFrame(chunkLen);
  const count = Math.ceil(message.length / per);
  if (count > MAX_FRAMES_PER_MESSAGE) {
    throw new ProtocolError(`message exceeds ${MAX_FRAMES_PER_MESSAGE} frames`);
  }
  const frames: Uint8Array[] = [];
  for (let i = 0; i < count; i++) {
    const payload = message.subarray(i * per, Math.min((i + 1) * per, message.length));
    const frame = new Uint8Array(pad ? chunkLen : FRAME_HEADER_LEN + payload.length);
    frame[0] = i === count - 1 ? i | FRAME_FINAL_FLAG : i;
    frame[1] = payload.length;
    frame.set(payload, FRAME_HEADER_LEN);
    frames.push(frame);
  }
  return frames;
}

/**
 * Reassembles one message at a time. A frame with sequence 0 always starts a
 * new message (dropping an incomplete one); errors reset the reassembler.
 */
export class Reassembler {
  private chunks: Uint8Array[] = [];
  private length = 0;
  private nextSeq = 0;

  reset(): void {
    this.chunks = [];
    this.length = 0;
    this.nextSeq = 0;
  }

  /** Feed one received chunk (padding beyond the declared payload length is
   * ignored). Returns the complete message on the FINAL frame, else null. */
  push(frame: Uint8Array): Uint8Array | null {
    if (frame.length < FRAME_HEADER_LEN) {
      this.reset();
      throw new ProtocolError("frame shorter than its header");
    }
    const control = frame[0];
    const seq = control & FRAME_SEQ_MASK;
    const isFinal = (control & FRAME_FINAL_FLAG) !== 0;
    const payloadLen = frame[1];
    if (payloadLen === 0) {
      this.reset();
      throw new ProtocolError("frame has zero-length payload");
    }
    if (frame.length < FRAME_HEADER_LEN + payloadLen) {
      this.reset();
      throw new ProtocolError("frame shorter than declared payload");
    }
    if (seq === 0) {
      this.chunks = [];
      this.length = 0;
    } else if (seq !== this.nextSeq) {
      const expected = this.nextSeq;
      this.reset();
      throw new ProtocolError(`expected frame sequence ${expected}, got ${seq}`);
    }
    if (this.length + payloadLen > MAX_MESSAGE_LEN) {
      this.reset();
      throw new ProtocolError("reassembled message exceeds MAX_MESSAGE_LEN");
    }
    this.chunks.push(frame.slice(FRAME_HEADER_LEN, FRAME_HEADER_LEN + payloadLen));
    this.length += payloadLen;
    if (isFinal) {
      const message = new Uint8Array(this.length);
      let at = 0;
      for (const chunk of this.chunks) {
        message.set(chunk, at);
        at += chunk.length;
      }
      this.reset();
      return message;
    }
    if (seq === MAX_FRAMES_PER_MESSAGE - 1) {
      this.reset();
      throw new ProtocolError(`message exceeds ${MAX_FRAMES_PER_MESSAGE} frames`);
    }
    this.nextSeq = seq + 1;
    return null;
  }
}
