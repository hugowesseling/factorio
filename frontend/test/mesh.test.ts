import {
  BELT,
  beltItemQuad,
  entityColor,
  entityMesh,
  entityQuad,
  FLOATS_PER_VERTEX,
  GROUND,
  groundColor,
  gridMesh,
  highlightMesh,
  HOVER,
  itemColor,
  machineColor,
  quadsToVertices,
  terrainMesh,
  tileQuad,
  VERTS_PER_QUAD,
  WATER,
  type MeshQuad,
} from "../src/render/mesh";
import { EntityKind, WorldStore } from "../src/state/world";
import { ITEM_COAL, ITEM_METAL_ORE, ITEM_STONE_ORE, ITEM_WATER } from "../src/content";

const land = { resource: 0, ore: 0, water: false };
const stone = { resource: 2, ore: 250, water: false };
const water = { resource: 0, ore: 0, water: true };

describe("groundColor", () => {
  it("uses the plain ground colour for bare land", () => {
    expect(groundColor(land)).toBe(GROUND);
  });

  it("uses the water colour for water, whatever the resource says", () => {
    expect(groundColor(water)).toBe(WATER);
    expect(groundColor({ resource: 3, ore: 900, water: true })).toBe(WATER);
  });

  it("tints ground towards the ore colour", () => {
    const tinted = groundColor(stone);
    expect(tinted).not.toEqual(GROUND);
    expect(tinted[3]).toBe(1);
    expect(tinted[0]).toBeGreaterThan(GROUND[0]);
  });

  it("falls back to plain ground for an out of range resource", () => {
    expect(groundColor({ resource: 9, ore: 100, water: false })).toBe(GROUND);
  });

  it("falls back to plain ground when a resource has no ore", () => {
    expect(groundColor({ resource: 1, ore: 0, water: false })).toBe(GROUND);
  });
});

describe("quadsToVertices", () => {
  it("emits six vertices per quad with position and colour", () => {
    const data = quadsToVertices([tileQuad({ x: 1, y: 2 }, GROUND)]);
    expect(data).toHaveLength(VERTS_PER_QUAD * FLOATS_PER_VERTEX);
    expect(data[0]).toBe(1);
    expect(data[1]).toBe(2);
    expect(data[2]).toBeCloseTo(GROUND[0], 6);
    expect(data[3]).toBeCloseTo(GROUND[1], 6);
    expect(data[4]).toBeCloseTo(GROUND[2], 6);
    expect(data[5]).toBeCloseTo(GROUND[3], 6);
  });

  it("covers the quad's corners exactly once each", () => {
    const quad: MeshQuad = { x: 2, y: 3, width: 4, height: 5, color: GROUND };
    const data = quadsToVertices([quad]);
    const corners: string[] = [];
    for (let vertex = 0; vertex < VERTS_PER_QUAD; vertex += 1) {
      const base = vertex * FLOATS_PER_VERTEX;
      corners.push(`${data[base]},${data[base + 1]}`);
    }
    expect(new Set(corners)).toEqual(new Set(["2,3", "6,3", "2,8", "6,8"]));
  });

  it("winds triangles consistently", () => {
    const data = quadsToVertices([{ x: 0, y: 0, width: 1, height: 1, color: GROUND }]);
    const at = (vertex: number, field: number): number =>
      data[vertex * FLOATS_PER_VERTEX + field] as number;
    const first = [at(0, 0), at(0, 1), at(1, 0), at(1, 1)] as const;
    const second = [at(2, 0), at(2, 1), at(3, 0), at(3, 1)] as const;
    expect(first).toEqual([0, 0, 1, 0]);
    expect(second).toEqual([0, 1, 0, 1]);
    expect([at(4, 0), at(4, 1)]).toEqual([1, 0]);
    expect([at(5, 0), at(5, 1)]).toEqual([1, 1]);
  });

  it("produces an empty buffer for no quads", () => {
    expect(quadsToVertices([])).toHaveLength(0);
  });

  it("keeps every vertex finite", () => {
    const data = quadsToVertices(terrainMesh(new WorldStore(7), { minX: -2, minY: -2, maxX: 2, maxY: 2 }));
    expect(data.length).toBeGreaterThan(0);
    expect([...data].every(Number.isFinite)).toBe(true);
  });
});

describe("terrainMesh", () => {
  it("emits one quad per tile in the bounds", () => {
    const world = new WorldStore(7);
    const quads = terrainMesh(world, { minX: 0, minY: 0, maxX: 3, maxY: 2 });
    expect(quads).toHaveLength(6);
    expect(quads[0]).toEqual({ x: 0, y: 0, width: 1, height: 1, color: expect.any(Array) });
  });

  it("culls tiles outside the bounds", () => {
    const world = new WorldStore(7);
    expect(terrainMesh(world, { minX: -1, minY: -1, maxX: 1, maxY: 1 })).toHaveLength(4);
  });

  it("paints water tiles with the water colour", () => {
    const world = new WorldStore(7);
    let waterQuad: MeshQuad | undefined;
    for (let y = -160; y <= 160 && waterQuad === undefined; y += 1) {
      for (let x = -160; x <= 160; x += 1) {
        if (world.tile({ x, y }).water) {
          waterQuad = tileQuad({ x, y }, groundColor(world.tile({ x, y })));
          break;
        }
      }
    }
    expect(waterQuad?.color).toBe(WATER);
  });
});

describe("gridMesh", () => {
  it("draws a line per row and column", () => {
    const quads = gridMesh({ minX: 0, minY: 0, maxX: 4, maxY: 3 });
    expect(quads).toHaveLength(7);
  });

  it("scales the line thickness in tile units", () => {
    const [first] = gridMesh({ minX: 0, minY: 0, maxX: 1, maxY: 1 });
    expect(first?.width).toBeLessThan(0.1);
  });
});

describe("entity quads", () => {
  const belt = {
    kind: EntityKind.Belt,
    index: 1,
    x: 4,
    y: 5,
    prototype: 0,
    rotation: 0,
    label: "belt",
  };
  const furnace = {
    kind: EntityKind.Machine,
    index: 2,
    x: 1,
    y: 1,
    prototype: 1,
    rotation: 0,
    label: "stone furnace",
  };
  const player = {
    kind: EntityKind.Player,
    index: 0,
    x: 0,
    y: 0,
    prototype: 0,
    rotation: 0,
    label: "player 0",
  };

  it("colours each kind distinctly", () => {
    expect(entityColor(belt)).toBe(BELT);
    expect(entityColor(furnace)).toBe(machineColor(1));
    expect(entityColor(player)).not.toBe(entityColor(belt));
  });

  it("insets entities inside their tile", () => {
    const quad = entityQuad(belt, { width: 1, height: 1 });
    expect(quad.x).toBeGreaterThan(4);
    expect(quad.x + quad.width).toBeLessThan(5);
  });

  it("gives a furnace its two by two footprint", () => {
    const quad = entityQuad(furnace, { width: 2, height: 2 });
    expect(quad.width).toBeCloseTo(1.9, 6);
    expect(quad.height).toBeCloseTo(1.9, 6);
  });

  it("draws a player smaller than its tile", () => {
    const quad = entityQuad(player, { width: 1, height: 1 });
    expect(quad.width).toBeCloseTo(0.5, 6);
  });

  it("never produces a degenerate quad", () => {
    const tiny = { ...belt, prototype: 0 };
    expect(entityQuad(tiny, { width: 0.01, height: 0.01 }).width).toBeGreaterThan(0);
  });

  it("collects every entity in the world", () => {
    const world = new WorldStore(7);
    world.upsertEntity(belt);
    world.upsertEntity(furnace);
    world.upsertEntity(player);
    expect(entityMesh(world)).toHaveLength(3);
  });

  it("uses the machine footprint from the world", () => {
    const world = new WorldStore(7);
    world.upsertEntity({ ...furnace, prototype: 2 });
    const [quad] = entityMesh(world);
    expect(quad?.width).toBeCloseTo(2.9, 6);
  });
});

describe("belt items", () => {
  it("sits on the belt tile plus its progress", () => {
    const quad = beltItemQuad({ x: 10, y: 4, slot: 0, item: ITEM_METAL_ORE, progress: 0.5 });
    expect(quad.x).toBeCloseTo(10.5, 6);
    expect(quad.width).toBeGreaterThan(0);
  });

  it("offsets the quad by the belt row as well as the lane", () => {
    const onRow0 = beltItemQuad({ x: 0, y: 0, slot: 0, item: ITEM_METAL_ORE, progress: 0 });
    const onRow7 = beltItemQuad({ x: 0, y: 7, slot: 0, item: ITEM_METAL_ORE, progress: 0 });
    expect(onRow7.y - onRow0.y).toBeCloseTo(7, 6);
  });

  it("separates the two lanes", () => {
    const top = beltItemQuad({ x: 0, y: 0, slot: 0, item: ITEM_METAL_ORE, progress: 0 });
    const bottom = beltItemQuad({ x: 0, y: 0, slot: 1, item: ITEM_METAL_ORE, progress: 0 });
    expect(top.y).not.toBeCloseTo(bottom.y, 6);
  });

  it("colours items by the resource they came from", () => {
    expect(itemColor(ITEM_METAL_ORE)).not.toEqual(itemColor(ITEM_STONE_ORE));
    expect(itemColor(ITEM_COAL)).toBeDefined();
  });

  it("uses the plain item colour for items that are not ores", () => {
    expect(itemColor(ITEM_WATER)).toEqual([0.9, 0.9, 0.92, 1]);
  });
});

describe("highlightMesh", () => {
  it("draws the selection before the hover", () => {
    const quads = highlightMesh({ x: 2, y: 2 }, { x: 1, y: 1 });
    expect(quads).toHaveLength(2);
    expect(quads[0]?.x).toBe(1);
    expect(quads[1]?.color).toBe(HOVER);
  });

  it("draws nothing when the pointer is off the map", () => {
    expect(highlightMesh(null, null)).toHaveLength(0);
    expect(highlightMesh(null, { x: 0, y: 0 })).toHaveLength(1);
  });
});
