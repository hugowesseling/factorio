import { machineSize, resourceForItem } from "../content";
import type { Rect } from "./camera";
import type { BeltItem, Entity, WorldStore } from "../state/world";
import { EntityKind } from "../state/world";
import { RESOURCE_COUNT, type TileData, type TilePos } from "../sim/terrain";

export const FLOATS_PER_VERTEX = 7;
export const VERTS_PER_QUAD = 6;

export type Color = readonly [number, number, number, number];

export interface MeshQuad {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  readonly color: Color;
}

export const GROUND: Color = [0.16, 0.18, 0.15, 1];
export const WATER: Color = [0.1, 0.28, 0.52, 1];
export const ORE_TINTS: readonly Color[] = [
  [0.55, 0.57, 0.62, 1],
  [0.62, 0.36, 0.2, 1],
  [0.42, 0.42, 0.44, 1],
  [0.16, 0.15, 0.16, 1],
];
export const HOVER: Color = [1, 1, 1, 0.28];
export const SELECTED: Color = [0.95, 0.85, 0.3, 0.45];
export const GRID: Color = [0, 0, 0, 0.12];
export const BELT: Color = [0.28, 0.29, 0.33, 1];
export const INSERTER: Color = [0.7, 0.55, 0.2, 1];
export const PLAYER: Color = [0.95, 0.35, 0.3, 1];
export const PLAYER_NAME: Color = [1, 1, 1, 1];
export const BELT_ITEM: Color = [0.9, 0.9, 0.92, 1];

const MACHINE_TINTS: readonly Color[] = [
  [0.45, 0.42, 0.38, 1],
  [0.5, 0.4, 0.36, 1],
  [0.36, 0.44, 0.52, 1],
  [0.3, 0.46, 0.56, 1],
  [0.5, 0.46, 0.6, 1],
];

export function groundColor(tile: TileData): Color {
  if (tile.water) {
    return WATER;
  }
  if (tile.ore <= 0 || tile.resource >= RESOURCE_COUNT) {
    return GROUND;
  }
  const tint = ORE_TINTS[tile.resource] as Color;
  return [
    GROUND[0] * 0.6 + tint[0] * 0.4,
    GROUND[1] * 0.6 + tint[1] * 0.4,
    GROUND[2] * 0.6 + tint[2] * 0.4,
    1,
  ];
}

export function machineColor(prototype: number): Color {
  return MACHINE_TINTS[prototype] ?? [0.5, 0.5, 0.5, 1];
}

export function tileQuad(tile: TilePos, color: Color): MeshQuad {
  return { x: tile.x, y: tile.y, width: 1, height: 1, color };
}

export function entityColor(entity: Entity): Color {
  if (entity.kind === EntityKind.Player) {
    return PLAYER;
  }
  if (entity.kind === EntityKind.Inserter) {
    return INSERTER;
  }
  if (entity.kind === EntityKind.Machine) {
    return machineColor(entity.prototype);
  }
  return BELT;
}

export function entityQuad(entity: Entity, footprint: Size = { width: 1, height: 1 }): MeshQuad {
  const inset = entity.kind === EntityKind.Player ? 0.25 : 0.05;
  return {
    x: entity.x + inset,
    y: entity.y + inset,
    width: Math.max(0.05, footprint.width - inset * 2),
    height: Math.max(0.05, footprint.height - inset * 2),
    color: entityColor(entity),
  };
}

export function quadsToVertices(quads: readonly MeshQuad[]): Float32Array {
  const data = new Float32Array(quads.length * VERTS_PER_QUAD * FLOATS_PER_VERTEX);
  let offset = 0;
  for (const quad of quads) {
    const left = quad.x;
    const right = quad.x + quad.width;
    const top = quad.y;
    const bottom = quad.y + quad.height;
    const corners: readonly (readonly [number, number])[] = [
      [left, top],
      [right, top],
      [left, bottom],
      [left, bottom],
      [right, top],
      [right, bottom],
    ];
    for (const [x, y] of corners) {
      data[offset] = x;
      data[offset + 1] = y;
      data[offset + 2] = quad.color[0];
      data[offset + 3] = quad.color[1];
      data[offset + 4] = quad.color[2];
      data[offset + 5] = quad.color[3];
      data[offset + 6] = 0;
      offset += FLOATS_PER_VERTEX;
    }
  }
  return data;
}

export function vertexCount(quads: readonly MeshQuad[]): number {
  return quads.length * VERTS_PER_QUAD;
}

export function terrainMesh(world: WorldStore, bounds: Rect): MeshQuad[] {
  const quads: MeshQuad[] = [];
  for (let y = bounds.minY; y < bounds.maxY; y += 1) {
    for (let x = bounds.minX; x < bounds.maxX; x += 1) {
      quads.push(tileQuad({ x, y }, groundColor(world.tile({ x, y }))));
    }
  }
  return quads;
}

export function gridMesh(bounds: Rect, step = 1): MeshQuad[] {
  const quads: MeshQuad[] = [];
  for (let x = bounds.minX; x < bounds.maxX; x += step) {
    quads.push({ x, y: bounds.minY, width: 1 / 32, height: bounds.maxY - bounds.minY, color: GRID });
  }
  for (let y = bounds.minY; y < bounds.maxY; y += step) {
    quads.push({ x: bounds.minX, y, width: bounds.maxX - bounds.minX, height: 1 / 32, color: GRID });
  }
  return quads;
}

export function entityMesh(world: WorldStore): MeshQuad[] {
  const quads: MeshQuad[] = [];
  for (const entity of world.allEntities) {
    quads.push(
      entityQuad(
        entity,
        entity.kind === EntityKind.Machine ? machineSize(entity.prototype) : ONE,
      ),
    );
  }
  return quads;
}

export interface Size {
  readonly width: number;
  readonly height: number;
}

const ONE: Size = { width: 1, height: 1 };

export function beltItemQuad(item: BeltItem): MeshQuad {
  return {
    x: item.x + item.progress,
    y: item.y + (item.slot === 0 ? 0.28 : 0.56),
    width: 0.16,
    height: 0.16,
    color: itemColor(item.item),
  };
}

export function beltItemMesh(items: readonly BeltItem[]): MeshQuad[] {
  return items.map(beltItemQuad);
}

export function itemColor(item: number): Color {
  const resource = resourceForItem(item);
  if (resource >= 0 && resource < ORE_TINTS.length) {
    const tint = ORE_TINTS[resource] as Color;
    return [Math.min(1, tint[0] + 0.25), Math.min(1, tint[1] + 0.25), Math.min(1, tint[2] + 0.25), 1];
  }
  return BELT_ITEM;
}

export function highlightMesh(tile: TilePos | null, selected: TilePos | null): MeshQuad[] {
  const quads: MeshQuad[] = [];
  if (selected !== null) {
    quads.push(tileQuad(selected, SELECTED));
  }
  if (tile !== null) {
    quads.push(tileQuad(tile, HOVER));
  }
  return quads;
}
