import {
  advanceItems,
  beltTiles,
  buildDemoScene,
  DEMO_FURNACE_PERIOD,
  DEMO_ITEM_COAL,
  DemoSim,
  insertItem,
  LAYOUT,
  seedBeltItems,
} from "../src/sim/demo";
import { EntityKind, WorldStore } from "../src/state/world";
import { MovePredictor } from "../src/state/predict";

describe("layout", () => {
  it("lays belts out from the start tile along +x", () => {
    const tiles = beltTiles();
    expect(tiles).toHaveLength(LAYOUT.beltLength);
    expect(tiles[0]).toEqual(LAYOUT.beltStart);
    expect(tiles.at(-1)).toEqual({ x: LAYOUT.beltStart.x + LAYOUT.beltLength - 1, y: 0 });
  });

  it("never lays the belt through water", () => {
    const world = new WorldStore(7);
    for (const tile of beltTiles()) {
      expect(world.tile(tile).water).toBe(false);
    }
  });

  it("places the machines on land as well", () => {
    const world = new WorldStore(7);
    for (const tile of [LAYOUT.generator, LAYOUT.furnace, LAYOUT.lab, LAYOUT.inserter]) {
      expect(world.tile(tile).water).toBe(false);
    }
  });
});

describe("buildDemoScene", () => {
  it("places belts, machines, an inserter and the player", () => {
    const world = new WorldStore(7);
    buildDemoScene(world);
    const entities = [...world.allEntities];
    expect(entities).toHaveLength(LAYOUT.beltLength + 5);
    expect(entities.filter((entity) => entity.kind === EntityKind.Belt)).toHaveLength(
      LAYOUT.beltLength,
    );
    expect(entities.filter((entity) => entity.kind === EntityKind.Player)).toHaveLength(1);
  });

  it("gives the furnace its two tile footprint", () => {
    const world = new WorldStore(7);
    buildDemoScene(world);
    const furnace = world.entityAt(LAYOUT.furnace);
    expect(furnace?.kind).toBe(EntityKind.Machine);
    expect(world.entityAt({ x: LAYOUT.furnace.x + 1, y: LAYOUT.furnace.y })).toBeDefined();
    expect(world.entityAt({ x: LAYOUT.furnace.x + 2, y: LAYOUT.furnace.y })).toBeUndefined();
  });

  it("is idempotent, so rebuilding does not stack entities", () => {
    const world = new WorldStore(7);
    buildDemoScene(world);
    const first = [...world.allEntities].length;
    buildDemoScene(world);
    expect([...world.allEntities]).toHaveLength(first);
  });

  it("spawns the player at the origin", () => {
    const world = new WorldStore(7);
    buildDemoScene(world);
    expect(world.player(0)?.x).toBe(LAYOUT.spawn.x);
    expect(world.player(0)?.y).toBe(LAYOUT.spawn.y);
  });
});

describe("belt items", () => {
  const end = LAYOUT.beltStart.x + LAYOUT.beltLength;

  it("starts with items spread along the belt", () => {
    const items = seedBeltItems();
    expect(items.length).toBeGreaterThan(0);
    for (const item of items) {
      expect(item.x).toBeGreaterThanOrEqual(LAYOUT.beltStart.x);
      expect(item.x).toBeLessThan(end);
      expect(item.item).toBe(DEMO_ITEM_COAL);
    }
  });

  it("moves every item forward by one belt step", () => {
    const items = [{ x: 0, y: 0, slot: 0, item: 1, progress: 0 }];
    const moved = advanceItems(items, 10);
    expect(moved).toHaveLength(1);
    expect(moved[0]?.x).toBe(0);
    expect(moved[0]?.progress).toBeGreaterThan(0);
    expect(moved[0]?.progress).toBeLessThan(1);
  });

  it("rolls over into the next tile", () => {
    const items = [{ x: 0, y: 0, slot: 0, item: 1, progress: 0.98 }];
    const moved = advanceItems(items, 10);
    expect(moved[0]?.x).toBe(1);
    expect(moved[0]?.progress).toBeLessThan(0.1);
  });

  it("drops items that reach the end of the belt", () => {
    const crossing = [{ x: 2, y: 0, slot: 0, item: 1, progress: 0.99 }];
    expect(advanceItems(crossing, 3)).toHaveLength(0);
    const shortOf = [{ x: 2, y: 0, slot: 0, item: 1, progress: 0.9 }];
    expect(advanceItems(shortOf, 3)).toHaveLength(1);
  });

  it("keeps items ordered so rendering stays stable", () => {
    const items = [
      { x: 0, y: 0, slot: 0, item: 1, progress: 0.5 },
      { x: 1, y: 0, slot: 0, item: 1, progress: 0.1 },
    ];
    expect(advanceItems(items, 10).map((item) => item.x)).toEqual([0, 1]);
  });

  it("refuses to insert on top of a half full entrance", () => {
    const occupied = [{ x: LAYOUT.beltStart.x, y: 0, slot: 0, item: 1, progress: 0.1 }];
    expect(insertItem(occupied)).toHaveLength(1);
    expect(insertItem([])).toHaveLength(1);
    expect(insertItem([])[0]?.x).toBe(LAYOUT.beltStart.x);
  });
});

describe("DemoSim", () => {
  it("advances one tick at a time", () => {
    const world = new WorldStore(7);
    const sim = new DemoSim(world);
    expect(sim.state.tick).toBe(0);
    sim.step();
    sim.step();
    expect(sim.state.tick).toBe(2);
    expect(world.currentStats.tick).toBe(2);
  });

  it("reports power that switches with the machine cycle", () => {
    const world = new WorldStore(7);
    const sim = new DemoSim(world);
    const seen: number[] = [];
    for (let tick = 0; tick < 60; tick += 1) {
      seen.push(sim.step().generatorActive ? 1 : 0);
    }
    expect(new Set(seen)).toEqual(new Set([0, 1]));
  });

  it("draws power from the lab every tick", () => {
    const world = new WorldStore(7);
    const sim = new DemoSim(world);
    sim.step();
    expect(world.currentStats.powerConsumed).toBeGreaterThan(0);
  });

  it("produces an item every furnace cycle", () => {
    const world = new WorldStore(7);
    const sim = new DemoSim(world);
    for (let tick = 0; tick < DEMO_FURNACE_PERIOD; tick += 1) {
      sim.step();
    }
    expect(sim.state.itemsProduced).toBe(1);
  });

  it("publishes belt items for the renderer", () => {
    const world = new WorldStore(7);
    const sim = new DemoSim(world);
    sim.step();
    expect(world.allBeltItems.length).toBeGreaterThan(0);
  });

  it("is deterministic for the same number of steps", () => {
    const run = () => {
      const world = new WorldStore(7);
      const sim = new DemoSim(world);
      for (let tick = 0; tick < 500; tick += 1) {
        sim.step();
      }
      return {
        state: sim.state,
        items: world.allBeltItems.map((item) => `${item.x}:${item.progress.toFixed(6)}`),
      };
    };
    expect(run()).toEqual(run());
  });

  it("keeps the player entity on the predicted tile", () => {
    const world = new WorldStore(7);
    buildDemoScene(world);
    const sim = new DemoSim(world);
    const predictor = new MovePredictor(LAYOUT.spawn, () => false);
    predictor.tick(1, { dx: 1, dy: 0 });
    predictor.tick(2, { dx: 1, dy: 0 });
    sim.syncPlayer(predictor);
    expect(world.player(0)?.x).toBe(LAYOUT.spawn.x + 2);
    expect(world.player(0)?.y).toBe(LAYOUT.spawn.y);
  });
});
