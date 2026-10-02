import { toSeed } from "./rng";

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

const MASK32 = 0xffff_ffff;
const ORE_CELL = 36;
const TYPE_CELL = 36;
const WATER_CELL = 40;
const ORE_LEVEL = 54_000;
const WATER_LEVEL = 60_000;

function floorDiv(a: number, b: number): number {
  return Math.floor(a / b);
}

function rotateLeft32(value: number, amount: number): number {
  return (((value << amount) | (value >>> (32 - amount))) >>> 0);
}

function hashCorner(seed: number, x: number, y: number): number {
  let h = seed >>> 0;
  h = (h ^ Math.imul(x | 0, 0x9e3779b1)) >>> 0;
  h = rotateLeft32(h, 13);
  h = (h ^ Math.imul(y | 0, 0x85ebca77)) >>> 0;
  h = rotateLeft32(h, 17);
  h = (h ^ (h >>> 15)) >>> 0;
  h = Math.imul(h, 0x2c1b3c6d) >>> 0;
  h = (h ^ (h >>> 12)) >>> 0;
  h = Math.imul(h, 0x297a2d39) >>> 0;
  h = (h ^ (h >>> 15)) >>> 0;
  return h;
}

function smoothstep(value: number): number {
  const t = value;
  const t2 = Math.floor((t * t) / 65536);
  const t3 = Math.floor((t2 * t) / 65536);
  const blended = 3 * t2 - 2 * t3;
  if (blended < 0) {
    return 0;
  }
  if (blended > 65535) {
    return 65535;
  }
  return blended;
}

function lerp16(a: number, b: number, t: number): number {
  const delta = b - a;
  return (a + Math.floor((delta * t) / 65536)) >>> 0;
}

export function valueNoise(seed: number, x: number, y: number, cell: number): number {
  const cx = floorDiv(x, cell);
  const cy = floorDiv(y, cell);
  const fx = x - cx * cell;
  const fy = y - cy * cell;
  const tx = smoothstep(Math.trunc((fx * 65536) / cell));
  const ty = smoothstep(Math.trunc((fy * 65536) / cell));
  const v00 = hashCorner(seed, cx, cy) >>> 16;
  const v10 = hashCorner(seed, cx + 1, cy) >>> 16;
  const v01 = hashCorner(seed, cx, cy + 1) >>> 16;
  const v11 = hashCorner(seed, cx + 1, cy + 1) >>> 16;
  const top = lerp16(v00, v10, tx);
  const bottom = lerp16(v01, v11, tx);
  return lerp16(top, bottom, ty);
}

export function generateChunk(seed: number, chunkPos: ChunkPos): TerrainChunk {
  const value = toSeed(seed);
  const low = Number(value & BigInt(MASK32));
  const high = Number((value >> 32n) & BigInt(MASK32));
  const base = (Math.imul(low, 0x9e3779b1) ^ Math.imul(high, 0x85ebca77)) >>> 0;
  const oreSeed = (base ^ 0x4f52455f) >>> 0;
  const typeSeed = (base ^ 0x54595045) >>> 0;
  const waterSeed = (base ^ 0x57415452) >>> 0;

  const data = emptyChunk();
  const origin = chunkOrigin(chunkPos);

  for (let dx = 0; dx < CHUNK_SIZE; dx += 1) {
    for (let dy = 0; dy < CHUNK_SIZE; dy += 1) {
      const index = dx * CHUNK_SIZE + dy;
      const px = origin.x + dx;
      const py = origin.y + dy;
      if (valueNoise(waterSeed, px, py, WATER_CELL) > WATER_LEVEL) {
        data.resource[index] = 0;
        data.ore[index] = 0;
        data.flags[index] = WATER_FLAG;
        continue;
      }
      const richness = valueNoise(oreSeed, px, py, ORE_CELL);
      if (richness > ORE_LEVEL) {
        const kind = valueNoise(typeSeed, px, py, TYPE_CELL) >>> 14;
        const strength = richness - ORE_LEVEL;
        const amount = 200 + Math.trunc((strength * 400) / (65535 - ORE_LEVEL));
        data.resource[index] = kind;
        data.ore[index] = amount;
        data.flags[index] = 0;
      }
    }
  }
  return data;
}

export function resourceCounts(chunk: TerrainChunk): number[] {
  const counts: number[] = new Array<number>(RESOURCE_COUNT).fill(0);
  for (let index = 0; index < CHUNK_AREA; index += 1) {
    if ((chunk.ore[index] as number) === 0) {
      continue;
    }
    const resource = chunk.resource[index] as number;
    if (resource < RESOURCE_COUNT) {
      counts[resource] = (counts[resource] as number) + 1;
    }
  }
  return counts;
}
