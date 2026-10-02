import { chunkKey, Pcg32, rotateLeft, RNG_GOLDEN, toSeed } from "./rng";

export const CHUNK_SIZE = 32;
export const CHUNK_AREA = CHUNK_SIZE * CHUNK_SIZE;

export const WATER_FLAG = 1 << 0;
export const CLIFF_FLAG = 1 << 1;

export const RESOURCE_COUNT = 4;

export interface ChunkPos {
  readonly x: number;
  readonly y: number;
}

export interface TilePos {
  readonly x: number;
  readonly y: number;
}

export interface TileData {
  readonly resource: number;
  readonly ore: number;
  readonly water: boolean;
}

export interface TerrainChunk {
  readonly resource: Uint8Array;
  readonly ore: Uint16Array;
  readonly flags: Uint16Array;
}

export function chunkOf(pos: TilePos): ChunkPos {
  return {
    x: Math.floor(pos.x / CHUNK_SIZE),
    y: Math.floor(pos.y / CHUNK_SIZE),
  };
}

export function chunkOrigin(chunk: ChunkPos): TilePos {
  return { x: chunk.x * CHUNK_SIZE, y: chunk.y * CHUNK_SIZE };
}

export function chunkIndex(chunk: ChunkPos, pos: TilePos): number {
  return (pos.x - chunk.x * CHUNK_SIZE) * CHUNK_SIZE + (pos.y - chunk.y * CHUNK_SIZE);
}

export function chunkKeyString(chunk: ChunkPos): string {
  return `${chunk.x},${chunk.y}`;
}

export function parseChunkKey(key: string): ChunkPos {
  const [x = "0", y = "0"] = key.split(",");
  return { x: Number.parseInt(x, 10), y: Number.parseInt(y, 10) };
}

export function emptyChunk(): TerrainChunk {
  return {
    resource: new Uint8Array(CHUNK_AREA),
    ore: new Uint16Array(CHUNK_AREA),
    flags: new Uint16Array(CHUNK_AREA),
  };
}

export function tileAt(chunk: TerrainChunk, chunkPos: ChunkPos, pos: TilePos): TileData {
  const index = chunkIndex(chunkPos, pos);
  return {
    resource: chunk.resource[index] as number,
    ore: chunk.ore[index] as number,
    water: ((chunk.flags[index] as number) & WATER_FLAG) !== 0,
  };
}

export function generateChunk(seed: number, chunkPos: ChunkPos): TerrainChunk {
  const mixed = rotateLeft(chunkKey(chunkPos.x, chunkPos.y) * RNG_GOLDEN, 17n);
  const rng = new Pcg32(toSeed(seed) ^ mixed);
  const data = emptyChunk();

  for (let dx = 0; dx < CHUNK_SIZE; dx += 1) {
    for (let dy = 0; dy < CHUNK_SIZE; dy += 1) {
      const index = dx * CHUNK_SIZE + dy;
      const roll = rng.nextU32();
      if (roll % 29 === 0) {
        data.resource[index] = 0;
        data.ore[index] = 0;
        data.flags[index] = WATER_FLAG;
      } else {
        data.resource[index] = (roll / 29) % RESOURCE_COUNT;
        data.ore[index] = 200 + (rng.nextU32() % 300);
        data.flags[index] = 0;
      }
    }
  }
  return data;
}

export function resourceCounts(chunk: TerrainChunk): number[] {
  const counts: number[] = new Array<number>(RESOURCE_COUNT).fill(0);
  for (let index = 0; index < CHUNK_AREA; index += 1) {
    if (((chunk.flags[index] as number) & WATER_FLAG) !== 0) {
      continue;
    }
    const resource = chunk.resource[index] as number;
    if (resource < RESOURCE_COUNT) {
      counts[resource] = (counts[resource] as number) + 1;
    }
  }
  return counts;
}
