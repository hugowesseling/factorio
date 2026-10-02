import { machineSize } from "../content";
import {
  chunkKeyString,
  chunkOf,
  generateChunk,
  tileAt,
  type ChunkPos,
  type TerrainChunk,
  type TileData,
  type TilePos,
} from "../sim/terrain";

export const enum EntityKind {
  Belt = 0,
  Machine = 1,
  Inserter = 2,
  Player = 3,
}

export interface Entity {
  readonly kind: EntityKind;
  readonly index: number;
  readonly x: number;
  readonly y: number;
  readonly prototype: number;
  readonly rotation: number;
  readonly label: string;
}

export interface BeltItem {

  readonly x: number;

  readonly y: number;

  readonly slot: number;
  readonly item: number;

  readonly progress: number;
}

export interface WorldSnapshot {
  readonly tick: number;
  readonly hash: number;
}

export interface WorldStats {
  readonly tick: number;
  readonly hash: number;
  readonly powerProduced: number;
  readonly powerConsumed: number;
}

export function footprint(entity: Entity): { width: number; height: number } {
  return entity.kind === EntityKind.Machine ? machineSize(entity.prototype) : { width: 1, height: 1 };
}

function entityRank(entity: Entity): number {
  return entity.kind === EntityKind.Player ? 0 : 1 + entity.index;
}

const EMPTY_STATS: WorldStats = {
  tick: 0,
  hash: 0,
  powerProduced: 0,
  powerConsumed: 0,
};

export class WorldStore {
  readonly seed: number;
  private chunks = new Map<string, TerrainChunk>();
  private entities = new Map<number, Entity>();
  private beltItems: BeltItem[] = [];
  private players = new Map<number, Entity>();
  private stats: WorldStats = EMPTY_STATS;
  private snapshot: WorldSnapshot = { tick: 0, hash: 0 };
  private events: string[] = [];

  constructor(seed: number) {
    this.seed = seed;
  }

  chunk(chunk: ChunkPos): TerrainChunk {
    const key = chunkKeyString(chunk);
    let cached = this.chunks.get(key);
    if (cached === undefined) {
      cached = generateChunk(this.seed, chunk);
      this.chunks.set(key, cached);
    }
    return cached;
  }

  tile(pos: TilePos): TileData {
    const chunk = chunkOf(pos);
    return tileAt(this.chunk(chunk), chunk, pos);
  }

  hasChunk(chunk: ChunkPos): boolean {
    return this.chunks.has(chunkKeyString(chunk));
  }

  get residentChunks(): number {
    return this.chunks.size;
  }

  releaseChunk(chunk: ChunkPos): void {
    this.chunks.delete(chunkKeyString(chunk));
  }

  isSelectable(pos: TilePos): boolean {
    return !this.tile(pos).water;
  }

  upsertEntity(entity: Entity): void {
    const existing = this.entities.get(entity.index);
    if (existing !== undefined && existing.kind !== entity.kind) {
      this.entities.delete(entity.index);
      this.forgetIndexed(existing);
    }
    if (entity.kind === EntityKind.Player) {
      this.players.set(entity.index, entity);
    }
    this.entities.set(entity.index, entity);
  }

  private forgetIndexed(entity: Entity): void {
    if (entity.kind === EntityKind.Player) {
      this.players.delete(entity.index);
    }
  }

  removeEntity(kind: EntityKind, index: number): void {
    this.entities.delete(index);
    if (kind === EntityKind.Player) {
      this.players.delete(index);
    }
  }

  entityAt(pos: TilePos): Entity | undefined {
    let found: Entity | undefined;
    for (const entity of this.entities.values()) {
      const size = footprint(entity);
      if (entity.x <= pos.x && pos.x < entity.x + size.width && entity.y <= pos.y && pos.y < entity.y + size.height) {
        if (found === undefined || entityRank(entity) < entityRank(found)) {
          found = entity;
        }
      }
    }
    return found;
  }

  get allEntities(): IterableIterator<Entity> {
    return this.entities.values();
  }

  get allBeltItems(): readonly BeltItem[] {
    return this.beltItems;
  }

  setBeltItems(items: readonly BeltItem[]): void {
    this.beltItems = [...items];
  }

  player(index: number): Entity | undefined {
    return this.players.get(index);
  }

  get playerCount(): number {
    return this.players.size;
  }

  setStats(stats: WorldStats): void {
    this.stats = stats;
  }

  get currentStats(): WorldStats {
    return this.stats;
  }

  setSnapshot(snapshot: WorldSnapshot): void {
    this.snapshot = snapshot;
  }

  get currentSnapshot(): WorldSnapshot {
    return this.snapshot;
  }

  pushEvent(line: string): void {
    this.events.push(line);
    if (this.events.length > 64) {
      this.events.splice(0, this.events.length - 64);
    }
  }

  get recentEvents(): readonly string[] {
    return this.events;
  }
}
