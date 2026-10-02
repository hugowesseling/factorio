import { itemName, machineName, type MachineDef } from "../content";
import { setClass } from "./dom";

export interface HotbarEntry {
  readonly kind: "item" | "machine";
  readonly id: number;
  readonly label: string;
  readonly count?: number;
  readonly footprint?: { width: number; height: number };
}

export interface HotbarElements {
  readonly root: HTMLElement;
  readonly slots: readonly HTMLElement[];
}

export interface HotbarState {
  readonly entries: readonly HotbarEntry[];
  readonly selected: number;
}

export function buildHotbar(entries: readonly HotbarEntry[]): HotbarEntry[] {
  return entries.slice(0, 10);
}

export function hotbarLabel(entry: HotbarEntry): string {
  if (entry.kind === "machine") {
    return machineName(entry.id);
  }
  return itemName(entry.id);
}

export class Hotbar {
  private entries: readonly HotbarEntry[] = [];
  private selected = 0;

  constructor(private readonly elements: HotbarElements) {
    this.render({ entries: [], selected: 0 });
  }

  get selectedEntry(): HotbarEntry | undefined {
    return this.entries[this.selected];
  }

  get selectedIndex(): number {
    return this.selected;
  }

  update(state: HotbarState): void {
    const changed =
      state.selected !== this.selected ||
      state.entries.length !== this.entries.length ||
      state.entries.some((entry, index) => hotbarLabel(entry) !== hotbarLabel(this.entries[index] ?? entry));
    this.selected = state.selected;
    this.entries = state.entries;
    if (changed) {
      this.render(state);
    }
    this.select(state.selected);
  }

  private render(state: HotbarState): void {
    state.entries.forEach((entry, index) => {
      const slot = this.elements.slots[index];
      if (slot === undefined) {
        return;
      }
      slot.textContent = "";
      slot.append(slotKey(index), slotName(hotbarLabel(entry)), slotCount(entry));
      slot.dataset.kind = entry.kind;
      slot.dataset.id = String(entry.id);
    });
    this.select(this.selected);
  }

  private select(index: number): void {
    this.elements.slots.forEach((slot, position) => {
      setClass(slot, "is-selected", position === index);
    });
  }
}

function slotKey(index: number): HTMLElement {
  const element = document.createElement("span");
  element.className = "hotbar-key";
  element.textContent = (index + 1).toString();
  return element;
}

function slotName(label: string): HTMLElement {
  const element = document.createElement("span");
  element.className = "hotbar-name";
  element.textContent = label;
  return element;
}

function slotCount(entry: HotbarEntry): HTMLElement {
  const element = document.createElement("span");
  element.className = "hotbar-count";
  element.textContent = entry.count === undefined ? "" : entry.count.toLocaleString("en-US");
  return element;
}

export function machineEntry(def: MachineDef, count = 0): HotbarEntry {
  return {
    kind: "machine",
    id: def.prototype,
    label: def.key,
    count,
    footprint: { width: def.width, height: def.height },
  };
}
