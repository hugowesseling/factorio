import { BELTS, MACHINE_FURNACE, MACHINE_GENERATOR, MACHINE_LAB } from "../content";
import { EntityKind, type BeltItem, type Entity, type WorldStore } from "../state/world";
import { MovePredictor } from "../state/predict";
import type { TilePos } from "../sim/terrain";

export const DEMO_BELT_SPEED_TICKS = BELTS[0]?.speedTicks ?? 30;
export const DEMO_GENERATOR_PERIOD = 60;
export const DEMO_FURNACE_PERIOD = 120;
export const DEMO_ITEM_COAL = 0;

export interface DemoLayout {
  readonly generator: TilePos;
  readonly beltStart: TilePos;
  readonly beltLength: number;
  readonly furnace: TilePos;
  readonly inserter: TilePos;
  readonly lab: TilePos;
  readonly spawn: TilePos;
}

export const LAYOUT: DemoLayout = {
  generator: { x: -3, y: -3 },
  beltStart: { x: -3, y: 0 },
  beltLength: 8,
  furnace: { x: 6, y: -1 },
  inserter: { x: 5, y: 0 },
  lab: { x: 1, y: 4 },
  spawn: { x: 0, y: 0 },
};

export function beltTiles(layout: DemoLayout = LAYOUT): TilePos[] {
  const tiles: TilePos[] = [];
  for (let offset = 0; offset < layout.beltLength; offset += 1) {
    tiles.push({ x: layout.beltStart.x + offset, y: layout.beltStart.y });
  }
  return tiles;
}

function entity(
  kind: EntityKind,
  index: number,
  x: number,
  y: number,
  prototype: number,
  label: string,
): Entity {
  return { kind, index, x, y, prototype, rotation: 0, label };
}

export function buildDemoScene(world: WorldStore, layout: DemoLayout = LAYOUT): void {
  world.upsertEntity(
    entity(
      EntityKind.Machine,
      1,
      layout.generator.x,
      layout.generator.y,
      MACHINE_GENERATOR,
      "burner generator",
    ),
  );
  world.upsertEntity(
    entity(
      EntityKind.Machine,
      2,
      layout.furnace.x,
      layout.furnace.y,
      MACHINE_FURNACE,
      "stone furnace",
    ),
  );
  world.upsertEntity(
    entity(EntityKind.Machine, 3, layout.lab.x, layout.lab.y, MACHINE_LAB, "lab"),
  );
  world.upsertEntity(
    entity(
      EntityKind.Inserter,
      4,
      layout.inserter.x,
      layout.inserter.y,
      0,
      "inserter",
    ),
  );

  beltTiles(layout).forEach((tile, offset) => {
    world.upsertEntity(entity(EntityKind.Belt, 10 + offset, tile.x, tile.y, 0, "transport belt"));
  });

  world.upsertEntity(entity(EntityKind.Player, 0, layout.spawn.x, layout.spawn.y, 0, "player 0"));
}

export interface DemoState {
  readonly tick: number;
  readonly generatorActive: boolean;
  readonly furnaceActive: boolean;
  readonly itemsProduced: number;
}

export class DemoSim {
  private tick = 0;
  private items: BeltItem[] = [];
  private produced = 0;
  private readonly beltEnd: number;

  constructor(
    private readonly world: WorldStore,
    private readonly layout: DemoLayout = LAYOUT,
  ) {
    this.beltEnd = layout.beltStart.x + layout.beltLength;
    this.items = seedBeltItems(layout);
  }

  get state(): DemoState {
    return {
      tick: this.tick,
      generatorActive: this.tick % DEMO_GENERATOR_PERIOD < DEMO_GENERATOR_PERIOD / 2,
      furnaceActive: this.tick % DEMO_FURNACE_PERIOD < DEMO_FURNACE_PERIOD / 2,
      itemsProduced: this.produced,
    };
  }

  step(): DemoState {
    this.tick += 1;
    this.items = advanceItems(this.items, this.beltEnd);

    if (this.tick % DEMO_FURNACE_PERIOD === 0) {
      this.items = insertItem(this.items, this.layout);
      this.produced += 1;
    }

    this.world.setBeltItems(this.items);
    this.world.setStats({
      tick: this.tick,
      hash: 0,
      powerProduced: this.state.generatorActive ? generatorOutput() : 0,
      powerConsumed: labDraw(),
    });
    this.world.setSnapshot({ tick: this.tick, hash: 0 });
    return this.state;
  }

  syncPlayer(predictor: MovePredictor, index = 0): void {
    const position = predictor.predicted;
    const existing = this.world.player(index);
    this.world.upsertEntity(
      entity(
        EntityKind.Player,
        index,
        Math.round(position.x),
        Math.round(position.y),
        existing?.prototype ?? 0,
        existing?.label ?? `player ${index}`,
      ),
    );
  }
}

function generatorOutput(): number {
  return 5000;
}

function labDraw(): number {
  return 60_000;
}

export function seedBeltItems(layout: DemoLayout = LAYOUT): BeltItem[] {
  const items: BeltItem[] = [];
  for (const tile of beltTiles(layout)) {
    if ((tile.x - layout.beltStart.x) % DEMO_BELT_SPEED_TICKS === 0) {
      items.push({ x: tile.x, y: tile.y, slot: 0, item: DEMO_ITEM_COAL, progress: 0 });
    }
  }
  return items;
}

export function advanceItems(items: readonly BeltItem[], beltEnd: number): BeltItem[] {
  const step = 1 / DEMO_BELT_SPEED_TICKS;
  const next: BeltItem[] = [];
  for (const item of items) {
    const position = item.x + item.progress + step;
    const x = Math.floor(position);
    if (x >= beltEnd) {
      continue;
    }
    next.push({ ...item, x, progress: position - x });
  }
  return next;
}

export function insertItem(items: readonly BeltItem[], layout: DemoLayout = LAYOUT): BeltItem[] {
  const entrance = layout.beltStart;
  if (layout.beltLength <= 0) {
    return [...items];
  }
  if (items.some((item) => item.x === entrance.x && item.progress < 0.5)) {
    return [...items];
  }
  return [{ x: entrance.x, y: entrance.y, slot: 0, item: DEMO_ITEM_COAL, progress: 0 }, ...items];
}
