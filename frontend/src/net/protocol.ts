export const PROTOCOL_VERSION = 1;
export const MAX_FRAME_BYTES = 1 << 16;

export const enum IntentKind {
  Move = 1,
  PlaceBelt = 2,
  PlaceInserter = 3,
  PlaceMachine = 4,
  Remove = 5,
  Mine = 6,
  Collect = 7,
  Research = 8,
}

const INTENT_KINDS: readonly IntentKind[] = [
  IntentKind.Move,
  IntentKind.PlaceBelt,
  IntentKind.PlaceInserter,
  IntentKind.PlaceMachine,
  IntentKind.Remove,
  IntentKind.Mine,
  IntentKind.Collect,
  IntentKind.Research,
];

export interface Intent {
  readonly kind: IntentKind;
  readonly x: number;
  readonly y: number;
  readonly arg: number;
}

export function intent(kind: IntentKind, x: number, y: number, arg = 0): Intent {
  return { kind, x, y, arg };
}

export type ClientMessage =
  | { readonly tag: "hello"; readonly name: string; readonly version: number }
  | { readonly tag: "intent"; readonly player: number; readonly intent: Intent }
  | { readonly tag: "ping"; readonly stamp: number };

export type ServerMessage =
  | { readonly tag: "hello"; readonly version: number; readonly seed: number }
  | { readonly tag: "snapshot"; readonly tick: number; readonly hash: number }
  | { readonly tag: "event"; readonly tick: number; readonly code: number; readonly a: number; readonly b: number; readonly c: number }
  | { readonly tag: "pong"; readonly stamp: number };

export class ProtocolError extends Error {}

export class Writer {
  private readonly bytes: number[] = [];

  varint(value: number): void {
    let remaining = BigInt(value);
    if (remaining < 0n) {
      throw new ProtocolError(`varint must be non-negative, got ${value}`);
    }
    for (;;) {
      const low = Number(remaining & 0x7fn);
      remaining >>= 7n;
      if (remaining === 0n) {
        this.bytes.push(low);
        return;
      }
      this.bytes.push(low | 0x80);
    }
  }

  u8(value: number): void {
    if (!Number.isInteger(value) || value < 0 || value > 0xff) {
      throw new ProtocolError(`u8 out of range: ${value}`);
    }
    this.bytes.push(value);
  }

  i32(value: number): void {
    if (!Number.isInteger(value) || value < -2147483648 || value > 2147483647) {
      throw new ProtocolError(`i32 out of range: ${value}`);
    }
    this.varint(zigzag(value));
  }

  string(value: string): void {
    const encoded = new TextEncoder().encode(value);
    this.varint(encoded.length);
    for (const byte of encoded) {
      this.bytes.push(byte);
    }
  }

  raw(data: Uint8Array): void {
    for (const byte of data) {
      this.bytes.push(byte);
    }
  }

  finish(): Uint8Array {
    return Uint8Array.from(this.bytes);
  }
}

export class Reader {
  private pos = 0;

  constructor(private readonly bytes: Uint8Array) {}

  get position(): number {
    return this.pos;
  }

  get remaining(): number {
    return this.bytes.length - this.pos;
  }

  u8(): number {
    if (this.pos >= this.bytes.length) {
      throw new ProtocolError("unexpected end of frame");
    }
    const value = this.bytes[this.pos] as number;
    this.pos += 1;
    return value;
  }

  varint(): number {
    let value = 0n;
    let shift = 0n;
    for (;;) {
      const byte = this.u8();
      value |= BigInt(byte & 0x7f) << shift;
      if ((byte & 0x80) === 0) {
        if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
          throw new ProtocolError("varint exceeds safe integer range");
        }
        return Number(value);
      }
      shift += 7n;
      if (shift > 63n) {
        throw new ProtocolError("varint overflow");
      }
    }
  }

  i32(): number {
    return unzigzag(this.varint());
  }

  string(): string {
    const length = this.varint();
    return new TextDecoder().decode(this.take(length));
  }

  take(length: number): Uint8Array {
    if (length < 0 || this.pos + length > this.bytes.length) {
      throw new ProtocolError("unexpected end of frame");
    }
    const slice = this.bytes.subarray(this.pos, this.pos + length);
    this.pos += length;
    return slice;
  }
}

export function zigzag(value: number): number {
  return ((value << 1) ^ (value >> 31)) >>> 0;
}

export function unzigzag(value: number): number {
  return (value >>> 1) ^ -(value & 1);
}

export function encodeIntent(value: Intent): Uint8Array {
  const writer = new Writer();
  writer.u8(value.kind);
  writer.i32(value.x);
  writer.i32(value.y);
  writer.i32(value.arg);
  return writer.finish();
}

export function decodeIntent(bytes: Uint8Array): Intent {
  const reader = new Reader(bytes);
  const kind = reader.u8();
  if (!INTENT_KINDS.includes(kind as IntentKind)) {
    throw new ProtocolError(`bad intent kind: ${kind}`);
  }
  const x = reader.i32();
  const y = reader.i32();
  const arg = reader.i32();
  return { kind: kind as IntentKind, x, y, arg };
}

export function encodeClientMessage(message: ClientMessage): Uint8Array {
  const writer = new Writer();
  switch (message.tag) {
    case "hello":
      writer.u8(1);
      writer.string(message.name);
      writer.varint(message.version);
      break;
    case "intent": {
      writer.u8(2);
      writer.varint(message.player);
      const payload = encodeIntent(message.intent);
      writer.varint(payload.length);
      writer.raw(payload);
      break;
    }
    case "ping":
      writer.u8(3);
      writer.varint(message.stamp);
      break;
  }
  return writer.finish();
}

export function decodeClientMessage(bytes: Uint8Array): ClientMessage {
  const reader = new Reader(bytes);
  switch (reader.u8()) {
    case 1:
      return { tag: "hello", name: reader.string(), version: reader.varint() };
    case 2: {
      const player = reader.varint();
      const length = reader.varint();
      return { tag: "intent", player, intent: decodeIntent(reader.take(length)) };
    }
    case 3:
      return { tag: "ping", stamp: reader.varint() };
    default:
      throw new ProtocolError("bad client tag");
  }
}

export function encodeServerMessage(message: ServerMessage): Uint8Array {
  const writer = new Writer();
  switch (message.tag) {
    case "hello":
      writer.u8(1);
      writer.varint(message.version);
      writer.varint(message.seed);
      break;
    case "snapshot":
      writer.u8(2);
      writer.varint(message.tick);
      writer.varint(message.hash);
      break;
    case "event":
      writer.u8(3);
      writer.varint(message.tick);
      writer.u8(message.code);
      writer.i32(message.a);
      writer.i32(message.b);
      writer.i32(message.c);
      break;
    case "pong":
      writer.u8(4);
      writer.varint(message.stamp);
      break;
  }
  return writer.finish();
}

export function decodeServerMessage(bytes: Uint8Array): ServerMessage {
  const reader = new Reader(bytes);
  switch (reader.u8()) {
    case 1:
      return { tag: "hello", version: reader.varint(), seed: reader.varint() };
    case 2:
      return { tag: "snapshot", tick: reader.varint(), hash: reader.varint() };
    case 3:
      return {
        tag: "event",
        tick: reader.varint(),
        code: reader.u8(),
        a: reader.i32(),
        b: reader.i32(),
        c: reader.i32(),
      };
    case 4:
      return { tag: "pong", stamp: reader.varint() };
    default:
      throw new ProtocolError("bad server tag");
  }
}

export function frame(payload: Uint8Array): Uint8Array {
  const writer = new Writer();
  writer.varint(payload.length);
  writer.raw(payload);
  return writer.finish();
}

export class FrameDecoder {
  private buffer = new Uint8Array(0);

  get pending(): number {
    return this.buffer.length;
  }

  reset(): void {
    this.buffer = new Uint8Array(0);
  }

  push(data: Uint8Array): Uint8Array[] {
    const merged = new Uint8Array(this.buffer.length + data.length);
    merged.set(this.buffer, 0);
    merged.set(data, this.buffer.length);
    this.buffer = merged;

    const frames: Uint8Array[] = [];
    for (;;) {
      const header = peekVarint(this.buffer);
      if (header === null) {
        break;
      }
      const length = header.value;
      if (length > MAX_FRAME_BYTES) {
        throw new ProtocolError("frame too large");
      }
      const total = header.size + length;
      if (this.buffer.length < total) {
        break;
      }
      frames.push(this.buffer.slice(header.size, total));
      this.buffer = this.buffer.slice(total);
    }
    return frames;
  }

  serverMessages(data: Uint8Array): ServerMessage[] {
    return this.push(data).map(decodeServerMessage);
  }

  clientMessages(data: Uint8Array): ClientMessage[] {
    return this.push(data).map(decodeClientMessage);
  }
}

function peekVarint(bytes: Uint8Array): { value: number; size: number } | null {
  let value = 0n;
  let shift = 0n;
  for (let index = 0; index < bytes.length; index += 1) {
    const byte = bytes[index] as number;
    value |= BigInt(byte & 0x7f) << shift;
    if ((byte & 0x80) === 0) {
      if (value > BigInt(MAX_FRAME_BYTES)) {
        return { value: Number.MAX_SAFE_INTEGER, size: index + 1 };
      }
      return { value: Number(value), size: index + 1 };
    }
    shift += 7n;
    if (shift > 63n) {
      return null;
    }
  }
  return null;
}

export function toHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function fromHex(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) {
    throw new ProtocolError(`odd hex length: ${hex.length}`);
  }
  const bytes = new Uint8Array(hex.length / 2);
  for (let index = 0; index < bytes.length; index += 1) {
    const byte = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
    if (Number.isNaN(byte)) {
      throw new ProtocolError(`bad hex at ${index * 2}`);
    }
    bytes[index] = byte;
  }
  return bytes;
}
