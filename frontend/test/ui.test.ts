import { createCamera } from "../src/render/camera";
import { EntityKind, WorldStore } from "../src/state/world";
import { formatNumber, formatRate, formatTick, setClass, setText } from "../src/ui/dom";
import { Hud, type HudElements } from "../src/ui/hud";
import { buildHotbar, Hotbar, hotbarLabel, machineEntry } from "../src/ui/hotbar";
import {
  drawMinimap,
  minimapChunkCount,
  renderInventory,
  renderTooltip,
  type InventoryElements,
  type TooltipElements,
} from "../src/ui/panels";
import { MACHINES, ITEM_COAL, ITEM_METAL_ORE } from "../src/content";
import type { ConnectionInfo } from "../src/net/connection";

function element(tag = "div"): HTMLElement {
  return document.createElement(tag);
}

function hudElements(): HudElements {
  return {
    connection: element("span"),
    connectionDetail: element("span"),
    tick: element("span"),
    tickClock: element("span"),
    power: element("span"),
    position: element("span"),
    hover: element("span"),
    mode: element("span"),
    events: element("pre"),
  };
}

const connection = (partial: Partial<ConnectionInfo> = {}): ConnectionInfo => ({
  state: "idle",
  attempt: 0,
  nextRetryMs: null,
  serverVersion: null,
  seed: null,
  lastError: null,
  roundTripMs: null,
  ...partial,
});

describe("dom helpers", () => {
  it("only writes text when it changed", () => {
    const node = element();
    let writes = 0;
    const observer = new MutationObserver(() => {
      writes += 1;
    });
    observer.observe(node, { childList: true, characterData: true, subtree: true });

    setText(node, "same");
    setText(node, "same");
    expect(node.textContent).toBe("same");
    observer.disconnect();
    expect(writes).toBeLessThanOrEqual(1);
  });

  it("toggles classes", () => {
    const node = element();
    setClass(node, "is-open", true);
    expect(node.classList.contains("is-open")).toBe(true);
    setClass(node, "is-open", false);
    expect(node.classList.contains("is-open")).toBe(false);
  });

  it("formats numbers compactly", () => {
    expect(formatNumber(0)).toBe("0");
    expect(formatNumber(999)).toBe("999");
    expect(formatNumber(12_500)).toBe("12.5k");
    expect(formatNumber(2_400_000)).toBe("2.4M");
    expect(formatNumber(Number.NaN)).toBe("-");
  });

  it("formats tick clocks and rates", () => {
    expect(formatTick(0)).toBe("0:00");
    expect(formatTick(61)).toBe("1:01");
    expect(formatRate(1200)).toBe("1,200/min");
    expect(formatRate(12_500)).toBe("12.5k/min");
  });
});

describe("Hud", () => {
  const base = {
    connection: connection(),
    stats: { tick: 0, hash: 0, powerProduced: 0, powerConsumed: 0 },
    player: { x: 0, y: 0 },
    hover: null,
    hoverTile: null,
    demo: false,
    recentEvents: [] as string[],
  };

  it("shows the connection state and marks it open", () => {
    const elements = hudElements();
    new Hud(elements).update({ ...base, connection: connection({ state: "open" }) });
    expect(elements.connection.textContent).toBe("open");
    expect(elements.connection.classList.contains("is-open")).toBe(true);
  });

  it("shows round trip time once it is known", () => {
    const elements = hudElements();
    new Hud(elements).update({
      ...base,
      connection: connection({ state: "open", roundTripMs: 42.4 }),
    });
    expect(elements.connectionDetail.textContent).toBe("42 ms round trip");
  });

  it("counts down to the next retry", () => {
    const elements = hudElements();
    new Hud(elements).update({
      ...base,
      connection: connection({ state: "reconnecting", nextRetryMs: 1500 }),
    });
    expect(elements.connectionDetail.textContent).toBe("retrying in 1.5s");
  });

  it("explains a connection that gave up", () => {
    const elements = hudElements();
    new Hud(elements).update({
      ...base,
      connection: connection({ state: "failed", attempt: 13 }),
    });
    expect(elements.connectionDetail.textContent).toBe("gave up after 13 attempts");
    expect(elements.connection.classList.contains("is-error")).toBe(true);
  });

  it("shows power with a warning when demand exceeds supply", () => {
    const elements = hudElements();
    new Hud(elements).update({
      ...base,
      stats: { tick: 1, hash: 0, powerProduced: 5000, powerConsumed: 9000 },
    });
    expect(elements.power.textContent).toContain("brownout");
  });

  it("flags a nearly saturated grid", () => {
    const elements = hudElements();
    new Hud(elements).update({
      ...base,
      stats: { tick: 1, hash: 0, powerProduced: 10_000, powerConsumed: 9500 },
    });
    expect(elements.power.textContent).toContain("saturated");
  });

  it("describes the hovered tile", () => {
    const elements = hudElements();
    const hud = new Hud(elements);
    hud.update({
      ...base,
      hover: { x: 2, y: 3 },
      hoverTile: { resource: 0, ore: 250, water: false },
    });
    expect(elements.hover.textContent).toBe("2, 3 - resource 0, 250 ore");
    expect(elements.hover.classList.contains("is-hidden")).toBe(false);

    hud.update({ ...base, hover: null, hoverTile: null });
    expect(elements.hover.textContent).toBe("");
    expect(elements.hover.classList.contains("is-hidden")).toBe(true);
  });

  it("names water when the pointer is over it", () => {
    const elements = hudElements();
    new Hud(elements).update({
      ...base,
      hover: { x: 1, y: 1 },
      hoverTile: { resource: 0, ore: 900, water: true },
    });
    expect(elements.hover.textContent).toContain("water");
  });

  it("shows the player position and the active mode", () => {
    const elements = hudElements();
    new Hud(elements).update({ ...base, player: { x: -4, y: 9 }, demo: true });
    expect(elements.position.textContent).toBe("-4, 9");
    expect(elements.mode.textContent).toBe("demo");
  });

  it("shows the newest events first", () => {
    const elements = hudElements();
    new Hud(elements).update({
      ...base,
      recentEvents: ["one", "two", "three"],
    });
    expect(elements.events.textContent?.split("\n")).toEqual(["three", "two", "one"]);
  });

  it("limits the event log it renders", () => {
    const elements = hudElements();
    const many = Array.from({ length: 30 }, (_, index) => `e${index}`);
    new Hud(elements).update({ ...base, recentEvents: many });
    expect(elements.events.textContent?.split("\n")).toHaveLength(6);
    expect(elements.events.textContent).toContain("e29");
  });
});

describe("Hotbar", () => {
  function hotbarElements(count = 10): { elements: HotbarElementsLike; slots: HTMLElement[] } {
    const slots = Array.from({ length: count }, () => element("button"));
    return { elements: { root: element(), slots }, slots };
  }
  interface HotbarElementsLike {
    readonly root: HTMLElement;
    readonly slots: readonly HTMLElement[];
  }

  const entries = [
    { kind: "item" as const, id: ITEM_COAL, label: "coal", count: 12 },
    { kind: "item" as const, id: ITEM_METAL_ORE, label: "metal", count: 0 },
    machineEntry(MACHINES[1] as (typeof MACHINES)[number], 1),
  ];

  it("keeps at most ten entries", () => {
    expect(buildHotbar(Array.from({ length: 14 }, () => entries[0] as (typeof entries)[number]))).toHaveLength(10);
  });

  it("labels items and machines from the content manifest", () => {
    expect(hotbarLabel(entries[0] as (typeof entries)[number])).toBe("Coal");
    expect(hotbarLabel(entries[2] as (typeof entries)[number])).toBe("stone furnace");
  });

  it("renders one button per entry with key, name and count", () => {
    const { elements, slots } = hotbarElements();
    new Hotbar(elements as never).update({ entries, selected: 0 });
    expect(slots[0]?.textContent).toContain("Coal");
    expect(slots[0]?.textContent).toContain("12");
    expect(slots[2]?.dataset.kind).toBe("machine");
  });

  it("marks the selected slot", () => {
    const { elements, slots } = hotbarElements();
    const hotbar = new Hotbar(elements as never);
    hotbar.update({ entries, selected: 1 });
    expect(slots[1]?.classList.contains("is-selected")).toBe(true);
    expect(slots[0]?.classList.contains("is-selected")).toBe(false);
    expect(hotbar.selectedIndex).toBe(1);
    expect(hotbar.selectedEntry?.id).toBe(ITEM_METAL_ORE);
  });

  it("moves the selection without re-rendering", () => {
    const { elements, slots } = hotbarElements();
    const hotbar = new Hotbar(elements as never);
    hotbar.update({ entries, selected: 0 });
    const before = slots[0]?.innerHTML;
    hotbar.update({ entries, selected: 2 });
    expect(slots[0]?.innerHTML).toBe(before);
    expect(slots[2]?.classList.contains("is-selected")).toBe(true);
  });

  it("has no selection when the hotbar is empty", () => {
    const { elements } = hotbarElements();
    expect(new Hotbar(elements as never).selectedEntry).toBeUndefined();
  });
});

describe("inventory", () => {
  function inventoryElements(count = 4): InventoryElements {
    return { root: element(), slots: Array.from({ length: count }, () => element("div")) };
  }

  it("names each slot and stores the item id", () => {
    const elements = inventoryElements();
    renderInventory(elements, [
      { item: ITEM_METAL_ORE, count: 250 },
      { item: ITEM_COAL, count: 8 },
    ]);
    expect(elements.slots[0]?.textContent).toBe("Metal ore");
    expect(elements.slots[0]?.title).toBe("Metal ore x250");
    expect(elements.slots[1]?.dataset.item).toBe(String(ITEM_COAL));
  });

  it("empties slots with nothing in them", () => {
    const elements = inventoryElements();
    renderInventory(elements, [{ item: ITEM_COAL, count: 0 }]);
    expect(elements.slots[0]?.classList.contains("is-empty")).toBe(true);
  });

  it("blanks unused slots", () => {
    const elements = inventoryElements();
    renderInventory(elements, [{ item: ITEM_COAL, count: 1 }]);
    expect(elements.slots[3]?.textContent).toBe("");
  });
});

describe("tooltip", () => {
  function tooltipElements(): TooltipElements {
    return { root: element(), title: element("strong"), body: element("span") };
  }

  it("is hidden until there is a tile under the pointer", () => {
    const elements = tooltipElements();
    renderTooltip(elements, null, new WorldStore(7), createCamera(800, 600));
    expect(elements.root.classList.contains("is-hidden")).toBe(true);
  });

  it("describes an ore tile by its item name", () => {
    const elements = tooltipElements();
    const world = new WorldStore(7);
    let tile: { x: number; y: number } | null = null;
    for (let y = -40; y <= 40 && tile === null; y += 1) {
      for (let x = -40; x <= 40; x += 1) {
        if (world.tile({ x, y }).ore > 0) {
          tile = { x, y };
          break;
        }
      }
    }
    renderTooltip(elements, tile, world, createCamera(800, 600));
    expect(elements.root.classList.contains("is-hidden")).toBe(false);
    expect(elements.title.textContent).toMatch(/ore|Coal|Stone/);
  });

  it("names the entity standing on the tile", () => {
    const elements = tooltipElements();
    const world = new WorldStore(7);
    world.upsertEntity({
      kind: EntityKind.Machine,
      index: 1,
      x: 3,
      y: 3,
      prototype: 1,
      rotation: 0,
      label: "stone furnace",
    });
    renderTooltip(elements, { x: 3, y: 3 }, world, createCamera(800, 600));
    expect(elements.title.textContent).toBe("stone furnace");
    expect(elements.body.textContent).toContain("tile 3, 3");
  });

  it("positions itself over the tile", () => {
    const elements = tooltipElements();
    const camera = createCamera(800, 600);
    renderTooltip(elements, { x: 0, y: 0 }, new WorldStore(7), camera);
    expect(elements.root.style.left).toBe("400px");
    expect(elements.root.style.top).toBe("300px");
  });
});

describe("minimap", () => {
  function canvas(): HTMLCanvasElement {
    const node = document.createElement("canvas");
    node.width = 64;
    node.height = 64;
    return node;
  }

  it("draws without a world chunk being resident", () => {
    const element2 = canvas();
    const world = new WorldStore(7);
    expect(() =>
      drawMinimap({ element: element2, size: 64, world, camera: createCamera(800, 600), localPlayer: 0 }),
    ).not.toThrow();
  });

  it("marks the local player and other players differently", () => {
    const element2 = canvas();
    const world = new WorldStore(7);
    world.upsertEntity({
      kind: EntityKind.Player,
      index: 0,
      x: 0,
      y: 0,
      prototype: 0,
      rotation: 0,
      label: "local",
    });
    world.upsertEntity({
      kind: EntityKind.Player,
      index: 1,
      x: 2,
      y: 2,
      prototype: 0,
      rotation: 0,
      label: "remote",
    });
    expect(() =>
      drawMinimap({ element: element2, size: 64, world, camera: createCamera(800, 600), localPlayer: 0 }),
    ).not.toThrow();
  });

  it("reports how many chunks the minimap spans", () => {
    expect(minimapChunkCount(64)).toBe(2);
    expect(minimapChunkCount(32)).toBe(1);
    expect(minimapChunkCount(33)).toBe(2);
  });
});
