import { Store } from "../state/stores";
import { EntityKind, type Entity, type WorldStore } from "../state/world";
import type { ServerMessage } from "./protocol";

export const enum EventCode {
  PlayerMoved = 1,
  EntityPlaced = 2,
  EntityRemoved = 3,
  TileMined = 4,
  MachineProduced = 5,
  ResearchProgress = 6,
  ResearchCompleted = 7,
  WaterExtracted = 8,
  TickCompleted = 9,
}

export const TICK_HZ = 60;
export const TICK_MS = 1000 / TICK_HZ;

export interface ClockInfo {
  readonly lastTick: number;
  readonly hash: number;
  readonly receivedAtMs: number;
  readonly stalenessMs: number;

  readonly alpha: number;
  readonly droppedFrames: number;
}

export class SnapshotClock {
  readonly info = new Store<ClockInfo>({
    lastTick: 0,
    hash: 0,
    receivedAtMs: 0,
    stalenessMs: 0,
    alpha: 0,
    droppedFrames: 0,
  });

  private previousTickValue = 0;
  private established = false;

  constructor(private readonly now: () => number = () => Date.now()) {}

  observe(tick: number, hash: number): void {
    this.previousTickValue = this.info.value.lastTick;
    this.info.update((current) => ({
      ...current,
      lastTick: tick,
      hash,
      receivedAtMs: this.now(),
      droppedFrames: this.established
        ? current.droppedFrames + Math.max(0, tick - current.lastTick)
        : current.droppedFrames,
    }));
    this.established = true;
  }

  sample(): void {
    this.info.update((current) => {
      const stalenessMs = Math.max(0, this.now() - current.receivedAtMs);
      return {
        ...current,
        stalenessMs,
        alpha: Math.max(0, Math.min(1, stalenessMs / TICK_MS)),
      };
    });
  }

  get lastTick(): number {
    return this.info.value.lastTick;
  }

  get previousTick(): number {
    return this.previousTickValue;
  }

  get stalled(): boolean {
    return this.info.value.stalenessMs > TICK_MS * 4;
  }
}

export function applyServerMessage(
  world: WorldStore,
  clock: SnapshotClock,
  message: ServerMessage,
): void {
  switch (message.tag) {
    case "hello":
      world.pushEvent(`server hello: protocol ${message.version}, seed ${message.seed}`);
      break;
    case "snapshot":
      world.setSnapshot({ tick: message.tick, hash: message.hash });
      world.setStats({ ...world.currentStats, tick: message.tick, hash: message.hash });
      clock.observe(message.tick, message.hash);
      break;
    case "event": {
      world.pushEvent(describeEvent(message));
      const entity = entityFromEvent(message);
      if (entity !== null) {
        world.upsertEntity(entity);
      }
      break;
    }
    case "pong":
      break;
  }
}

export function entityFromEvent(message: ServerMessage): Entity | null {
  if (message.tag !== "event") {
    return null;
  }
  if (message.code === EventCode.EntityPlaced) {
    return {
      kind: message.a as EntityKind,
      index: message.b,
      x: message.c,
      y: message.d,
      prototype: 0,
      rotation: 0,
      label: kindName(message.a),
    };
  }
  if (message.code === EventCode.EntityRemoved) {
    return null;
  }
  if (message.code === EventCode.PlayerMoved) {
    return {
      kind: EntityKind.Player,
      index: message.a,
      x: message.b,
      y: message.c,
      prototype: 0,
      rotation: 0,
      label: `player ${message.a}`,
    };
  }
  return null;
}

export function removeEntityFromEvent(world: WorldStore, message: ServerMessage): boolean {
  if (message.tag !== "event" || message.code !== EventCode.EntityRemoved) {
    return false;
  }
  world.removeEntity(message.a as EntityKind, message.b);
  return true;
}

export function kindName(kind: number): string {
  switch (kind) {
    case EntityKind.Belt:
      return "belt";
    case EntityKind.Machine:
      return "machine";
    case EntityKind.Inserter:
      return "inserter";
    case EntityKind.Player:
      return "player";
    default:
      return `entity ${kind}`;
  }
}

export function describeEvent(message: ServerMessage): string {
  if (message.tag !== "event") {
    return message.tag;
  }
  switch (message.code) {
    case EventCode.PlayerMoved:
      return `t${message.tick} player ${message.a} moved to ${message.b},${message.c}`;
    case EventCode.EntityPlaced:
      return `t${message.tick} placed ${kindName(message.a)} #${message.b} at ${message.c},${message.d}`;
    case EventCode.EntityRemoved:
      return `t${message.tick} removed ${kindName(message.a)} #${message.b}`;
    case EventCode.TileMined:
      return `t${message.tick} mined item ${message.c} x${message.d} at ${message.a},${message.b}`;
    case EventCode.MachineProduced:
      return `t${message.tick} machine #${message.a} made item ${message.b} x${message.c}`;
    case EventCode.ResearchProgress:
      return `t${message.tick} tech ${message.a} at ${message.b}`;
    case EventCode.ResearchCompleted:
      return `t${message.tick} tech ${message.a} complete`;
    case EventCode.WaterExtracted:
      return `t${message.tick} pump #${message.a} extracted water`;
    case EventCode.TickCompleted:
      return `t${message.tick} tick ${message.a}`;
    default:
      return `t${message.tick} event ${message.code} (${message.a},${message.b},${message.c})`;
  }
}
