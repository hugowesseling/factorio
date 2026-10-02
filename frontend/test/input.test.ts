import { hotbarSlot, InputController } from "../src/input/controls";
import { createCamera } from "../src/render/camera";
import type { TilePos } from "../src/sim/terrain";

const rect = (left: number, top: number, width: number, height: number): DOMRect => ({
  left,
  top,
  width,
  height,
  right: left + width,
  bottom: top + height,
  x: left,
  y: top,
  toJSON: () => ({}),
});

interface Recorded {
  hotbar: number[];
  actions: string[];
  gridToggles: number;
  connectionToggles: number;
  builds: number[];
  cancels: number;
}

function setup(overrides: Partial<HTMLElement> = {}) {
  const target = new EventTarget() as EventTarget & Partial<HTMLElement>;
  Object.assign(target, { tagName: "CANVAS", ...overrides });

  const recorded: Recorded = {
    hotbar: [],
    actions: [],
    gridToggles: 0,
    connectionToggles: 0,
    builds: [],
    cancels: 0,
  };
  const camera = createCamera(800, 600);
  const controller = new InputController({
    target,
    camera,
    onHotbar: (slot) => recorded.hotbar.push(slot),
    onToggleGrid: () => {
      recorded.gridToggles += 1;
    },
    onToggleConnection: () => {
      recorded.connectionToggles += 1;
    },
    onCycleBuild: (direction) => recorded.builds.push(direction),
    onCancel: () => {
      recorded.cancels += 1;
    },
    onAction: (action) => recorded.actions.push(action),
  });

  const fire = (type: string, init: Record<string, unknown> = {}): void => {
    const event = new Event(type, { bubbles: true, cancelable: true });
    Object.assign(event, init);
    target.dispatchEvent(event);
  };

  return { controller, recorded, camera, fire };
}

describe("movement keys", () => {
  it("turns WASD into a movement command", () => {
    const { controller, fire } = setup();
    fire("keydown", { code: "KeyD" });
    expect(controller.moveIntent()).toEqual({ dx: 1, dy: 0 });
    fire("keydown", { code: "KeyS" });
    expect(controller.moveIntent()).toEqual({ dx: 1, dy: 1 });
  });

  it("accepts the arrow keys too", () => {
    const { controller, fire } = setup();
    fire("keydown", { code: "ArrowLeft" });
    expect(controller.moveIntent()).toEqual({ dx: -1, dy: 0 });
  });

  it("cancels opposing keys instead of jittering", () => {
    const { controller, fire } = setup();
    fire("keydown", { code: "KeyA" });
    fire("keydown", { code: "KeyD" });
    expect(controller.moveIntent()).toBeNull();
  });

  it("stops moving once the key is released", () => {
    const { controller, fire } = setup();
    fire("keydown", { code: "KeyW" });
    fire("keyup", { code: "KeyW" });
    expect(controller.moveIntent()).toBeNull();
    expect(controller.isHeld("KeyW")).toBe(false);
  });

  it("forgets every key when the window loses focus", () => {
    const { controller, fire } = setup();
    fire("keydown", { code: "KeyW" });
    fire("keydown", { code: "KeyA" });
    fire("blur");
    expect(controller.moveIntent()).toBeNull();
  });

  it("ignores keys aimed at a text field", () => {
    const { controller, recorded, fire } = setup({ tagName: "INPUT" });
    fire("keydown", { code: "KeyW" });
    expect(controller.moveIntent()).toBeNull();
    expect(recorded.actions).toHaveLength(0);
  });
});

describe("hotbar and actions", () => {
  it("maps digit keys to slots", () => {
    const { recorded, fire } = setup();
    fire("keydown", { code: "Digit1" });
    fire("keydown", { code: "Digit0" });
    fire("keydown", { code: "Numpad7" });
    expect(recorded.hotbar).toEqual([1, 0, 7]);
  });

  it("ignores digits outside the hotbar", () => {
    const { recorded, fire } = setup();
    fire("keydown", { code: "Digit11" });
    expect(recorded.hotbar).toEqual([]);
  });

  it("parses hotbar codes directly", () => {
    expect(hotbarSlot("Digit5")).toBe(5);
    expect(hotbarSlot("Numpad0")).toBe(0);
    expect(hotbarSlot("KeyA")).toBeNull();
    expect(hotbarSlot("Digit")).toBeNull();
  });

  it("routes named keys to actions", () => {
    const { recorded, fire } = setup();
    fire("keydown", { code: "KeyE" });
    fire("keydown", { code: "KeyR" });
    fire("keydown", { code: "KeyM" });
    expect(recorded.actions).toEqual(["interact", "rotate", "map"]);
  });

  it("uses space for rotation instead of scrolling the page", () => {
    const { recorded, fire } = setup();
    fire("keydown", { code: "Space" });
    expect(recorded.actions).toEqual(["rotate"]);
  });

  it("treats escape as cancel", () => {
    const { recorded, fire } = setup();
    fire("keydown", { code: "Escape" });
    expect(recorded.cancels).toBe(1);
    expect(recorded.actions).toHaveLength(0);
  });

  it("toggles the grid and the connection", () => {
    const { recorded, fire } = setup();
    fire("keydown", { code: "KeyG" });
    fire("keydown", { code: "KeyC" });
    expect(recorded.gridToggles).toBe(1);
    expect(recorded.connectionToggles).toBe(1);
  });

  it("cycles the selected building in both directions", () => {
    const { recorded, fire } = setup();
    fire("keydown", { code: "KeyF" });
    fire("keydown", { code: "KeyV" });
    expect(recorded.builds).toEqual([1, -1]);
  });
});

describe("pointer", () => {
  const canvas = (): Partial<HTMLElement> => ({
    getBoundingClientRect: () => rect(10, 20, 800, 600),
  });

  it("reports the tile under the pointer", () => {
    const { controller, fire } = setup(canvas());
    fire("pointermove", { clientX: 410, clientY: 320 });
    expect(controller.hoveredTile).toEqual({ x: 0, y: 0 });
  });

  it("has no hovered tile before the pointer enters", () => {
    const { controller } = setup(canvas());
    expect(controller.hoveredTile).toBeNull();
  });

  it("clears the hover when the pointer leaves", () => {
    const { controller, fire } = setup(canvas());
    fire("pointermove", { clientX: 410, clientY: 320 });
    fire("pointerleave");
    expect(controller.hoveredTile).toBeNull();
  });

  it("selects and removes with the mouse buttons", () => {
    const { recorded, fire } = setup(canvas());
    fire("pointermove", { clientX: 410, clientY: 320 });
    fire("pointerdown", { button: 0 });
    fire("pointerdown", { button: 2 });
    expect(recorded.actions).toEqual(["select", "remove"]);
  });

  it("pans with the middle button", () => {
    const { controller, camera, fire } = setup(canvas());
    camera.scale = 1;
    fire("pointerdown", { button: 1, clientX: 100, clientY: 100 });
    expect(controller.isPanning).toBe(true);
    fire("pointermove", { clientX: 150, clientY: 130 });
    expect(camera.x).toBeCloseTo(-50, 6);
    expect(camera.y).toBeCloseTo(-30, 6);
    fire("pointerup", { button: 1 });
    expect(controller.isPanning).toBe(false);
  });

  it("pans with shift and the left button", () => {
    const { controller, fire } = setup(canvas());
    fire("pointerdown", { button: 0, shiftKey: true, clientX: 10, clientY: 10 });
    expect(controller.isPanning).toBe(true);
  });

  it("does not select while shift panning", () => {
    const { recorded, fire } = setup(canvas());
    fire("pointermove", { clientX: 410, clientY: 320 });
    fire("pointerdown", { button: 0, shiftKey: true, clientX: 410, clientY: 320 });
    expect(recorded.actions).toHaveLength(0);
  });

  it("zooms from the wheel", () => {
    const { recorded, fire } = setup(canvas());
    fire("wheel", { deltaY: -1 });
    fire("wheel", { deltaY: 240 });
    expect(recorded.actions).toEqual(["zoom-in", "zoom-out"]);
  });
});

describe("hover mapping", () => {
  it("follows the camera as it moves", () => {
    const { controller, camera, fire } = setup({
      getBoundingClientRect: () => rect(0, 0, 800, 600),
    });
    camera.scale = 1;
    fire("pointermove", { clientX: 400, clientY: 300 });
    const before = controller.hoveredTile as TilePos;
    camera.x = 10;
    const after = controller.hoveredTile as TilePos;
    expect(after.x).toBe(before.x + 10);
  });
});

describe("keyboard scope", () => {
  it("listens for keys on the window rather than the canvas", () => {
    const canvasTarget = new EventTarget() as EventTarget & Partial<HTMLElement>;
    Object.assign(canvasTarget, { tagName: "CANVAS" });
    const keyTarget = new EventTarget();
    const actions: string[] = [];
    const controller = new InputController({
      target: canvasTarget,
      keyTarget,
      camera: createCamera(800, 600),
      onHotbar: () => {},
      onToggleGrid: () => {},
      onToggleConnection: () => {},
      onCycleBuild: () => {},
      onCancel: () => {},
      onAction: (action) => actions.push(action),
    });

    const press = (): void => {
      const event = new Event("keydown", { bubbles: true, cancelable: true });
      Object.assign(event, { code: "Space" });
      canvasTarget.dispatchEvent(event);
    };

    press();
    expect(actions).toHaveLength(0);

    const onWindow = new Event("keydown", { bubbles: true, cancelable: true });
    Object.assign(onWindow, { code: "Space" });
    keyTarget.dispatchEvent(onWindow);
    expect(actions).toEqual(["rotate"]);

    controller.dispose();
  });
});
