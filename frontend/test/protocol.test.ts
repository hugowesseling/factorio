import vectors from "./vectors.json";

import {
  decodeIntent,
  decodeServerMessage,
  encodeClientMessage,
  encodeIntent,
  encodeServerMessage,
  FrameDecoder,
  fromHex,
  IntentKind,
  toHex,
  unzigzag,
  zigzag,
  type ClientMessage,
  type ServerMessage,
} from "../src/net/protocol";

const protocolVectors = vectors.protocol as Record<string, string>;

function framed(payload: Uint8Array): Uint8Array {
  return Uint8Array.from([payload.length, ...payload]);
}

describe("primitive encodings", () => {
  it("zigzags signed values the way the backend does", () => {
    const values = [0, -1, 1, -2147483648, 2147483647, -12345];
    for (const value of values) {
      expect(unzigzag(zigzag(value))).toBe(value);
    }
    expect(zigzag(1)).toBe(2);
    expect(zigzag(-1)).toBe(1);
    expect(zigzag(-412)).toBe(823);
  });

  it("round-trips varints of every magnitude", () => {
    const values = [0, 1, 127, 128, 300, 4294967295, Number.MAX_SAFE_INTEGER];
    for (const value of values) {
      const encoded = encodeServerMessage({ tag: "pong", stamp: value });
      const [message] = new FrameDecoder().serverMessages(framed(encoded));
      expect(message).toEqual({ tag: "pong", stamp: value });
    }
  });
});

describe("byte-for-byte parity with the Rust encoder", () => {
  const clientCases: [string, ClientMessage][] = [
    ["hello", { tag: "hello", name: "hugo", version: 1 }],
    ["intent_move", { tag: "intent", player: 3, intent: { kind: IntentKind.Move, x: 1, y: -1, arg: 0 } }],
    [
      "intent_place_machine",
      { tag: "intent", player: 0, intent: { kind: IntentKind.PlaceMachine, x: -412, y: 88, arg: 2 } },
    ],
    ["ping", { tag: "ping", stamp: 987654321 }],
  ];

  for (const [name, message] of clientCases) {
    it(`encodes ${name} identically`, () => {
      expect(toHex(encodeClientMessage(message))).toBe(protocolVectors[name]);
    });

    it(`decodes ${name} back to the same value`, () => {
      const [decoded] = new FrameDecoder().clientMessages(framed(encodeClientMessage(message)));
      expect(decoded).toEqual(message);
    });
  }

  const serverCases: [string, ServerMessage][] = [
    ["server_hello", { tag: "hello", version: 1, seed: 42 }],
    ["server_snapshot", { tag: "snapshot", tick: 600, hash: 0xdeadbeef }],
    ["server_event", { tag: "event", tick: 7, code: 3, a: -1, b: 0, c: 9, d: -42 }],
    ["server_pong", { tag: "pong", stamp: 12 }],
  ];

  for (const [name, message] of serverCases) {
    it(`encodes ${name} identically`, () => {
      expect(toHex(encodeServerMessage(message))).toBe(protocolVectors[name]);
    });

    it(`decodes ${name} back to the same value`, () => {
      const [decoded] = new FrameDecoder().serverMessages(framed(encodeServerMessage(message)));
      expect(decoded).toEqual(message);
    });
  }

  it("encodes a bare intent identically", () => {
    const payload = encodeIntent({ kind: IntentKind.PlaceMachine, x: -412, y: 88, arg: 2 });
    expect(toHex(payload)).toBe("04b706b00104");
    expect(decodeIntent(payload)).toEqual({
      kind: IntentKind.PlaceMachine,
      x: -412,
      y: 88,
      arg: 2,
    });
  });
});

describe("framing", () => {
  it("length-prefixes a frame exactly as Rust does", () => {
    expect(toHex(framed(encodeClientMessage({ tag: "ping", stamp: 5 })))).toBe(vectors.framedPing);
  });

  it("splits a coalesced client stream into the frames Rust produced", () => {
    const messages = new FrameDecoder().clientMessages(fromHex(vectors.splitFrames));
    expect(messages).toEqual([
      {
        tag: "intent",
        player: 1,
        intent: { kind: IntentKind.PlaceBelt, x: 4, y: 4, arg: 0 },
      },
      { tag: "ping", stamp: 9 },
    ]);
  });

  it("reassembles a frame delivered one byte at a time", () => {
    const stream = framed(encodeServerMessage({ tag: "snapshot", tick: 600, hash: 0xdeadbeef }));
    const decoder = new FrameDecoder();
    const collected: ServerMessage[] = [];
    for (const byte of stream) {
      collected.push(...decoder.serverMessages(Uint8Array.from([byte])));
    }
    expect(collected).toEqual([{ tag: "snapshot", tick: 600, hash: 0xdeadbeef }]);
    expect(decoder.pending).toBe(0);
  });

  it("handles several frames in a single read", () => {
    const first = framed(encodeServerMessage({ tag: "pong", stamp: 1 }));
    const second = framed(encodeServerMessage({ tag: "pong", stamp: 2 }));
    const stream = Uint8Array.from([...first, ...second]);
    expect(new FrameDecoder().serverMessages(stream)).toEqual([
      { tag: "pong", stamp: 1 },
      { tag: "pong", stamp: 2 },
    ]);
  });

  it("rejects an oversized frame instead of buffering it forever", () => {
    expect(() => new FrameDecoder().push(Uint8Array.from([0xff, 0xff, 0xff, 0x7f]))).toThrow(
      /frame too large/,
    );
  });

  it("rejects truncated payloads", () => {
    const encoded = encodeServerMessage({ tag: "event", tick: 1, code: 1, a: 1, b: 2, c: 3, d: 4 });
    expect(() => decodeServerMessage(encoded.subarray(0, encoded.length - 1))).toThrow();
  });

  it("rejects an unknown server tag", () => {
    expect(() => decodeServerMessage(Uint8Array.from([99]))).toThrow(/bad server tag/);
  });

  it("rejects an unknown intent kind", () => {
    expect(() => decodeIntent(Uint8Array.from([42, 0, 0, 0]))).toThrow(/bad intent kind/);
  });
});
