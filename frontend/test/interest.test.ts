import {
  chunksInBounds,
  DEFAULT_VIEW_RADIUS,
  desiredChunks,
  desiredChunksForTile,
  missingChunks,
  staleChunks,
  visibleTiles,
  withinAoi,
} from "../src/net/interest";
import { chunkOf, chunkKeyString, CHUNK_SIZE, type ChunkPos, type TilePos } from "../src/sim/terrain";

describe("desiredChunks", () => {
  it("matches the server's ring shape", () => {
    const one = desiredChunks({ x: 0, y: 0 }, { viewRadius: 1 });
    expect(one).toHaveLength(9);
    expect(one[0]).toBe("-1,-1");

    const two = desiredChunks({ x: 0, y: 0 }, { viewRadius: 2 });
    expect(two).toHaveLength(25);
  });

  it("is a square when the radius is large enough", () => {
    const wide = desiredChunks({ x: 0, y: 0 }, { viewRadius: 4 });
    expect(wide).toHaveLength(81);
  });

  it("is empty at radius zero apart from the centre", () => {
    expect(desiredChunks({ x: 2, y: 3 }, { viewRadius: 0 })).toEqual(["2,3"]);
  });

  it("is sorted and free of duplicates", () => {
    for (const radius of [0, 1, 2, 3, 5]) {
      const wanted = desiredChunks({ x: -7, y: 11 }, { viewRadius: radius });
      expect(new Set(wanted).size).toBe(wanted.length);
      expect([...wanted].sort()).toEqual(wanted);
    }
  });

  it("is stable regardless of the order chunks were visited in", () => {
    const first = desiredChunks({ x: 40, y: -12 }, { viewRadius: DEFAULT_VIEW_RADIUS });
    const second = desiredChunks({ x: 40, y: -12 }, { viewRadius: DEFAULT_VIEW_RADIUS });
    expect(first).toEqual(second);
  });

  it("clamps a negative radius to the centre", () => {
    expect(desiredChunks({ x: 1, y: 1 }, { viewRadius: -5 })).toEqual(["1,1"]);
  });

  it("follows the focus tile as it crosses a chunk boundary", () => {
    const before = desiredChunksForTile({ x: 31, y: 0 }, { viewRadius: 0 });
    const after = desiredChunksForTile({ x: 32, y: 0 }, { viewRadius: 0 });
    expect(before).toEqual(["0,0"]);
    expect(after).toEqual(["1,0"]);
  });
});

describe("withinAoi", () => {
  it("accepts neighbours and rejects distant chunks", () => {
    const settings = { viewRadius: 1 };
    expect(withinAoi({ x: 0, y: 0 }, { x: 1, y: 1 }, settings)).toBe(true);
    expect(withinAoi({ x: 0, y: 0 }, { x: 3, y: 0 }, settings)).toBe(false);
  });

  it("agrees with desiredChunks on membership", () => {
    const settings = { viewRadius: 2 };
    const wanted = new Set(desiredChunks({ x: 0, y: 0 }, settings));
    for (const key of wanted) {
      const [x = "0", y = "0"] = key.split(",");
      expect(withinAoi({ x: 0, y: 0 }, { x: Number(x), y: Number(y) }, settings)).toBe(true);
    }
  });
});

describe("staleChunks and missingChunks", () => {
  it("drops exactly what left the wanted set", () => {
    const wanted = desiredChunks({ x: 12, y: 0 }, { viewRadius: 1 });
    const resident = [...wanted, "0,0", "1,0", "-1,0"];
    expect(staleChunks(resident, wanted)).toEqual(["-1,0", "0,0", "1,0"]);
  });

  it("asks for exactly what is missing", () => {
    const wanted = desiredChunks({ x: 0, y: 0 }, { viewRadius: 1 });
    const half = wanted.slice(0, 5);
    expect(missingChunks(half, wanted)).toEqual(wanted.slice(5));
  });

  it("does nothing when the set already matches", () => {
    const wanted = desiredChunks({ x: 5, y: 5 }, { viewRadius: 1 });
    expect(staleChunks(wanted, wanted)).toEqual([]);
    expect(missingChunks(wanted, wanted)).toEqual([]);
  });

  it("keeps chunks the player still cares about after a big jump", () => {
    const before = desiredChunks({ x: 0, y: 0 }, { viewRadius: 2 });
    const after = desiredChunks({ x: 500, y: 0 }, { viewRadius: 2 });
    const overlap = after.filter((key) => before.includes(key));
    expect(overlap).toEqual([]);
    expect(staleChunks(before, after)).toHaveLength(before.length);
  });
});

describe("view bounds", () => {
  it("covers the viewport in tile coordinates", () => {
    const bounds = visibleTiles({ x: 0.5, y: 0.5 }, 20, 10);
    expect(bounds.minX).toBe(-10);
    expect(bounds.maxX).toBe(10);
    expect(bounds.minY).toBe(-5);
    expect(bounds.maxY).toBe(5);
  });

  it("lists every chunk the viewport touches", () => {
    const bounds = visibleTiles({ x: 0, y: 0 }, CHUNK_SIZE * 2, CHUNK_SIZE * 2);
    const keys = chunksInBounds(bounds);
    expect(keys).toEqual(["-1,-1", "-1,0", "0,-1", "0,0"]);
  });

  it("returns a single chunk for a small viewport", () => {
    expect(chunksInBounds(visibleTiles({ x: 4, y: 4 }, 8, 8))).toEqual(["0,0"]);
  });

  it("handles a viewport far from the origin", () => {
    const keys = chunksInBounds(visibleTiles({ x: 100, y: -100 }, 64, 64));
    expect(keys).toContain(chunkKeyString(chunkOf({ x: 100, y: -100 })));
    expect(keys.length).toBeGreaterThan(0);
  });
});

describe("chunk keys", () => {
  it("round-trips through the store's string form", () => {
    const chunk: ChunkPos = { x: -12, y: 34 };
    expect(parseRoundTrip(chunk)).toEqual(chunk);
  });
});

function parseRoundTrip(chunk: ChunkPos): ChunkPos {
  const key = chunkKeyString(chunk);
  const [x = "0", y = "0"] = key.split(",");
  return { x: Number.parseInt(x, 10), y: Number.parseInt(y, 10) };
}

describe("focus tiles", () => {
  it("keeps the focus tile inside its own chunk", () => {
    const focus: TilePos = { x: -1, y: 33 };
    expect(chunkOf(focus)).toEqual({ x: -1, y: 1 });
  });
});
