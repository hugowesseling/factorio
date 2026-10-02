import manifest from "./generated/content.json";

export interface ItemDef {
  readonly id: number;
  readonly key: string;
  readonly maxStack: number;
  readonly name: string;
}

export interface MachineDef {
  readonly prototype: number;
  readonly key: string;
  readonly width: number;
  readonly height: number;
  readonly maxPower: number;
}

export interface BeltDef {
  readonly id: number;
  readonly key: string;
  readonly speedTicks: number;
}

export interface InserterDef {
  readonly id: number;
  readonly key: string;
  readonly swingTicks: number;
}

export interface RecipeDef {
  readonly id: number;
  readonly key: string;
}

export interface TechDef {
  readonly id: number;
  readonly key: string;
}

export const ITEM_COAL = 0;
export const ITEM_COPPER_ORE = 1;
export const ITEM_COPPER_PLATE = 2;
export const ITEM_GEAR = 3;
export const ITEM_METAL_ORE = 4;
export const ITEM_METAL_PLATE = 5;
export const ITEM_SCIENCE_PACK = 6;
export const ITEM_STONE_BRICK = 7;
export const ITEM_STONE_ORE = 8;
export const ITEM_WATER = 9;

export const MACHINE_GENERATOR = 0;
export const MACHINE_FURNACE = 1;
export const MACHINE_ASSEMBLER = 2;
export const MACHINE_PUMP = 3;
export const MACHINE_LAB = 4;

type ItemRow = readonly [number, string, number, string];
type MachineRow = readonly [number, string, number, number, number];
type NamedRow = readonly [number, string, number];

const rawItems = manifest.items as unknown as readonly ItemRow[];
const rawMachines = manifest.machines as unknown as readonly MachineRow[];
const rawBelts = manifest.belts as unknown as readonly NamedRow[];
const rawInserters = manifest.inserters as unknown as readonly NamedRow[];
const rawRecipes = manifest.recipes as unknown as readonly (readonly [number, string])[];
const rawTechs = manifest.technologies as unknown as readonly (readonly [number, string])[];

export const ITEMS: readonly ItemDef[] = rawItems.map(([id, key, maxStack, name]) => ({
  id,
  key,
  maxStack,
  name,
}));

export const MACHINES: readonly MachineDef[] = rawMachines.map(
  ([prototype, key, width, height, maxPower]) => ({
    prototype,
    key,
    width,
    height,
    maxPower,
  }),
);

export const BELTS: readonly BeltDef[] = rawBelts.map(([id, key, speedTicks]) => ({
  id,
  key,
  speedTicks,
}));

export const INSERTERS: readonly InserterDef[] = rawInserters.map(([id, key, swingTicks]) => ({
  id,
  key,
  swingTicks,
}));

export const RECIPES: readonly RecipeDef[] = rawRecipes.map(([id, key]) => ({ id, key }));

export const TECHS: readonly TechDef[] = rawTechs.map(([id, key]) => ({ id, key }));

export const RESOURCE_ITEMS: readonly number[] = manifest.resourceItems as unknown as number[];

export function item(id: number): ItemDef {
  return ITEMS[id] ?? { id, key: "unknown", maxStack: 0, name: `Item ${id}` };
}

export function itemName(id: number): string {
  return item(id).name;
}

export function machine(prototype: number): MachineDef {
  return (
    MACHINES[prototype] ?? {
      prototype,
      key: "unknown",
      width: 1,
      height: 1,
      maxPower: 0,
    }
  );
}

export function machineName(prototype: number): string {
  return machine(prototype).key.replace(/_/g, " ");
}

export function machineSize(prototype: number): { width: number; height: number } {
  const def = machine(prototype);
  return { width: def.width, height: def.height };
}

export function itemForResource(resource: number): number {
  return RESOURCE_ITEMS[resource] ?? -1;
}

export function resourceForItem(item: number): number {
  return RESOURCE_ITEMS.indexOf(item);
}

export const beltSpeedTicks = (): number => BELTS[0]?.speedTicks ?? 30;

export function has(defs: readonly { readonly id: number }[], id: number): boolean {
  return defs.some((def) => def.id === id);
}
