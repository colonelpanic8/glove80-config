// In-memory Glove80 for tests and Lightbench's demo mode.
//
// MockKeyboard implements the device side of the host protocol against the
// same TS codec the app uses: overlay semantics (TTL, replace, partial
// apply), brightness, toggles, and the full v1.1 config transfer session,
// per PROTOCOL.md. MockTransport frames it like the USB HID transport
// (32-byte zero-padded chunks) so the whole client stack is exercised.

import {
  CONFIG_HEADER_LEN,
  crc32,
  decodeLightingConfig,
  decodeRequest,
  encodeLightingConfig,
  encodeResponse,
  FEATURE_ATOMIC_REPLACE,
  FEATURE_BOOTLOADER_ENTRY,
  FEATURE_OVERLAY_READBACK,
  FEATURE_PARTIAL_APPLY,
  FEATURE_PERSISTENT_CONFIG,
  FEATURE_TOGGLES,
  FEATURE_TTL,
  MAX_CONFIG_DATA_PER_MESSAGE,
  BOOTLOADER_MAGIC,
  ProtocolError,
  Reassembler,
  splitFrames,
  type Capabilities,
  type CellState,
  type CellWrite,
  type LightingConfig,
  type Request,
  type Response,
  type ResponsePayload,
  type StatusName,
} from "./host-protocol";
import type { Transport } from "./transport";

const LED_COUNT = 80;
const LEFT_LED_COUNT = 40;

export const MOCK_CAPABILITIES: Capabilities = {
  protocolMajor: 1,
  protocolMinor: 1,
  ledCountLeft: LEFT_LED_COUNT,
  ledCountRight: LED_COUNT - LEFT_LED_COUNT,
  layerCapacity: 8,
  maxCellsPerOp: 40,
  effectMask: 0b111, // solid | blink | breathe
  overlayCellCapacity: 80,
  maxMessageLen: 1536,
  featureBits:
    FEATURE_TTL |
    FEATURE_TOGGLES |
    FEATURE_BOOTLOADER_ENTRY |
    FEATURE_ATOMIC_REPLACE |
    FEATURE_OVERLAY_READBACK |
    FEATURE_PARTIAL_APPLY |
    FEATURE_PERSISTENT_CONFIG,
  maxConfigBlobLen: 7148,
};

interface OverlayCell {
  effect: CellWrite["effect"];
  /** Clock timestamp at which the cell expires; null = no TTL. */
  expiresAt: number | null;
}

interface TransferSession {
  totalLen: number;
  blobCrc32: number;
  received: number;
  buffer: Uint8Array;
}

export interface MockKeyboardOptions {
  /** Injectable clock (ms) so tests can advance TTL time deterministically. */
  now?: () => number;
  capabilities?: Partial<Capabilities>;
  /** Simulate the right half being offline: right-half writes answer
   * PARTIAL_APPLY listing the pending keys. */
  peripheralOffline?: boolean;
  /** Preloaded persisted config (as if committed earlier). */
  initialConfig?: LightingConfig;
}

export class MockKeyboard {
  readonly capabilities: Capabilities;
  private readonly now: () => number;
  peripheralOffline: boolean;

  private overlay = new Map<number, OverlayCell>();
  private brightness = 255;
  private toggles = new Map<number, boolean>();
  private configBlob: Uint8Array | null = null;
  private session: TransferSession | null = null;

  constructor(options: MockKeyboardOptions = {}) {
    this.capabilities = { ...MOCK_CAPABILITIES, ...options.capabilities };
    this.now = options.now ?? (() => Date.now());
    this.peripheralOffline = options.peripheralOffline ?? false;
    if (options.initialConfig) {
      this.configBlob = encodeLightingConfig(options.initialConfig);
      this.adoptConfigToggles(options.initialConfig);
    }
  }

  /** Handle one decoded request; returns the response the device would send. */
  handle(requestId: number, request: Request): Response {
    const respond = (status: StatusName, payload: ResponsePayload = { type: "empty" }): Response => ({
      requestId,
      command: request.command,
      status,
      payload,
    });
    switch (request.command) {
      case "getCapabilities":
        if (request.clientMajor !== this.capabilities.protocolMajor) {
          return respond("unsupportedVersion");
        }
        return respond("ok", { type: "capabilities", ...this.capabilities });
      case "ping":
        return respond("ok", { type: "echo", data: request.data });
      case "setCells":
      case "replaceOverlay": {
        if (request.cells.length > this.capabilities.maxCellsPerOp) {
          return respond("capacityExceeded");
        }
        for (const cell of request.cells) {
          if (cell.key >= LED_COUNT) return respond("outOfRange");
        }
        const next =
          request.command === "replaceOverlay"
            ? new Map<number, OverlayCell>()
            : new Map(this.overlay);
        const expiresAt = request.ttlMs > 0 ? this.now() + request.ttlMs : null;
        for (const cell of request.cells) next.set(cell.key, { effect: cell.effect, expiresAt });
        if (next.size > this.capabilities.overlayCellCapacity) return respond("capacityExceeded");
        this.overlay = next;
        return this.overlayAck(
          respond,
          request.cells.map((cell) => cell.key),
        );
      }
      case "unsetCells": {
        if (request.keys.length > this.capabilities.maxCellsPerOp) {
          return respond("capacityExceeded");
        }
        for (const key of request.keys) {
          if (key >= LED_COUNT) return respond("outOfRange");
        }
        for (const key of request.keys) this.overlay.delete(key);
        return this.overlayAck(respond, request.keys);
      }
      case "clearOverlay": {
        const hadRightCells = [...this.overlay.keys()].some((key) => key >= LEFT_LED_COUNT);
        this.overlay.clear();
        if (this.peripheralOffline && hadRightCells) {
          // A pending clear on the offline half: PARTIAL_APPLY, no key list.
          return respond("partialApply", { type: "overlayAck", pendingKeys: [] });
        }
        return respond("ok", { type: "overlayAck", pendingKeys: [] });
      }
      case "readOverlay": {
        this.pruneExpired();
        const cells: CellState[] = [...this.overlay.entries()]
          .sort(([a], [b]) => a - b)
          .map(([key, cell]) => ({
            key,
            effect: cell.effect,
            remainingTtlMs:
              cell.expiresAt === null ? 0 : Math.max(1, cell.expiresAt - this.now()),
          }));
        return respond("ok", { type: "overlayState", cells });
      }
      case "getBrightness":
        return respond("ok", { type: "brightness", level: this.brightness });
      case "setBrightness":
        this.brightness = request.level;
        return respond("ok", { type: "brightness", level: this.brightness });
      case "getToggle": {
        const state = this.toggles.get(request.id);
        if (state === undefined) return respond("unknownToggle");
        return respond("ok", { type: "toggle", id: request.id, state });
      }
      case "setToggle": {
        if (!this.toggles.has(request.id)) return respond("unknownToggle");
        this.toggles.set(request.id, request.state);
        return respond("ok", { type: "toggle", id: request.id, state: request.state });
      }
      case "configBegin": {
        if (request.totalLen > this.capabilities.maxConfigBlobLen) {
          this.session = null;
          return respond("capacityExceeded");
        }
        this.session = {
          totalLen: request.totalLen,
          blobCrc32: request.blobCrc32,
          received: 0,
          buffer: new Uint8Array(request.totalLen),
        };
        return respond("ok");
      }
      case "configData": {
        const session = this.session;
        if (!session) return respond("noSession");
        if (
          request.offset !== session.received ||
          request.offset + request.data.length > session.totalLen
        ) {
          this.session = null; // a bad offset aborts the session
          return respond("badOffset");
        }
        session.buffer.set(request.data, request.offset);
        session.received += request.data.length;
        return respond("ok");
      }
      case "configCommit": {
        const session = this.session;
        this.session = null; // every commit, success or failure, ends the session
        if (!session) return respond("noSession");
        if (session.received < session.totalLen) return respond("configIncomplete");
        const blob = session.buffer;
        if (crc32(blob) !== session.blobCrc32) return respond("crcMismatch");
        if (blob.length < CONFIG_HEADER_LEN) return respond("invalidConfig");
        const view = new DataView(blob.buffer, blob.byteOffset, blob.byteLength);
        const bodyCrc = view.getUint32(12, true);
        if (crc32(blob.subarray(CONFIG_HEADER_LEN)) !== bodyCrc) return respond("crcMismatch");
        let config: LightingConfig;
        try {
          config = decodeLightingConfig(blob);
        } catch {
          return respond("invalidConfig");
        }
        this.configBlob = blob.slice(); // atomically activate + persist
        this.adoptConfigToggles(config);
        return respond("ok");
      }
      case "configAbort":
        this.session = null; // idempotent
        return respond("ok");
      case "configRead": {
        const blob = this.configBlob;
        const totalLen = blob?.length ?? 0;
        if (request.offset > totalLen) return respond("outOfRange");
        const maxLen = Math.min(request.maxLen, MAX_CONFIG_DATA_PER_MESSAGE);
        const data = blob
          ? blob.slice(request.offset, Math.min(request.offset + maxLen, totalLen))
          : new Uint8Array(0);
        return respond("ok", { type: "configData", totalLen, data });
      }
      case "enterBootloader":
        if (request.magic !== BOOTLOADER_MAGIC) return respond("badMagic");
        if (request.target === "peripheral" && this.peripheralOffline) return respond("busy");
        return respond("ok");
    }
  }

  /** The active (committed) blob, byte-stable; null when none is stored. */
  activeConfigBlob(): Uint8Array | null {
    return this.configBlob ? this.configBlob.slice() : null;
  }

  overlaySize(): number {
    this.pruneExpired();
    return this.overlay.size;
  }

  private adoptConfigToggles(config: LightingConfig): void {
    const next = new Map<number, boolean>();
    for (const record of config.records) {
      if (record.activation.kind === "toggle") {
        const id = record.activation.id;
        const persisted = (config.togglePersistMask & (1 << id)) !== 0 && this.toggles.has(id);
        next.set(
          id,
          persisted
            ? (this.toggles.get(id) as boolean)
            : (config.toggleInitialState & (1 << id)) !== 0,
        );
      }
    }
    this.toggles = next;
  }

  private pruneExpired(): void {
    const now = this.now();
    for (const [key, cell] of this.overlay) {
      if (cell.expiresAt !== null && cell.expiresAt <= now) this.overlay.delete(key);
    }
  }

  private overlayAck(
    respond: (status: StatusName, payload?: ResponsePayload) => Response,
    writtenKeys: number[],
  ): Response {
    if (this.peripheralOffline) {
      const pendingKeys = [...new Set(writtenKeys.filter((key) => key >= LEFT_LED_COUNT))].sort(
        (a, b) => a - b,
      );
      if (pendingKeys.length > 0) {
        return respond("partialApply", { type: "overlayAck", pendingKeys });
      }
    }
    return respond("ok", { type: "overlayAck", pendingKeys: [] });
  }
}

/**
 * Transport adapter over a MockKeyboard: frames like USB HID (32-byte
 * zero-padded chunks), delivers responses asynchronously.
 */
export class MockTransport implements Transport {
  readonly kind = "demo" as const;
  readonly label = "Demo keyboard";
  readonly chunkSize = 32;
  readonly pad = true;

  private reassembler = new Reassembler();
  private chunkHandler: ((chunk: Uint8Array) => void) | null = null;
  private disconnectHandler: (() => void) | null = null;
  private closed = false;

  constructor(readonly keyboard: MockKeyboard) {}

  async sendChunk(chunk: Uint8Array): Promise<void> {
    if (this.closed) throw new ProtocolError("demo transport closed");
    const message = this.reassembler.push(chunk);
    if (message === null) return;
    const { requestId, request } = decodeRequest(message);
    const response = encodeResponse(this.keyboard.handle(requestId, request));
    const frames = splitFrames(response, this.chunkSize, this.pad);
    queueMicrotask(() => {
      if (this.closed) return;
      for (const frame of frames) this.chunkHandler?.(frame);
    });
  }

  onChunk(handler: (chunk: Uint8Array) => void): void {
    this.chunkHandler = handler;
  }

  onDisconnect(handler: () => void): void {
    this.disconnectHandler = handler;
  }

  /** Simulate the keyboard dropping the link (for tests). */
  simulateDisconnect(): void {
    this.closed = true;
    this.disconnectHandler?.();
  }

  async close(): Promise<void> {
    this.closed = true;
  }
}

/** A demo keyboard preloaded with a small, presentable persistent config. */
export function createDemoKeyboard(): MockKeyboard {
  const teal = { kind: "solid" as const, r: 0x2b, g: 0xd4, b: 0xc0, periodMs: 0, phaseMs: 0, dutyPercent: 0 };
  const amber = { kind: "breathe" as const, r: 0xf5, g: 0xa5, b: 0x24, periodMs: 2400, phaseMs: 0, dutyPercent: 0 };
  const red = { kind: "blink" as const, r: 0xf0, g: 0x5d, b: 0x3e, periodMs: 900, phaseMs: 0, dutyPercent: 40 };
  return new MockKeyboard({
    initialConfig: {
      togglePersistMask: 1 << 2,
      toggleInitialState: 1 << 2,
      records: [
        {
          activation: { kind: "always" },
          cells: [0, 1, 2, 3, 4, 5, 40, 41, 42, 43, 44, 45].map((key) => ({ key, effect: teal })),
        },
        {
          activation: { kind: "layerActive", layer: 1 },
          cells: [10, 16, 22, 50, 56, 62].map((key) => ({ key, effect: amber })),
        },
        {
          activation: { kind: "toggle", id: 2 },
          cells: [34, 74].map((key) => ({ key, effect: red })),
        },
      ],
    },
  });
}
