import { machineSize } from "../src/content";
import { footprint, type Entity, EntityKind, WorldStore } from "../src/state/world";

function entity(partial: Partial<Entity> & Pick<Entity, "kind" | "index">): Entity {
  return {
    x: 0,
    y: 0,
    prototype: 0,
    rotation: 0,
    label: "",
    ...partial,
  };
}

describe("WorldStore entities", () => {
  it("keeps the player index in step with the entity table", () => {
    const world = new WorldStore(7);
    world.upsertEntity(entity({ kind: EntityKind.Player, index: 0, x: 1, y: 1 }));
    expect(world.player(0)?.x).toBe(1);
    expect(world.playerCount).toBe(1);

    world.removeEntity(EntityKind.Player, 0);
    expect(world.player(0)).toBeUndefined();
    expect(world.playerCount).toBe(0);
  });

  it("drops a stale player entry when the index is reused by another kind", () => {
    const world = new WorldStore(7);
    world.upsertEntity(entity({ kind: EntityKind.Player, index: 0, x: 1, y: 1 }));
    world.upsertEntity(entity({ kind: EntityKind.Machine, index: 0, x: 1, y: 1, prototype: 1 }));

    expect(world.player(0)).toBeUndefined();
    expect(world.playerCount).toBe(0);
    expect(world.entityAt({ x: 1, y: 1 })?.kind).toBe(EntityKind.Machine);
  });

  it("returns nothing for a tile with no entity", () => {
    expect(new WorldStore(7).entityAt({ x: 0, y: 0 })).toBeUndefined();
  });

  it("covers every tile of a multi-tile machine", () => {
    const world = new WorldStore(7);
    world.upsertEntity(entity({ kind: EntityKind.Machine, index: 4, x: 2, y: 2, prototype: 1 }));
    const size = machineSize(1);

    for (let dy = 0; dy < size.height; dy += 1) {
      for (let dx = 0; dx < size.width; dx += 1) {
        expect(world.entityAt({ x: 2 + dx, y: 2 + dy })?.index).toBe(4);
      }
    }
    expect(world.entityAt({ x: 2 + size.width, y: 2 })).toBeUndefined();
    expect(world.entityAt({ x: 2, y: 2 + size.height })).toBeUndefined();
  });

  it("prefers the player when a player shares a tile", () => {
    const world = new WorldStore(7);
    world.upsertEntity(entity({ kind: EntityKind.Player, index: 5, x: 0, y: 0 }));
    world.upsertEntity(entity({ kind: EntityKind.Machine, index: 2, x: 0, y: 0, prototype: 1 }));
    expect(world.entityAt({ x: 0, y: 0 })?.kind).toBe(EntityKind.Player);
  });

  it("picks the same entity regardless of the order things arrive in", () => {
    const first = new WorldStore(7);
    first.upsertEntity(entity({ kind: EntityKind.Belt, index: 7, x: 0, y: 0 }));
    first.upsertEntity(entity({ kind: EntityKind.Inserter, index: 3, x: 0, y: 0 }));

    const second = new WorldStore(7);
    second.upsertEntity(entity({ kind: EntityKind.Inserter, index: 3, x: 0, y: 0 }));
    second.upsertEntity(entity({ kind: EntityKind.Belt, index: 7, x: 0, y: 0 }));

    expect(first.entityAt({ x: 0, y: 0 })?.index).toBe(second.entityAt({ x: 0, y: 0 })?.index);
    expect(first.entityAt({ x: 0, y: 0 })?.index).toBe(3);
  });

  it("updates an entity in place when the same index is resend", () => {
    const world = new WorldStore(7);
    world.upsertEntity(entity({ kind: EntityKind.Player, index: 0, x: 0, y: 0 }));
    world.upsertEntity(entity({ kind: EntityKind.Player, index: 0, x: 4, y: 6 }));
    expect(world.player(0)).toMatchObject({ x: 4, y: 6 });
    expect(world.entityAt({ x: 0, y: 0 })).toBeUndefined();
  });

  it("treats every kind but machines as a single tile", () => {
    expect(footprint(entity({ kind: EntityKind.Belt, index: 0 }))).toEqual({ width: 1, height: 1 });
    expect(footprint(entity({ kind: EntityKind.Machine, index: 0, prototype: 1 }))).toEqual(machineSize(1));
  });
});

describe("WorldStore belt items", () => {
  it("copies the list it is given", () => {
    const world = new WorldStore(7);
    const items = [{ x: 0, y: 0, slot: 0, item: 0, progress: 0 }];
    world.setBeltItems(items);
    items.push({ x: 1, y: 0, slot: 0, item: 0, progress: 0 });
    expect(world.allBeltItems).toHaveLength(1);
    expect(world.allBeltItems).not.toBe(items);
  });
});

describe("WorldStore events", () => {
  it("keeps only the most recent lines", () => {
    const world = new WorldStore(7);
    for (let index = 0; index < 100; index += 1) {
      world.pushEvent(`line ${index}`);
    }
    expect(world.recentEvents).toHaveLength(64);
    expect(world.recentEvents[63]).toBe("line 99");
    expect(world.recentEvents[0]).toBe("line 36");
  });
});