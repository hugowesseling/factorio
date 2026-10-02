import type { TilePos } from "../sim/terrain";

export interface Camera {

  x: number;
  y: number;

  scale: number;
  viewportWidth: number;
  viewportHeight: number;
  rotation: number;
}

export interface Rect {
  readonly minX: number;
  readonly minY: number;
  readonly maxX: number;
  readonly maxY: number;
}

export const MIN_SCALE = 1 / 16;
export const MAX_SCALE = 8;

export function createCamera(width: number, height: number): Camera {
  return { x: 0, y: 0, scale: 1, viewportWidth: width, viewportHeight: height, rotation: 0 };
}

export function clampScale(scale: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}

export function resizeCamera(camera: Camera, width: number, height: number): void {
  camera.viewportWidth = Math.max(1, width);
  camera.viewportHeight = Math.max(1, height);
}

export function panCamera(camera: Camera, dxPixels: number, dyPixels: number): void {
  camera.x += dxPixels * camera.scale;
  camera.y += dyPixels * camera.scale;
}

export function zoomCamera(
  camera: Camera,
  factor: number,
  focusX = camera.viewportWidth / 2,
  focusY = camera.viewportHeight / 2,
): void {
  const before = screenToTile(camera, focusX, focusY);
  camera.scale = clampScale(camera.scale * factor);
  const after = screenToTile(camera, focusX, focusY);
  camera.x += before.x - after.x;
  camera.y += before.y - after.y;
}

export function centerOn(camera: Camera, tile: TilePos): void {
  camera.x = tile.x;
  camera.y = tile.y;
}

export function tileToScreen(camera: Camera, tile: TilePos): { x: number; y: number } {
  const offsetX = (tile.x - camera.x) / camera.scale;
  const offsetY = (tile.y - camera.y) / camera.scale;
  const angle = -camera.rotation;
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  const halfWidth = camera.viewportWidth / 2;
  const halfHeight = camera.viewportHeight / 2;
  return {
    x: halfWidth + offsetX * cos - offsetY * sin,
    y: halfHeight + offsetX * sin + offsetY * cos,
  };
}

export function screenToTile(camera: Camera, x: number, y: number): TilePos {
  const halfWidth = camera.viewportWidth / 2;
  const halfHeight = camera.viewportHeight / 2;
  const dx = x - halfWidth;
  const dy = y - halfHeight;
  const angle = camera.rotation;
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  const offsetX = dx * cos - dy * sin;
  const offsetY = dx * sin + dy * cos;
  return { x: camera.x + offsetX * camera.scale, y: camera.y + offsetY * camera.scale };
}

export function tileAtPixel(camera: Camera, x: number, y: number): TilePos {
  const centre = screenToTile(camera, x, y);
  return { x: Math.floor(centre.x), y: Math.floor(centre.y) };
}

export function visibleTileBounds(camera: Camera, pad = 1): Rect {
  const corners: TilePos[] = [
    screenToTile(camera, 0, 0),
    screenToTile(camera, camera.viewportWidth, 0),
    screenToTile(camera, 0, camera.viewportHeight),
    screenToTile(camera, camera.viewportWidth, camera.viewportHeight),
  ];
  let minX = corners[0]?.x ?? 0;
  let maxX = minX;
  let minY = corners[0]?.y ?? 0;
  let maxY = minY;
  for (const corner of corners) {
    minX = Math.min(minX, corner.x);
    maxX = Math.max(maxX, corner.x);
    minY = Math.min(minY, corner.y);
    maxY = Math.max(maxY, corner.y);
  }
  return {
    minX: Math.floor(minX) - pad,
    minY: Math.floor(minY) - pad,
    maxX: Math.ceil(maxX) + pad,
    maxY: Math.ceil(maxY) + pad,
  };
}

export function tileRect(camera: Camera, tile: TilePos): {
  x: number;
  y: number;
  width: number;
  height: number;
} {
  const topLeft = tileToScreen(camera, tile);
  const bottomRight = tileToScreen(camera, { x: tile.x + 1, y: tile.y + 1 });
  return {
    x: Math.min(topLeft.x, bottomRight.x),
    y: Math.min(topLeft.y, bottomRight.y),
    width: Math.abs(bottomRight.x - topLeft.x),
    height: Math.abs(bottomRight.y - topLeft.y),
  };
}

export function orthoMatrix(camera: Camera): Float32Array {
  const halfWidth = (camera.viewportWidth / 2) * camera.scale;
  const halfHeight = (camera.viewportHeight / 2) * camera.scale;
  const left = camera.x - halfWidth;
  const right = camera.x + halfWidth;
  const top = camera.y - halfHeight;
  const bottom = camera.y + halfHeight;
  const cos = Math.cos(camera.rotation);
  const sin = Math.sin(camera.rotation);
  const sx = 2 / (right - left);
  const sy = -2 / (bottom - top);
  const matrix = new Float32Array(16);
  matrix[0] = sx * cos;
  matrix[1] = sx * sin;
  matrix[4] = -sy * sin;
  matrix[5] = sy * cos;
  matrix[10] = 1;
  matrix[12] = -(camera.x * sx * cos + camera.y * sx * sin);
  matrix[13] = camera.x * sy * sin - camera.y * sy * cos;
  matrix[15] = 1;
  return matrix;
}

export function matricesClose(a: Float32Array, b: Float32Array, epsilon = 1e-5): boolean {
  if (a.length !== b.length) {
    return false;
  }
  return a.every((value, index) => Math.abs(value - (b[index] as number)) <= epsilon);
}
