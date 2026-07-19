// WebHID transport for the Glove80 host protocol.
//
// PROTOCOL.md "USB raw HID": a dedicated vendor raw-HID interface on the
// composite device (VID 0x16C0, PID 0x27DB), matched by usage page 0xFF88 /
// usage 0x01 — NOT Vial's 0xFF60/0x61 interface, whose opcode space collides.
// Reports are 32 bytes each way, no report IDs, zero-padded frames.

import type { Transport } from "./transport";

export const GLOVE80_VID = 0x16c0;
export const GLOVE80_PID = 0x27db;
export const HOST_PROTOCOL_USAGE_PAGE = 0xff88;
export const HOST_PROTOCOL_USAGE = 0x01;
export const HID_CHUNK_LEN = 32;

export function webHidSupported(): boolean {
  return typeof navigator !== "undefined" && "hid" in navigator;
}

class WebHidTransport implements Transport {
  readonly kind = "usb" as const;
  readonly chunkSize = HID_CHUNK_LEN;
  readonly pad = true;
  readonly label: string;

  private chunkHandler: ((chunk: Uint8Array) => void) | null = null;
  private disconnectHandler: (() => void) | null = null;
  private readonly onGlobalDisconnect: (ev: { device: HIDDevice }) => void;

  constructor(private readonly device: HIDDevice) {
    this.label = device.productName || "Glove80";
    device.oninputreport = (event) => {
      // The report is one frame-layer chunk; padding is ignored by the
      // reassembler via the frame's own payload-length byte.
      const bytes = new Uint8Array(event.data.buffer, event.data.byteOffset, event.data.byteLength);
      this.chunkHandler?.(bytes.slice());
    };
    this.onGlobalDisconnect = ({ device: gone }) => {
      if (gone === this.device) this.disconnectHandler?.();
    };
    navigator.hid.addEventListener("disconnect", this.onGlobalDisconnect);
  }

  async sendChunk(chunk: Uint8Array): Promise<void> {
    // splitFrames(pad=true) already zero-padded to 32 bytes; no report ID.
    // Copy into a fresh ArrayBuffer-backed view: WebHID's BufferSource type
    // rejects Uint8Array<ArrayBufferLike>.
    await this.device.sendReport(0, new Uint8Array(chunk));
  }

  onChunk(handler: (chunk: Uint8Array) => void): void {
    this.chunkHandler = handler;
  }

  onDisconnect(handler: () => void): void {
    this.disconnectHandler = handler;
  }

  async close(): Promise<void> {
    navigator.hid.removeEventListener("disconnect", this.onGlobalDisconnect);
    this.device.oninputreport = null;
    await this.device.close().catch(() => undefined);
  }
}

function isHostProtocolInterface(device: HIDDevice): boolean {
  return device.collections.some(
    (c) => c.usagePage === HOST_PROTOCOL_USAGE_PAGE && c.usage === HOST_PROTOCOL_USAGE,
  );
}

/** Prompt for the keyboard and open its host-protocol HID interface. */
export async function connectWebHid(): Promise<Transport> {
  const devices = await navigator.hid.requestDevice({
    filters: [
      {
        vendorId: GLOVE80_VID,
        productId: GLOVE80_PID,
        usagePage: HOST_PROTOCOL_USAGE_PAGE,
        usage: HOST_PROTOCOL_USAGE,
      },
    ],
  });
  // Chromium returns every HID interface of the chosen physical device; pick
  // the host-protocol collection, never Vial's look-alike raw interface.
  const device = devices.find(isHostProtocolInterface) ?? devices[0];
  if (!device) throw new Error("No keyboard chosen");
  if (!isHostProtocolInterface(device)) {
    throw new Error("Chosen device has no host-protocol interface (usage page 0xFF88)");
  }
  if (!device.opened) await device.open();
  return new WebHidTransport(device);
}
