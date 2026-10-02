import type { MoveCommand } from "../state/predict";
import { tileAtPixel, type Camera } from "../render/camera";
import type { TilePos } from "../sim/terrain";

export interface InputOptions {
  readonly target: EventTarget;
  readonly keyTarget?: EventTarget;
  readonly camera: Camera;
  readonly onHotbar: (slot: number) => void;
  readonly onToggleGrid: () => void;
  readonly onToggleConnection: () => void;
  readonly onCycleBuild: (direction: number) => void;
  readonly onCancel: () => void;
  readonly onAction: (action: string) => void;
}

const MOVE_KEYS: readonly string[] = [
  "KeyW",
  "KeyA",
  "KeyS",
  "KeyD",
  "ArrowUp",
  "ArrowLeft",
  "ArrowDown",
  "ArrowRight",
];

const ACTION_KEYS: Readonly<Record<string, string>> = {
  KeyE: "interact",
  KeyR: "rotate",
  KeyQ: "mine",
  KeyM: "map",
  Escape: "cancel",
  Backquote: "debug",
};

export class InputController {
  private readonly held = new Set<string>();
  private pointerX = 0;
  private pointerY = 0;
  private pointerInside = false;
  private panning = false;
  private lastPanX = 0;
  private lastPanY = 0;
  private detachers: (() => void)[] = [];

  constructor(private readonly options: InputOptions) {
    this.listen();
  }

  dispose(): void {
    for (const detach of this.detachers) {
      detach();
    }
    this.detachers = [];
    this.held.clear();
  }

  get isPanning(): boolean {
    return this.panning;
  }

  get hoveredTile(): TilePos | null {
    if (!this.pointerInside) {
      return null;
    }
    return tileAtPixel(this.options.camera, this.pointerX, this.pointerY);
  }

  get pointer(): { x: number; y: number; inside: boolean } {
    return { x: this.pointerX, y: this.pointerY, inside: this.pointerInside };
  }

  isHeld(code: string): boolean {
    return this.held.has(code);
  }

  moveIntent(): MoveCommand | null {
    const left = this.anyHeld(["KeyA", "ArrowLeft"]);
    const right = this.anyHeld(["KeyD", "ArrowRight"]);
    const up = this.anyHeld(["KeyW", "ArrowUp"]);
    const down = this.anyHeld(["KeyS", "ArrowDown"]);

    const dx = (right ? 1 : 0) - (left ? 1 : 0);
    const dy = (down ? 1 : 0) - (up ? 1 : 0);
    if (dx === 0 && dy === 0) {
      return null;
    }
    return { dx, dy };
  }

  private anyHeld(codes: readonly string[]): boolean {
    return codes.some((code) => this.held.has(code));
  }

  private listen(): void {
    const target = this.options.target;
    const keyTarget = this.options.keyTarget ?? target;
    const on = <K extends keyof WindowEventMap>(
      scope: EventTarget,
      type: K,
      handler: (event: WindowEventMap[K]) => void,
      options?: AddEventListenerOptions,
    ): void => {
      const listener = (event: Event): void => {
        handler(event as WindowEventMap[K]);
      };
      scope.addEventListener(type, listener as EventListener, options);
      this.detachers.push(() => scope.removeEventListener(type, listener as EventListener, options));
    };
    const onKeys = <K extends keyof WindowEventMap>(
      type: K,
      handler: (event: WindowEventMap[K]) => void,
    ): void => on(keyTarget, type, handler);

    onKeys("keydown", event => {
      if (isTypingTarget(event.target)) {
        return;
      }
      if (MOVE_KEYS.includes(event.code)) {
        this.held.add(event.code);
        event.preventDefault();
        return;
      }
      if (event.code === "Space") {
        event.preventDefault();
        this.options.onAction("rotate");
        return;
      }
      const digit = hotbarSlot(event.code);
      if (digit !== null) {
        this.options.onHotbar(digit);
        return;
      }
      const action = ACTION_KEYS[event.code];
      if (action !== undefined) {
        if (action === "cancel") {
          this.options.onCancel();
        } else {
          this.options.onAction(action);
        }
        return;
      }
      switch (event.code) {
        case "KeyG":
          this.options.onToggleGrid();
          break;
        case "KeyC":
          this.options.onToggleConnection();
          break;
        case "KeyF":
          this.options.onCycleBuild(1);
          break;
        case "KeyV":
          this.options.onCycleBuild(-1);
          break;
        default:
          break;
      }
    });

    onKeys("keyup", event => {
      this.held.delete(event.code);
    });

    onKeys("blur", () => {
      this.held.clear();
      this.panning = false;
    });

    on(target, "pointermove", event => {
      const rect = this.rect();
      this.pointerX = event.clientX - rect.left;
      this.pointerY = event.clientY - rect.top;
      this.pointerInside =
        this.pointerX >= 0 &&
        this.pointerY >= 0 &&
        this.pointerX <= rect.width &&
        this.pointerY <= rect.height;
      if (this.panning) {
        const dx = event.clientX - this.lastPanX;
        const dy = event.clientY - this.lastPanY;
        this.lastPanX = event.clientX;
        this.lastPanY = event.clientY;
        this.options.camera.x -= dx * this.options.camera.scale;
        this.options.camera.y -= dy * this.options.camera.scale;
      }
    });

    on(target, "pointerdown", event => {
      if (event.button === 1 || (event.button === 0 && event.shiftKey)) {
        this.panning = true;
        this.lastPanX = event.clientX;
        this.lastPanY = event.clientY;
        event.preventDefault();
        return;
      }
      if (event.button === 0) {
        const tile = this.hoveredTile;
        if (tile !== null) {
          this.options.onAction("select");
        }
      }
      if (event.button === 2) {
        this.options.onAction("remove");
      }
    });

    const endPan = (): void => {
      this.panning = false;
    };
    on(target, "pointerup", endPan);
    on(target, "pointercancel", endPan);

    on(target, "pointerleave", () => {
      this.pointerInside = false;
      this.panning = false;
    });

    on(
      target,
      "wheel",
      event => {
        event.preventDefault();
        this.options.onAction(event.deltaY < 0 ? "zoom-in" : "zoom-out");
      },
      { passive: false },
    );

    on(target, "contextmenu", event => event.preventDefault());
  }

  private rect(): { left: number; top: number; width: number; height: number } {
    const element = this.options.target as Partial<HTMLCanvasElement>;
    return element.getBoundingClientRect?.() ?? { left: 0, top: 0, width: 0, height: 0 };
  }
}

export function hotbarSlot(code: string): number | null {
  if (code.startsWith("Digit")) {
    const digit = Number.parseInt(code.slice("Digit".length), 10);
    return Number.isInteger(digit) && digit >= 0 && digit <= 9 ? digit : null;
  }
  if (code.startsWith("Numpad")) {
    const digit = Number.parseInt(code.slice("Numpad".length), 10);
    return Number.isInteger(digit) && digit >= 0 && digit <= 9 ? digit : null;
  }
  return null;
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (target === null) {
    return false;
  }
  const element = target as Partial<HTMLElement>;
  return (
    element.tagName === "INPUT" ||
    element.tagName === "TEXTAREA" ||
    element.isContentEditable === true
  );
}
