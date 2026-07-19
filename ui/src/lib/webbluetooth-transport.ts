// Web Bluetooth transport for the Glove80 host protocol.
//
// PROTOCOL.md "BLE GATT": a custom primary service (deliberately not HID so
// Web Bluetooth can reach it). It is not advertised, so it must be listed in
// optionalServices. Requests are write-without-response to the request
// characteristic; responses arrive as notifications on the response
// characteristic. The GATT database requires an encrypted (bonded) link, so
// the OS bond to the keyboard must already exist.

import type { Transport } from "./transport";

export const GATT_SERVICE_UUID = "fc550001-f8e0-459f-b421-c254fc42b138";
export const GATT_REQUEST_UUID = "fc550002-f8e0-459f-b421-c254fc42b138";
export const GATT_RESPONSE_UUID = "fc550003-f8e0-459f-b421-c254fc42b138";

// Web Bluetooth exposes no negotiated-MTU query, so outgoing chunks stay at
// the guaranteed ATT floor (23 - 3 = 20 bytes). The keyboard may notify
// larger chunks if the link negotiated more; the reassembler handles any
// size, so only our upstream throughput pays for the conservative floor.
export const BLE_CHUNK_LEN = 20;

export function webBluetoothSupported(): boolean {
  return typeof navigator !== "undefined" && "bluetooth" in navigator;
}

class WebBluetoothTransport implements Transport {
  readonly kind = "ble" as const;
  readonly chunkSize = BLE_CHUNK_LEN;
  readonly pad = false;
  readonly label: string;

  private chunkHandler: ((chunk: Uint8Array) => void) | null = null;
  private disconnectHandler: (() => void) | null = null;
  private closing = false;

  constructor(
    private readonly device: BluetoothDevice,
    private readonly request: BluetoothRemoteGATTCharacteristic,
    private readonly response: BluetoothRemoteGATTCharacteristic,
  ) {
    this.label = device.name || "Glove80 (BLE)";
    response.addEventListener("characteristicvaluechanged", () => {
      const value = this.response.value;
      if (!value) return;
      this.chunkHandler?.(new Uint8Array(value.buffer, value.byteOffset, value.byteLength).slice());
    });
    device.addEventListener("gattserverdisconnected", () => {
      if (!this.closing) this.disconnectHandler?.();
    });
  }

  async sendChunk(chunk: Uint8Array): Promise<void> {
    // One frame-layer chunk per ATT write, unpadded.
    await this.request.writeValueWithoutResponse(
      chunk.buffer.slice(chunk.byteOffset, chunk.byteOffset + chunk.byteLength) as ArrayBuffer,
    );
  }

  onChunk(handler: (chunk: Uint8Array) => void): void {
    this.chunkHandler = handler;
  }

  onDisconnect(handler: () => void): void {
    this.disconnectHandler = handler;
  }

  async close(): Promise<void> {
    this.closing = true;
    try {
      await this.response.stopNotifications();
    } catch {
      // The link may already be gone.
    }
    this.device.gatt?.disconnect();
  }
}

/** Prompt for the keyboard and wire up the fc55 host-protocol service. */
export async function connectWebBluetooth(): Promise<Transport> {
  const device = await navigator.bluetooth.requestDevice({
    // The service is absent from the advertising payload, so it cannot be a
    // filter — match on the device name and claim the service via
    // optionalServices instead.
    filters: [{ namePrefix: "Glove80" }],
    optionalServices: [GATT_SERVICE_UUID],
  });
  const gatt = device.gatt;
  if (!gatt) throw new Error("Chosen device has no GATT server");
  const server = await gatt.connect();
  try {
    const service = await server.getPrimaryService(GATT_SERVICE_UUID);
    const request = await service.getCharacteristic(GATT_REQUEST_UUID);
    const response = await service.getCharacteristic(GATT_RESPONSE_UUID);
    await response.startNotifications(); // subscribes via the CCCD
    return new WebBluetoothTransport(device, request, response);
  } catch (error) {
    gatt.disconnect();
    throw error;
  }
}
