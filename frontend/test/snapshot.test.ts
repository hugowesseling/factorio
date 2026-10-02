import {
  applyServerMessage,
  describeEvent,
  entityFromEvent,
  EventCode,
  kindName,
  removeEntityFromEvent,
  SnapshotClock,
  TICK_MS,
} from "../src/net/snapshot";
import type { ServerMessage } from "../src/net/protocol";
import { EntityKind, WorldStore } from "../src/state/world";

function event(code: EventCode, a: number, b = 0, c = 0, tick = 1, d = 0): ServerMessage {
  return { tag: "event", tick, code, a, b, c, d };
}

describe("SnapshotClock", () => {
  it("starts idle at tick zero", () => {
    const clock = new SnapshotClock(() => 0);
    expect(clock.info.value.lastTick).toBe(0);
    expect(clock.stalled).toBe(false);
  });

  it("records the tick and hash of a snapshot", () => {
    const clock = new SnapshotClock(() => 1000);
    clock.observe(120, 4242);
    expect(clock.info.value.lastTick).toBe(120);
    expect(clock.info.value.hash).toBe(4242);
    expect(clock.info.value.receivedAtMs).toBe(1000);
    expect(clock.previousTick).toBe(0);
  });

  it("interpolates by the elapsed fraction of a tick", () => {
    let now = 0;
    const clock = new SnapshotClock(() => now);
    clock.observe(10, 1);
    now = TICK_MS / 4;
    clock.sample();
    expect(clock.info.value.alpha).toBeCloseTo(0.25, 5);
  });

  it("never extrapolates past the newest snapshot", () => {
    let now = 0;
    const clock = new SnapshotClock(() => now);
    clock.observe(10, 1);
    now = TICK_MS * 10;
    clock.sample();
    expect(clock.info.value.alpha).toBe(1);
  });

  it("reports a stalled stream once it falls several ticks behind", () => {
    let now = 0;
    const clock = new SnapshotClock(() => now);
    clock.observe(10, 1);
    now = TICK_MS * 5;
    clock.sample();
    expect(clock.stalled).toBe(true);
  });

  it("counts ticks it missed, so a gap is visible", () => {
    let now = 0;
    const clock = new SnapshotClock(() => now);
    clock.observe(10, 1);
    now = TICK_MS;
    clock.observe(15, 1);
    expect(clock.info.value.droppedFrames).toBe(5);
  });

  it("ignores a snapshot that goes backwards", () => {
    let now = 0;
    const clock = new SnapshotClock(() => now);
    clock.observe(20, 1);
    now = TICK_MS;
    clock.observe(5, 1);
    expect(clock.info.value.droppedFrames).toBe(0);
    expect(clock.info.value.lastTick).toBe(5);
  });
});

describe("applyServerMessage", () => {
  it("writes the snapshot into the world store", () => {
    const world = new WorldStore(7);
    const clock = new SnapshotClock(() => 0);
    applyServerMessage(world, clock, { tag: "snapshot", tick: 64, hash: 99 });
    expect(world.currentSnapshot).toEqual({ tick: 64, hash: 99 });
    expect(world.currentStats.tick).toBe(64);
    expect(clock.info.value.lastTick).toBe(64);
  });

  it("keeps power figures across a snapshot", () => {
    const world = new WorldStore(7);
    const clock = new SnapshotClock(() => 0);
    world.setStats({
      tick: 1,
      hash: 0,
      powerProduced: 900,
      powerConsumed: 450,
    });
    applyServerMessage(world, clock, { tag: "snapshot", tick: 2, hash: 5 });
    expect(world.currentStats.powerProduced).toBe(900);
    expect(world.currentStats.powerConsumed).toBe(450);
  });

  it("logs the server hello with its seed", () => {
    const world = new WorldStore(7);
    const clock = new SnapshotClock(() => 0);
    applyServerMessage(world, clock, { tag: "hello", version: 1, seed: 4242 });
    expect(world.recentEvents.at(-1)).toContain("seed 4242");
  });

  it("logs events without mutating the world on its own", () => {
    const world = new WorldStore(7);
    const clock = new SnapshotClock(() => 0);
    applyServerMessage(world, clock, event(EventCode.TileMined, 4, 25, 9, 1, 2));
    expect(world.recentEvents.at(-1)).toBe("t1 mined item 9 x2 at 4,25");
  });

  it("keeps only a bounded log", () => {
    const world = new WorldStore(7);
    const clock = new SnapshotClock(() => 0);
    for (let index = 0; index < 200; index += 1) {
      applyServerMessage(world, clock, event(EventCode.TickCompleted, index, 0, 0, index));
    }
    expect(world.recentEvents).toHaveLength(64);
    expect(world.recentEvents.at(-1)).toContain(`t199 tick 199`);
  });
});

describe("event decoding", () => {
  it("builds a player entity from a movement event", () => {
    const entity = entityFromEvent(event(EventCode.PlayerMoved, 2, -5, 7));
    expect(entity).toEqual({
      kind: EntityKind.Player,
      index: 2,
      x: -5,
      y: 7,
      prototype: 0,
      rotation: 0,
      label: "player 2",
    });
  });

  it("builds an entity from a placement event", () => {
    const entity = entityFromEvent(event(EventCode.EntityPlaced, EntityKind.Machine, 4, 9, 1, -3));
    expect(entity).toEqual({
      kind: EntityKind.Machine,
      index: 4,
      x: 9,
      y: -3,
      prototype: 0,
      rotation: 0,
      label: "machine",
    });
  });

  it("returns null for events that are not entities", () => {
    expect(entityFromEvent(event(EventCode.MachineProduced, 1, 2, 3))).toBeNull();
    expect(entityFromEvent({ tag: "pong", stamp: 1 })).toBeNull();
  });

  it("removes the entity named by a removal event", () => {
    const world = new WorldStore(7);
    world.upsertEntity({
      kind: EntityKind.Inserter,
      index: 3,
      x: 1,
      y: 1,
      prototype: 0,
      rotation: 0,
      label: "inserter",
    });
    const removal = event(EventCode.EntityRemoved, EntityKind.Inserter, 3);
    expect(removeEntityFromEvent(world, removal)).toBe(true);
    expect(world.entityAt({ x: 1, y: 1 })).toBeUndefined();
    expect(removeEntityFromEvent(world, event(EventCode.PlayerMoved, 1, 1, 1))).toBe(false);
  });

  it("names every entity kind, including unknown ones", () => {
    expect(kindName(EntityKind.Belt)).toBe("belt");
    expect(kindName(EntityKind.Player)).toBe("player");
    expect(kindName(9)).toBe("entity 9");
  });

  it("describes each known event", () => {
    expect(describeEvent(event(EventCode.PlayerMoved, 1, 4, 5, 9))).toBe(
      "t9 player 1 moved to 4,5",
    );
    expect(describeEvent(event(EventCode.EntityPlaced, EntityKind.Belt, 2, 0, 0))).toContain(
      "placed belt #2",
    );
    expect(describeEvent(event(EventCode.ResearchCompleted, 3, 0, 0, 7))).toBe("t7 tech 3 complete");
    expect(describeEvent(event(EventCode.WaterExtracted, 8))).toContain("pump #8");
    expect(describeEvent(event(EventCode.ResearchProgress, 1, 50))).toContain("tech 1 at 50");
    expect(describeEvent(event(EventCode.MachineProduced, 2, 4, 3))).toContain(
      "machine #2 made item 4 x3",
    );
  });

  it("falls back to the raw code for an unknown event", () => {
    expect(describeEvent(event(200 as EventCode, 1, 2, 3))).toBe("t1 event 200 (1,2,3)");
  });

  it("names a non-event message by its tag", () => {
    expect(describeEvent({ tag: "snapshot", tick: 1, hash: 2 })).toBe("snapshot");
  });
});
