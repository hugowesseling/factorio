import {
  clampScale,
  createCamera,
  matricesClose,
  MAX_SCALE,
  MIN_SCALE,
  orthoMatrix,
  panCamera,
  resizeCamera,
  screenToTile,
  tileAtPixel,
  tileRect,
  tileToScreen,
  visibleTileBounds,
  zoomCamera,
} from "../src/render/camera";

describe("projection", () => {
  it("puts the camera centre in the middle of the viewport", () => {
    const camera = createCamera(800, 600);
    expect(tileToScreen(camera, { x: 0, y: 0 })).toEqual({ x: 400, y: 300 });
    expect(tileToScreen(camera, { x: 3, y: 4 })).toEqual({ x: 403, y: 304 });
  });

  it("scales tiles by the zoom level", () => {
    const camera = createCamera(800, 600);
    camera.scale = 0.5;
    const point = tileToScreen(camera, { x: 2, y: 0 });
    expect(point.x).toBeCloseTo(400 + 4, 6);
    expect(point.y).toBeCloseTo(300, 6);
  });

  it("round-trips a screen pixel through tiles", () => {
    const camera = createCamera(1024, 768);
    camera.x = -12.5;
    camera.y = 7.25;
    camera.scale = 0.75;
    for (const [x, y] of [
      [0, 0],
      [1024, 768],
      [512, 384],
      [100, 700],
    ] as const) {
      const tile = screenToTile(camera, x, y);
      const back = tileToScreen(camera, tile);
      expect(back.x).toBeCloseTo(x, 6);
      expect(back.y).toBeCloseTo(y, 6);
    }
  });

  it("round-trips with rotation applied", () => {
    const camera = createCamera(640, 480);
    camera.x = 3;
    camera.y = -2;
    camera.rotation = Math.PI / 4;
    const screen = { x: 220, y: 130 };
    const tile = screenToTile(camera, screen.x, screen.y);
    const back = tileToScreen(camera, tile);
    expect(back.x).toBeCloseTo(screen.x, 6);
    expect(back.y).toBeCloseTo(screen.y, 6);
  });

  it("finds the tile under the cursor", () => {
    const camera = createCamera(800, 600);
    camera.scale = 1 / 20;
    expect(tileAtPixel(camera, 400, 300)).toEqual({ x: 0, y: 0 });
    expect(tileAtPixel(camera, 420, 300)).toEqual({ x: 1, y: 0 });
    expect(tileAtPixel(camera, 500, 300)).toEqual({ x: 5, y: 0 });
    expect(tileAtPixel(camera, 380, 300)).toEqual({ x: -1, y: 0 });
    expect(tileAtPixel(camera, 399, 299)).toEqual({ x: -1, y: -1 });
  });

  it("gives a tile a rectangle of one tile per scale", () => {
    const camera = createCamera(800, 600);
    camera.scale = 0.5;
    const rect = tileRect(camera, { x: 1, y: 1 });
    expect(rect).toEqual({ x: 402, y: 302, width: 2, height: 2 });
  });
});

describe("panning and zooming", () => {
  it("pans by pixels scaled to tiles", () => {
    const camera = createCamera(800, 600);
    camera.scale = 0.5;
    panCamera(camera, 100, -40);
    expect(camera.x).toBeCloseTo(50, 6);
    expect(camera.y).toBeCloseTo(-20, 6);
  });

  it("keeps the focused tile still while zooming", () => {
    const camera = createCamera(800, 600);
    const before = screenToTile(camera, 250, 175);
    zoomCamera(camera, 1.5, 250, 175);
    const after = screenToTile(camera, 250, 175);
    expect(after.x).toBeCloseTo(before.x, 6);
    expect(after.y).toBeCloseTo(before.y, 6);
    expect(camera.scale).toBeCloseTo(1.5, 6);
  });

  it("clamps the zoom range", () => {
    const camera = createCamera(800, 600);
    zoomCamera(camera, 1000);
    expect(camera.scale).toBe(MAX_SCALE);
    zoomCamera(camera, 0.00001);
    expect(camera.scale).toBe(MIN_SCALE);
    expect(clampScale(0)).toBe(MIN_SCALE);
    expect(clampScale(1000)).toBe(MAX_SCALE);
  });

  it("zooms towards the centre by default", () => {
    const camera = createCamera(800, 600);
    const before = { ...camera };
    zoomCamera(camera, 2);
    expect(camera.x).toBeCloseTo(before.x, 6);
    expect(camera.y).toBeCloseTo(before.y, 6);
  });

  it("never lets the viewport collapse to zero", () => {
    const camera = createCamera(0, 0);
    resizeCamera(camera, 320, 240);
    expect(camera.viewportWidth).toBe(320);
    expect(camera.viewportHeight).toBe(240);

    resizeCamera(camera, 0, -5);
    expect(camera.viewportWidth).toBe(1);
    expect(camera.viewportHeight).toBe(1);
    expect(tileToScreen(camera, { x: 0, y: 0 })).toEqual({ x: 0.5, y: 0.5 });
  });
});

describe("visibleTileBounds", () => {
  it("covers the viewport plus a one tile pad", () => {
    const camera = createCamera(800, 600);
    camera.scale = 1;
    const bounds = visibleTileBounds(camera);
    expect(bounds.minX).toBe(-401);
    expect(bounds.maxX).toBe(401);
    expect(bounds.minY).toBe(-301);
    expect(bounds.maxY).toBe(301);
  });

  it("shrinks when zoomed in, since scale is tiles per pixel", () => {
    const camera = createCamera(800, 600);
    camera.scale = 0.25;
    const bounds = visibleTileBounds(camera, 0);
    expect(bounds.minX).toBe(-100);
    expect(bounds.maxX).toBe(100);

    camera.scale = 4;
    expect(visibleTileBounds(camera, 0).maxX).toBe(1600);
  });

  it("follows the camera", () => {
    const camera = createCamera(800, 600);
    camera.x = 1000;
    const bounds = visibleTileBounds(camera, 0);
    expect(bounds.minX).toBe(600);
    expect(bounds.maxX).toBe(1400);
  });

  it("grows when the view is rotated", () => {
    const straight = createCamera(800, 600);
    const rotated = createCamera(800, 600);
    rotated.rotation = Math.PI / 3;
    const a = visibleTileBounds(straight, 0);
    const b = visibleTileBounds(rotated, 0);
    expect(b.maxX - b.minX).toBeGreaterThan(a.maxX - a.minX);
  });
});

describe("orthoMatrix", () => {
  const at = (matrix: Float32Array, index: number): number => matrix[index] as number;

  it("is an affine 2D transform", () => {
    const matrix = orthoMatrix(createCamera(800, 600));
    expect(matrix.length).toBe(16);
    expect(at(matrix, 10)).toBe(1);
    expect(at(matrix, 15)).toBe(1);
    expect(at(matrix, 0)).toBeCloseTo(2 / 800, 6);
    expect(at(matrix, 5)).toBeCloseTo(-2 / 600, 6);
    expect(at(matrix, 12)).toBeCloseTo(0, 6);
    expect(at(matrix, 13)).toBeCloseTo(0, 6);
  });

  it("maps the camera centre to the origin of clip space", () => {
    for (const rotation of [0, 0.5, Math.PI / 3, Math.PI / 2]) {
      const camera = createCamera(800, 600);
      camera.x = 17;
      camera.y = -4;
      camera.rotation = rotation;
      const matrix = orthoMatrix(camera);
      const centreX = camera.x * at(matrix, 0) + camera.y * at(matrix, 1) + at(matrix, 12);
      const centreY = camera.x * at(matrix, 4) + camera.y * at(matrix, 5) + at(matrix, 13);
      expect(centreX).toBeCloseTo(0, 5);
      expect(centreY).toBeCloseTo(0, 5);
    }
  });

  it("agrees with tileToScreen once converted to pixels, rotated or not", () => {
    for (const rotation of [0, 0.7, Math.PI / 2, 2.4]) {
      for (const scale of [0.4, 1, 3]) {
        const camera = createCamera(900, 700);
        camera.x = -3.5;
        camera.y = 11.25;
        camera.scale = scale;
        camera.rotation = rotation;
        const matrix = orthoMatrix(camera);

        for (const tile of [
          { x: 0, y: 0 },
          { x: 12.5, y: -8.25 },
          { x: -100, y: 44 },
        ] as const) {
          const clipX = tile.x * at(matrix, 0) + tile.y * at(matrix, 1) + at(matrix, 12);
          const clipY = tile.x * at(matrix, 4) + tile.y * at(matrix, 5) + at(matrix, 13);
          const pixelsX = (clipX * 0.5 + 0.5) * 900;
          const pixelsY = (-clipY * 0.5 + 0.5) * 700;
          const expected = tileToScreen(camera, tile);
          expect(pixelsX).toBeCloseTo(expected.x, 3);
          expect(pixelsY).toBeCloseTo(expected.y, 3);
        }
      }
    }
  });

  it("keeps y pointing down, matching screen space", () => {
    const camera = createCamera(800, 600);
    const matrix = orthoMatrix(camera);
    const below = 10 * at(matrix, 5) + at(matrix, 13);
    expect(below).toBeLessThan(0);
  });

  it("matches a hand-built unrotated matrix", () => {
    const camera = createCamera(400, 400);
    camera.scale = 2;
    camera.x = 10;
    camera.y = -10;
    expect(
      matricesClose(
        orthoMatrix(camera),
        new Float32Array([1 / 400, 0, 0, 0, 0, -1 / 400, 0, 0, 0, 0, 1, 0, -0.025, -0.025, 0, 1]),
      ),
    ).toBe(true);
  });

  it("stays invertible at any zoom", () => {
    for (const scale of [MIN_SCALE, 0.5, 1, 3, MAX_SCALE]) {
      const camera = createCamera(1280, 720);
      camera.scale = scale;
      const matrix = orthoMatrix(camera);
      expect(Number.isFinite(matrix[0])).toBe(true);
      expect(matrix[0]).toBeGreaterThan(0);
    }
  });
});
