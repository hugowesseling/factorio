import { Store } from "../state/stores";
import {
  encodeClientMessage,
  FrameDecoder,
  PROTOCOL_VERSION,
  ProtocolError,
  decodeServerMessage,
  type ClientMessage,
  type Intent,
  type ServerMessage,
} from "./protocol";

export type ConnectionState = "idle" | "connecting" | "open" | "reconnecting" | "failed";

export interface ConnectionOptions {
  readonly url: string;
  readonly name: string;
  readonly pingIntervalMs: number;
  readonly baseBackoffMs: number;
  readonly maxBackoffMs: number;
  readonly jitter: number;
  readonly maxAttempts: number;
  readonly now: () => number;
  readonly schedule: (callback: () => void, delayMs: number) => void;
  readonly createSocket: (url: string) => SocketLike;
}

export interface SocketLike {
  send(data: Uint8Array): void;
  close(): void;
  onopen: (() => void) | null;
  onclose: (() => void) | null;
  onerror: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
}

export interface ConnectionInfo {
  readonly state: ConnectionState;
  readonly attempt: number;
  readonly nextRetryMs: number | null;
  readonly serverVersion: number | null;
  readonly seed: number | null;
  readonly lastError: string | null;
  readonly roundTripMs: number | null;
}

export const DEFAULT_OPTIONS: Omit<ConnectionOptions, "url" | "name" | "createSocket"> = {
  pingIntervalMs: 2000,
  baseBackoffMs: 250,
  maxBackoffMs: 8000,
  jitter: 0.25,
  maxAttempts: 12,
  now: () => Date.now(),
  schedule: (callback, delayMs) => {
    setTimeout(callback, delayMs);
  },
};

export function backoffDelay(attempt: number, baseMs: number, maxMs: number, jitter: number): number {
  const exponential = Math.min(baseMs * 2 ** Math.max(0, attempt - 1), maxMs);
  const spread = ((attempt * 2654435761) % 1000) / 1000 - 0.5;
  return Math.max(0, Math.round(exponential * (1 + spread * 2 * jitter)));
}

export class Connection {
  readonly info = new Store<ConnectionInfo>({
    state: "idle",
    attempt: 0,
    nextRetryMs: null,
    serverVersion: null,
    seed: null,
    lastError: null,
    roundTripMs: null,
  });

  private readonly decoder = new FrameDecoder();
  private readonly queue: ClientMessage[] = [];
  private socket: SocketLike | null = null;
  private attempt = 0;
  private pingTimer: ReturnType<typeof setInterval> | null = null;
  private pingSentAt = 0;
  private pingStamp = 0;
  private stopped = false;

  constructor(private readonly options: ConnectionOptions) {}

  onMessage: (message: ServerMessage) => void = () => {};
  onOpen: () => void = () => {};
  onClose: () => void = () => {};

  get isOpen(): boolean {
    return this.info.value.state === "open";
  }

  connect(): void {
    this.stopped = false;
    this.attempt = 0;
    this.patch({ state: "connecting", attempt: 0, nextRetryMs: null, lastError: null });
    this.open();
  }

  disconnect(): void {
    this.stopped = true;
    this.stopPing();
    this.socket?.close();
    this.socket = null;
    this.patch({ state: "idle", nextRetryMs: null });
  }

  send(message: ClientMessage): void {
    if (!this.isOpen || this.socket === null) {
      if (this.queue.length < 256) {
        this.queue.push(message);
      }
      return;
    }
    this.socket.send(encodeClientMessage(message));
  }

  sendHello(): void {
    this.send({ tag: "hello", name: this.options.name, version: PROTOCOL_VERSION });
  }

  sendIntent(player: number, intent: Intent): void {
    this.send({ tag: "intent", player, intent });
  }

  private open(): void {
    if (this.stopped) {
      return;
    }
    this.patch({ state: this.attempt === 0 ? "connecting" : "reconnecting", lastError: null });
    let socket: SocketLike;
    try {
      socket = this.options.createSocket(this.options.url);
    } catch (error) {
      this.patch({ lastError: describe(error) });
      this.scheduleRetry();
      return;
    }
    this.socket = socket;

    socket.onopen = () => {
      this.attempt = 0;
      this.decoder.reset();
      this.patch({ state: "open", attempt: 0, nextRetryMs: null, lastError: null });
      this.sendHello();
      const queued = this.queue.splice(0, this.queue.length);
      for (const message of queued) {
        this.send(message);
      }
      this.startPing();
      this.onOpen();
    };

    socket.onmessage = (event) => {
      const bytes = toBytes(event.data);
      if (bytes === null) {
        this.patch({ lastError: "server sent a non-binary frame" });
        return;
      }
      let frames: Uint8Array[];
      try {
        frames = this.decoder.push(bytes);
      } catch (error) {
        this.patch({ lastError: describe(error) });
        this.decoder.reset();
        return;
      }
      for (const frame of frames) {
        let message: ServerMessage;
        try {
          message = decodeServerMessage(frame);
        } catch (error) {
          this.patch({ lastError: describe(error) });
          continue;
        }
        this.handle(message);
      }
    };

    socket.onerror = () => {
      this.patch({ lastError: "socket error" });
    };

    socket.onclose = () => {
      this.stopPing();
      this.socket = null;
      if (!this.stopped) {
        this.patch({ state: "reconnecting" });
        this.onClose();
        this.scheduleRetry();
      } else {
        this.patch({ state: "idle" });
      }
    };
  }

  private handle(message: ServerMessage): void {
    switch (message.tag) {
      case "hello":
        this.patch({ serverVersion: message.version, seed: message.seed });
        break;
      case "pong":
        this.patch({ roundTripMs: this.options.now() - this.pingSentAt });
        break;
      case "snapshot":
      case "event":
        break;
    }
    this.onMessage(message);
  }

  private startPing(): void {
    this.stopPing();
    if (this.options.pingIntervalMs <= 0) {
      return;
    }
    this.pingTimer = setInterval(() => {
      this.pingStamp += 1;
      this.pingSentAt = this.options.now();
      this.send({ tag: "ping", stamp: this.pingStamp });
    }, this.options.pingIntervalMs);
  }

  private stopPing(): void {
    if (this.pingTimer !== null) {
      clearInterval(this.pingTimer);
      this.pingTimer = null;
    }
  }

  private scheduleRetry(): void {
    if (this.stopped || this.info.value.state === "failed") {
      return;
    }
    this.attempt += 1;
    if (this.attempt > this.options.maxAttempts) {
      this.patch({ state: "failed", attempt: this.attempt, nextRetryMs: null });
      return;
    }
    const delay = backoffDelay(
      this.attempt,
      this.options.baseBackoffMs,
      this.options.maxBackoffMs,
      this.options.jitter,
    );
    this.patch({ state: "reconnecting", attempt: this.attempt, nextRetryMs: delay });
    this.options.schedule(() => {
      this.patch({ nextRetryMs: null });
      this.open();
    }, delay);
  }

  private patch(partial: Partial<ConnectionInfo>): void {
    this.info.update((current) => ({ ...current, ...partial }));
  }
}

function toBytes(data: unknown): Uint8Array | null {
  if (data instanceof Uint8Array) {
    return data;
  }
  if (data instanceof ArrayBuffer) {
    return new Uint8Array(data);
  }
  if (ArrayBuffer.isView(data)) {
    const view = data as ArrayBufferView;
    return new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
  }
  return null;
}

function describe(error: unknown): string {
  if (error instanceof ProtocolError) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export function defaultSocket(url: string): SocketLike {
  return new WebSocket(url) as unknown as SocketLike;
}

export function createConnection(url: string, name: string): Connection {
  return new Connection({ ...DEFAULT_OPTIONS, url, name, createSocket: defaultSocket });
}
