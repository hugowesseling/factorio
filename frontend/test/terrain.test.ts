import vectors from "./vectors.json";

import { Pcg32 } from "../src/sim/rng";
import {
  CHUNK_AREA,
  chunkIndex,
  chunkOf,
  chunkOrigin,
  generateChunk,
  resourceCounts,
  tileAt,
  WATER_FLAG,
  type ChunkPos,
  type TilePos,
} from "../src/sim/terrain";

const pcgVectors = vectors.pcg32 as Record<string, string[]>;
type TileVector = [number, number, number];
const chunkVectors = vectors.chunks as unknown as Record<string, TileVector[]>;

describe("Pcg32", () => {
  for (const [seed, expected] of Object.entries(pcgVectors)) {
    it(`produces the Rust sequence for seed ${seed}`, () => {
      const rng = new Pcg32(Number.parseInt(seed, 10));
      const actual = Array.from({ length: expected.length }, () =>
        rng.nextU32().toString(16).padStart(8, "0"),
      );
      expect(actual).toEqual(expected);
    });
  }

  it("is independent of construction order", () => {
    const a = new Pcg32(1234);
    const b = new Pcg32(1234);
    for (let i = 0; i < 100; i += 1) {
      expect(a.nextU32()).toBe(b.nextU32());
    }
  });

  it("clamps range() when max <= min", () => {
    const rng = new Pcg32(1);
    expect(rng.range(5, 5)).toBe(5);
    expect(rng.range(5, 2)).toBe(5);
  });
});

describe("generateChunk", () => {
  const cases = Object.entries(chunkVectors).map(([key, tiles]) => {
    const [seed = "0", cx = "0", cy = "0"] = key.replace(/^s/, "").split(":");
    return {
      key,
      seed: Number.parseInt(seed, 10),
      chunk: { x: Number.parseInt(cx, 10), y: Number.parseInt(cy, 10) },
      tiles,
    };
  });

  for (const { key, seed, chunk, tiles } of cases) {
    it(`matches Rust for ${key}`, () => {
      const generated = generateChunk(seed, chunk);
      const actual: [number, number, number][] = [];
      for (let index = 0; index < tiles.length; index += 1) {
        actual.push([
          generated.resource[index] as number,
          generated.ore[index] as number,
          generated.flags[index] as number,
        ]);
      }
      expect(actual).toEqual(tiles);
    });

    it(`is stable across calls for ${key}`, () => {
      const first = generateChunk(seed, chunk);
      const second = generateChunk(seed, chunk);
      expect(Array.from(first.resource)).toEqual(Array.from(second.resource));
      expect(Array.from(first.ore)).toEqual(Array.from(second.ore));
      expect(Array.from(first.flags)).toEqual(Array.from(second.flags));
    });
  }

  it("does not depend on how many chunks were generated first", () => {
    const order: [ChunkPos, number][] = [
      [{ x: 40, y: 40 }, 1],
      [{ x: -1, y: -1 }, 3],
      [{ x: 1, y: -2 }, 7],
    ];
    const forwards = order.map(([chunk, seed]) => generateChunk(seed, chunk).ore[500]);
    const backwards = order
      .slice()
      .reverse()
      .map(([chunk, seed]) => generateChunk(seed, chunk).ore[500]);
    expect(forwards).toEqual(backwards.slice().reverse());
  });

  it("never puts water and ore on the same tile", () => {
    for (const { seed, chunk } of cases) {
      const generated = generateChunk(seed, chunk);
      for (let index = 0; index < CHUNK_AREA; index += 1) {
        const water = ((generated.flags[index] as number) & WATER_FLAG) !== 0;
        const ore = generated.ore[index] as number;
        if (water) {
          expect(ore).toBe(0);
          expect(generated.resource[index]).toBe(0);
        } else if (ore > 0) {
          expect(ore).toBeGreaterThanOrEqual(200);
          expect(ore).toBeLessThanOrEqual(600);
          expect(generated.resource[index]).toBeLessThan(4);
        } else {
          expect(generated.resource[index]).toBe(0);
        }
      }
    }
  });

  it("keeps every resource id inside the declared range", () => {
    const generated = generateChunk(7, { x: 0, y: 0 });
    for (let index = 0; index < CHUNK_AREA; index += 1) {
      expect(generated.resource[index]).toBeLessThan(4);
    }
  });

  it("counts only ore tiles as resources", () => {
    const generated = generateChunk(7, { x: 0, y: 0 });
    const counts = resourceCounts(generated);
    const sum = counts.reduce((total, value) => total + value, 0);
    let ore = 0;
    for (let index = 0; index < CHUNK_AREA; index += 1) {
      if ((generated.ore[index] as number) > 0) {
        ore += 1;
      }
    }
    expect(sum).toBe(ore);
  });
});

describe("chunk coordinates", () => {
  it("maps tiles to chunks with floor division, including negatives", () => {
    const cases: [TilePos, ChunkPos][] = [
      [{ x: 0, y: 0 }, { x: 0, y: 0 }],
      [{ x: 31, y: 31 }, { x: 0, y: 0 }],
      [{ x: 32, y: 32 }, { x: 1, y: 1 }],
      [{ x: -1, y: -1 }, { x: -1, y: -1 }],
      [{ x: -32, y: -32 }, { x: -1, y: -1 }],
      [{ x: -33, y: -33 }, { x: -2, y: -2 }],
    ];
    for (const [pos, chunk] of cases) {
      expect(chunkOf(pos)).toEqual(chunk);
    }
  });

  it("round-trips tile to chunk and back", () => {
    for (const pos of [{ x: -70, y: 12 }, { x: 0, y: 0 }, { x: 99, y: -40 }]) {
      const chunk = chunkOf(pos);
      const origin = chunkOrigin(chunk);
      expect(chunkOf(origin)).toEqual(chunk);
      expect(chunkIndex(chunk, pos)).toBeGreaterThanOrEqual(0);
      expect(chunkIndex(chunk, pos)).toBeLessThan(CHUNK_AREA);
    }
  });

  it("indexes x-major, matching the backend's chunk_index", () => {
    const chunk: ChunkPos = { x: -1, y: -1 };
    expect(chunkIndex(chunk, { x: -32, y: -32 })).toBe(0);
    expect(chunkIndex(chunk, { x: -32, y: -31 })).toBe(1);
    expect(chunkIndex(chunk, { x: -31, y: -32 })).toBe(32);
    expect(chunkIndex(chunk, { x: -1, y: -1 })).toBe(1023);
  });

  it("reads back the tile it wrote", () => {
    const chunk = generateChunk(3, { x: 2, y: -5 });
    const pos: TilePos = { x: 70, y: -150 };
    const tile = tileAt(chunk, chunkOf(pos), pos);
    const index = chunkIndex(chunkOf(pos), pos);
    expect(tile.resource).toBe(chunk.resource[index]);
    expect(tile.ore).toBe(chunk.ore[index]);
    expect(tile.water).toBe(((chunk.flags[index] as number) & WATER_FLAG) !== 0);
  });
});
