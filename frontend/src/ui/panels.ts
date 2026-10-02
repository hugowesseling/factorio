import { itemName, itemForResource } from "../content";
import { setText } from "./dom";
import { tileRect } from "../render/camera";
import type { Camera } from "../render/camera";
import type { WorldStore } from "../state/world";
import { EntityKind } from "../state/world";
import { CHUNK_SIZE, type TilePos } from "../sim/terrain";

export interface InventorySlot {
  readonly item: number;
  readonly count: number;
}

export interface InventoryElements {
  readonly root: HTMLElement;
  readonly slots: readonly HTMLElement[];
}

export function renderInventory(
  elements: InventoryElements,
  slots: readonly InventorySlot[],
): void {
  elements.slots.forEach((element, index) => {
    const slot = slots[index];
    if (slot === undefined) {
      element.textContent = "";
      element.dataset.item = "";
      return;
    }
    const label = itemName(slot.item);
    setText(element, label);
    element.dataset.item = String(slot.item);
    element.title = `${label} x${slot.count}`;
    element.classList.toggle("is-empty", slot.count <= 0);
  });
}

export interface TooltipElements {
  readonly root: HTMLElement;
  readonly title: HTMLElement;
  readonly body: HTMLElement;
}

export function renderTooltip(
  elements: TooltipElements,
  tile: TilePos | null,
  world: WorldStore,
  camera: Camera,
): void {
  if (tile === null) {
    elements.root.classList.add("is-hidden");
    return;
  }
  elements.root.classList.remove("is-hidden");

  const data = world.tile(tile);
  const entity = world.entityAt(tile);
  const lines: string[] = [];

  if (entity !== undefined && entity.kind !== EntityKind.Belt) {
    elements.title.textContent = entity.label;
  } else if (data.water) {
    elements.title.textContent = "Water";
  } else if (data.ore > 0) {
    const item = itemForResource(data.resource);
    elements.title.textContent = item >= 0 ? itemName(item) : `Resource ${data.resource}`;
  } else {
    elements.title.textContent = "Ground";
  }

  lines.push(`tile ${tile.x}, ${tile.y}`);
  if (!data.water && data.ore > 0) {
    lines.push(`amount ${data.ore}`);
  }
  if (entity !== undefined) {
    lines.push(entity.kind === EntityKind.Belt ? "transport belt" : entity.label);
  }
  setText(elements.body, lines.join("\n"));

  const rect = tileRect(camera, tile);
  elements.root.style.left = `${Math.round(rect.x)}px`;
  elements.root.style.top = `${Math.round(rect.y)}px`;
}

export interface MinimapOptions {
  readonly element: HTMLCanvasElement;
  readonly size: number;
  readonly world: WorldStore;
  readonly camera: Camera;
  readonly localPlayer: number;
}

export function drawMinimap(options: MinimapOptions): void {
  const { element, size, world, camera } = options;
  const context = element.getContext("2d");
  if (context === null) {
    return;
  }
  const tiles = size;
  const originX = Math.floor(camera.x - tiles / 2);
  const originY = Math.floor(camera.y - tiles / 2);
  const cell = element.width / tiles;

  context.clearRect(0, 0, element.width, element.height);
  for (let y = 0; y < tiles; y += 1) {
    for (let x = 0; x < tiles; x += 1) {
      const tile = { x: originX + x, y: originY + y };
      const data = world.tile(tile);
      context.fillStyle = data.water
        ? "#1a4a7a"
        : data.ore > 0
          ? "#4a4f52"
          : "#2f342d";
      context.fillRect(x * cell, y * cell, cell, cell);
    }
  }

  for (const entity of world.allEntities) {
    const screenX = (entity.x + 0.5 - originX) * cell;
    const screenY = (entity.y + 0.5 - originY) * cell;
    if (entity.kind === EntityKind.Player) {
      context.fillStyle = entity.index === options.localPlayer ? "#f2554a" : "#8ab4f8";
      context.beginPath();
      context.arc(screenX, screenY, Math.max(1.5, cell * 0.6), 0, Math.PI * 2);
      context.fill();
      continue;
    }
    context.fillStyle = entity.kind === EntityKind.Machine ? "#c9a227" : "#7a7f86";
    context.fillRect(screenX - cell * 0.4, screenY - cell * 0.4, cell * 0.8, cell * 0.8);
  }

  const viewX = (camera.viewportWidth / 2 / camera.scale) * cell;
  const viewY = (camera.viewportHeight / 2 / camera.scale) * cell;
  context.strokeStyle = "rgba(255,255,255,0.5)";
  context.lineWidth = 1;
  context.strokeRect(viewX - viewX / 2, viewY - viewY / 2, viewX, viewY);
}

export const MINIMAP_TILES = 64;

export function minimapChunkCount(tiles: number): number {
  return Math.ceil(tiles / CHUNK_SIZE);
}
