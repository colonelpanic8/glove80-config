// Transport abstraction for the Glove80 host protocol.
//
// A Transport moves raw frame-layer chunks (PROTOCOL.md "Frame layer") in
// both directions. Everything above the chunk level — frame split/reassembly,
// request ids, timeouts — lives in ProtocolClient (protocol-client.ts), so
// WebHID, Web Bluetooth and the in-memory mock all share one code path.

export type TransportKind = "usb" | "ble" | "demo";

export interface Transport {
  readonly kind: TransportKind;
  /** Human-readable device name for the connection readout. */
  readonly label: string;
  /** Chunk size for outgoing frames (32 for USB HID, ATT floor 20 for BLE). */
  readonly chunkSize: number;
  /** Zero-pad outgoing frames to chunkSize (USB HID fixed-size reports). */
  readonly pad: boolean;
  sendChunk(chunk: Uint8Array): Promise<void>;
  /** Register the single receiver for incoming chunks. */
  onChunk(handler: (chunk: Uint8Array) => void): void;
  /** Register the single handler called when the link drops unexpectedly. */
  onDisconnect(handler: () => void): void;
  close(): Promise<void>;
}
