import {
  backoffDelay,
  Connection,
  type ConnectionOptions,
  type SocketLike,
} from "../src/net/connection";
import { encodeServerMessage, frame, IntentKind, type ServerMessage } from "../src/net/protocol";

class FakeSocket implements SocketLike {
  static instances: FakeSocket[] = [];

  sent: Uint8Array[] = [];
  closed = false;

  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;

  constructor(readonly url: string) {
    FakeSocket.instances.push(this);
  }

  send(data: Uint8Array): void {
    this.sent.push(data);
  }

  close(): void {
    this.closed = true;
    this.onclose?.();
  }

  openIt(): void {
    this.onopen?.();
  }

  drop(): void {
    this.onclose?.();
  }

  deliver(message: ServerMessage): void {
    this.onmessage?.({ data: frame(encodeServerMessage(message)) });
  }

  deliverBytes(bytes: Uint8Array): void {
    this.onmessage?.({ data: bytes });
  }
}

function makeConnection(overrides: Partial<ConnectionOptions> = {}) {
  const pending: { callback: () => void; delay: number }[] = [];
  let clock = 1000;
  const options: ConnectionOptions = {
    url: "ws://localhost:9000/ws",
    name: "test",
    pingIntervalMs: 0,
    baseBackoffMs: 100,
    maxBackoffMs: 800,
    jitter: 0,
    maxAttempts: 12,
    now: () => clock,
    schedule: (callback, delayMs) => {
      pending.push({ callback, delay: delayMs });
    },
    createSocket: (url) => new FakeSocket(url),
    ...overrides,
  };
  return {
    connection: new Connection(options),
    pending,
    advance: (ms: number) => {
      clock += ms;
    },
  };
}

const hello: ServerMessage = { tag: "hello", version: 1, seed: 1234 };

beforeEach(() => {
  FakeSocket.instances = [];
});

describe("backoffDelay", () => {
  it("grows exponentially and stops at the ceiling", () => {
    expect(backoffDelay(1, 100, 800, 0)).toBe(100);
    expect(backoffDelay(2, 100, 800, 0)).toBe(200);
    expect(backoffDelay(3, 100, 800, 0)).toBe(400);
    expect(backoffDelay(4, 100, 800, 0)).toBe(800);
    expect(backoffDelay(9, 100, 800, 0)).toBe(800);
  });

  it("is deterministic, including the jitter term", () => {
    const first = [1, 2, 3, 4].map((n) => backoffDelay(n, 100, 800, 0.25));
    const second = [1, 2, 3, 4].map((n) => backoffDelay(n, 100, 800, 0.25));
    expect(first).toEqual(second);
  });

  it("keeps jittered delays within the jitter band", () => {
    for (let attempt = 1; attempt <= 6; attempt += 1) {
      const plain = backoffDelay(attempt, 100, 800, 0);
      const jittered = backoffDelay(attempt, 100, 800, 0.25);
      expect(Math.abs(jittered - plain)).toBeLessThanOrEqual(plain * 0.25 + 1);
    }
  });

  it("never returns a negative delay", () => {
    expect(backoffDelay(0, 100, 800, 0.9)).toBeGreaterThanOrEqual(0);
  });
});

describe("Connection lifecycle", () => {
  it("starts idle and opens on connect", () => {
    const { connection } = makeConnection();
    expect(connection.info.value.state).toBe("idle");
    connection.connect();
    expect(connection.info.value.state).toBe("connecting");
    expect(FakeSocket.instances).toHaveLength(1);
  });

  it("sends hello as soon as the socket opens", () => {
    const { connection } = makeConnection();
    connection.connect();
    FakeSocket.instances[0]?.openIt();
    expect(connection.info.value.state).toBe("open");
    expect(FakeSocket.instances[0]?.sent).toHaveLength(1);
    expect(connection.isOpen).toBe(true);
  });

  it("records the server version and seed from the server hello", () => {
    const { connection } = makeConnection();
    connection.connect();
    FakeSocket.instances[0]?.openIt();
    FakeSocket.instances[0]?.deliver(hello);
    expect(connection.info.value.serverVersion).toBe(1);
    expect(connection.info.value.seed).toBe(1234);
  });

  it("queues intents sent while disconnected and flushes them on open", () => {
    const { connection } = makeConnection();
    connection.sendIntent(3, { kind: IntentKind.Move, x: 1, y: 2, arg: 0 });
    expect(FakeSocket.instances).toHaveLength(0);
    connection.connect();
    FakeSocket.instances[0]?.openIt();
    expect(FakeSocket.instances[0]?.sent).toHaveLength(2);
  });

  it("reconnects with backoff after an unexpected close", () => {
    const { connection, pending } = makeConnection();
    connection.connect();
    FakeSocket.instances[0]?.openIt();
    FakeSocket.instances[0]?.drop();
    expect(connection.info.value.state).toBe("reconnecting");
    expect(pending).toHaveLength(1);
    expect(pending[0]?.delay).toBe(100);

    pending[0]?.callback();
    expect(FakeSocket.instances).toHaveLength(2);
    FakeSocket.instances[1]?.openIt();
    expect(connection.info.value.state).toBe("open");
    expect(connection.info.value.attempt).toBe(0);
  });

  it("backs off further on repeated failures", () => {
    const { connection, pending } = makeConnection();
    connection.connect();
    for (let round = 0; round < 3; round += 1) {
      FakeSocket.instances.at(-1)?.drop();
      pending.at(-1)?.callback();
    }
    expect(connection.info.value.attempt).toBe(3);
    expect(pending.at(-1)?.delay).toBe(400);
  });

  it("stops retrying once the attempt budget is spent", () => {
    const { connection, pending } = makeConnection();
    connection.connect();

    let rounds = 0;
    while (connection.info.value.state !== "failed" && rounds < 20) {
      const scheduled = pending.length;
      FakeSocket.instances.at(-1)?.drop();
      rounds += 1;
      if (pending.length > scheduled) {
        pending.at(-1)?.callback();
      }
    }
    expect(connection.info.value.state).toBe("failed");
    expect(rounds).toBe(13);
    expect(connection.info.value.attempt).toBe(13);

    const socketsAtFailure = FakeSocket.instances.length;
    const scheduledAtFailure = pending.length;
    FakeSocket.instances.at(-1)?.drop();
    expect(connection.info.value.state).toBe("failed");
    expect(pending).toHaveLength(scheduledAtFailure);
    expect(FakeSocket.instances).toHaveLength(socketsAtFailure);
  });

  it("starts over when connect is called again after giving up", () => {
    const { connection, pending } = makeConnection();
    connection.connect();
    for (let round = 0; round < 13; round += 1) {
      const scheduled = pending.length;
      FakeSocket.instances.at(-1)?.drop();
      if (pending.length > scheduled) {
        pending.at(-1)?.callback();
      }
    }
    expect(connection.info.value.state).toBe("failed");

    connection.connect();
    expect(connection.info.value.state).toBe("connecting");
    expect(connection.info.value.attempt).toBe(0);
    FakeSocket.instances.at(-1)?.openIt();
    expect(connection.info.value.state).toBe("open");
  });

  it("does not reconnect after disconnect", () => {
    const { connection, pending } = makeConnection();
    connection.connect();
    FakeSocket.instances[0]?.openIt();
    connection.disconnect();
    expect(connection.info.value.state).toBe("idle");
    expect(FakeSocket.instances[0]?.closed).toBe(true);
    expect(pending).toHaveLength(0);
  });

  it("reports a creation failure and retries", () => {
    const { connection, pending } = makeConnection({
      createSocket: () => {
        throw new Error("refused");
      },
    });
    connection.connect();
    expect(connection.info.value.lastError).toBe("refused");
    expect(pending).toHaveLength(1);
    expect(connection.info.value.state).toBe("reconnecting");
  });

  it("recovers from an oversized frame and decodes the next one", () => {
    const { connection } = makeConnection();
    const seen: ServerMessage[] = [];
    connection.onMessage = (message) => seen.push(message);
    connection.connect();
    FakeSocket.instances[0]?.openIt();

    FakeSocket.instances[0]?.deliverBytes(new Uint8Array([0x80, 0x80, 0x05]));
    expect(connection.info.value.lastError).toBe("frame too large");

    FakeSocket.instances[0]?.deliver(hello);
    expect(seen).toEqual([hello]);
    expect(connection.info.value.state).toBe("open");
  });

  it("skips an undecodable frame payload and keeps going", () => {
    const { connection } = makeConnection();
    const seen: ServerMessage[] = [];
    connection.onMessage = (message) => seen.push(message);
    connection.connect();
    FakeSocket.instances[0]?.openIt();

    FakeSocket.instances[0]?.deliverBytes(new Uint8Array([1, 0x7f]));
    expect(connection.info.value.lastError).toBe("bad server tag");

    FakeSocket.instances[0]?.deliver({ tag: "pong", stamp: 3 });
    expect(seen).toEqual([{ tag: "pong", stamp: 3 }]);
  });

  it("waits for the rest of an unterminated length prefix", () => {
    const { connection } = makeConnection();
    const seen: ServerMessage[] = [];
    connection.onMessage = (message) => seen.push(message);
    connection.connect();
    FakeSocket.instances[0]?.openIt();

    FakeSocket.instances[0]?.deliverBytes(new Uint8Array([0xff, 0xff, 0xff, 0xff]));
    expect(seen).toEqual([]);
    expect(connection.info.value.lastError).toBe(null);
  });

  it("reassembles a frame split across two messages", () => {
    const { connection } = makeConnection();
    const seen: ServerMessage[] = [];
    connection.onMessage = (message) => seen.push(message);
    connection.connect();
    FakeSocket.instances[0]?.openIt();

    const bytes = frame(encodeServerMessage({ tag: "snapshot", tick: 42, hash: 99 }));
    FakeSocket.instances[0]?.deliverBytes(bytes.slice(0, 3));
    FakeSocket.instances[0]?.deliverBytes(bytes.slice(3));
    expect(seen).toEqual([{ tag: "snapshot", tick: 42, hash: 99 }]);
  });

  it("delivers two frames coalesced into one message", () => {
    const { connection } = makeConnection();
    const seen: ServerMessage[] = [];
    connection.onMessage = (message) => seen.push(message);
    connection.connect();
    FakeSocket.instances[0]?.openIt();

    const both = new Uint8Array([
      ...frame(encodeServerMessage({ tag: "pong", stamp: 7 })),
      ...frame(encodeServerMessage({ tag: "snapshot", tick: 1, hash: 2 })),
    ]);
    FakeSocket.instances[0]?.deliverBytes(both);
    expect(seen).toEqual([
      { tag: "pong", stamp: 7 },
      { tag: "snapshot", tick: 1, hash: 2 },
    ]);
  });

  it("ignores text frames", () => {
    const { connection } = makeConnection();
    connection.connect();
    FakeSocket.instances[0]?.openIt();
    FakeSocket.instances[0]?.deliverBytes("hello" as unknown as Uint8Array);
    expect(connection.info.value.state).toBe("open");
  });
});

describe("ping and pong", () => {
  it("measures round trip time", () => {
    const { connection, advance } = makeConnection({ pingIntervalMs: 10 });
    connection.connect();
    FakeSocket.instances[0]?.openIt();
    advance(250);
    FakeSocket.instances[0]?.deliver({ tag: "pong", stamp: 1 });
    expect(connection.info.value.roundTripMs).toBeGreaterThan(0);
  });

  it("does not ping when the interval is disabled", () => {
    const { connection, advance } = makeConnection({ pingIntervalMs: 0 });
    connection.connect();
    FakeSocket.instances[0]?.openIt();
    const sentAfterHello = FakeSocket.instances[0]?.sent.length ?? 0;
    advance(1000);
    expect(FakeSocket.instances[0]?.sent.length).toBe(sentAfterHello);
  });
});
