import { chunkKeyString, chunkOf, CHUNK_SIZE, type ChunkPos, type TilePos } from "../sim/terrain";

export const DEFAULT_VIEW_RADIUS = 2;

export interface AoiSettings {

  readonly viewRadius: number;
}

export function sanitizeRadius(radius: number): number {
  return Math.max(0, Math.floor(radius));
}

export function desiredChunks(center: ChunkPos, settings: AoiSettings): string[] {
  const radius = sanitizeRadius(settings.viewRadius);
  const wanted: string[] = [];
  for (let dy = -radius; dy <= radius; dy += 1) {
    for (let dx = -radius; dx <= radius; dx += 1) {
      if (Math.abs(dx) + Math.abs(dy) > radius * 2) {
        continue;
      }
      wanted.push(chunkKeyString({ x: center.x + dx, y: center.y + dy }));
    }
  }
  wanted.sort();
  return wanted;
}

export function withinAoi(a: ChunkPos, b: ChunkPos, settings: AoiSettings): boolean {
  const radius = sanitizeRadius(settings.viewRadius);
  return Math.abs(a.x - b.x) + Math.abs(a.y - b.y) <= radius * 2;
}

export function desiredChunksForTile(focus: TilePos, settings: AoiSettings): string[] {
  return desiredChunks(chunkOf(focus), settings);
}

export function staleChunks(resident: Iterable<string>, wanted: readonly string[]): string[] {
  const keep = new Set(wanted);
  const stale: string[] = [];
  for (const key of resident) {
    if (!keep.has(key)) {
      stale.push(key);
    }
  }
  stale.sort();
  return stale;
}

export function missingChunks(resident: Iterable<string>, wanted: readonly string[]): string[] {
  const have = new Set(resident);
  return wanted.filter((key) => !have.has(key));
}

export function visibleTiles(
  center: { x: number; y: number },
  viewWidth: number,
  viewHeight: number,
): { minX: number; minY: number; maxX: number; maxY: number } {
  const halfWidth = viewWidth / 2;
  const halfHeight = viewHeight / 2;
  return {
    minX: Math.floor(center.x - halfWidth),
    minY: Math.floor(center.y - halfHeight),
    maxX: Math.ceil(center.x + halfWidth) - 1,
    maxY: Math.ceil(center.y + halfHeight) - 1,
  };
}

export function chunksInBounds(bounds: {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}): string[] {
  const first = chunkOf({ x: bounds.minX, y: bounds.minY });
  const last = chunkOf({ x: bounds.maxX, y: bounds.maxY });
  const keys: string[] = [];
  for (let cy = first.y; cy <= last.y; cy += 1) {
    for (let cx = first.x; cx <= last.x; cx += 1) {
      keys.push(chunkKeyString({ x: cx, y: cy }));
    }
  }
  keys.sort();
  return keys;
}

export function chunkSide(): number {
  return CHUNK_SIZE;
}
