import {
  BELTS,
  has,
  INSERTERS,
  item,
  itemForResource,
  itemName,
  ITEMS,
  ITEM_COPPER_ORE,
  ITEM_COAL,
  ITEM_METAL_ORE,
  ITEM_STONE_ORE,
  ITEM_WATER,
  MACHINES,
  machine,
  machineName,
  machineSize,
  RECIPES,
  RESOURCE_ITEMS,
  TECHS,
} from "../src/content";
import { RESOURCE_COUNT } from "../src/sim/terrain";
import vectors from "./vectors.json";

const backendContent = vectors.content as unknown as {
  items: (readonly [number, string, number, string])[];
  machines: (readonly [number, string, number, number, number])[];
  belts: (readonly [number, string, number])[];
  inserters: (readonly [number, string, number])[];
  recipes: (readonly [number, string])[];
  technologies: (readonly [number, string])[];
  resourceItems: number[];
};

describe("generated content", () => {
  it("matches the backend manifest item for item", () => {
    const expected = backendContent.items.map(([id, key, maxStack, name]) => ({
      id,
      key,
      maxStack,
      name,
    }));
    expect(ITEMS).toEqual(expected);
  });

  it("matches the backend machine footprints and power limits", () => {
    const expected = backendContent.machines.map(([prototype, key, width, height, maxPower]) => ({
      prototype,
      key,
      width,
      height,
      maxPower,
    }));
    expect(MACHINES).toEqual(expected);
  });

  it("matches the backend belts, inserters, recipes and technologies", () => {
    expect(BELTS.map((belt) => belt.key)).toEqual(
      backendContent.belts.map(([, key]) => key),
    );
    expect(INSERTERS.map((inserter) => inserter.key)).toEqual(
      backendContent.inserters.map(([, key]) => key),
    );
    expect(RECIPES.map((recipe) => recipe.key)).toEqual(
      backendContent.recipes.map(([, key]) => key),
    );
    expect(TECHS.map((tech) => tech.key)).toEqual(
      backendContent.technologies.map(([, key]) => key),
    );
  });

  it("gives every terrain resource a real item", () => {
    for (let resource = 0; resource < RESOURCE_COUNT; resource += 1) {
      const id = itemForResource(resource);
      expect(id).toBeGreaterThanOrEqual(0);
      expect(item(id).key).not.toBe("unknown");
    }
  });

  it("keeps the resource to item mapping", () => {
    expect(RESOURCE_ITEMS).toEqual(backendContent.resourceItems);
    expect(itemForResource(0)).toBe(ITEM_METAL_ORE);
    expect(itemForResource(1)).toBe(ITEM_COPPER_ORE);
    expect(itemForResource(2)).toBe(ITEM_STONE_ORE);
    expect(itemForResource(3)).toBe(ITEM_COAL);
    expect(itemForResource(9)).toBe(-1);
  });

  it("exposes the item names the tooltip shows", () => {
    expect(itemName(ITEM_METAL_ORE)).toBe("Metal ore");
    expect(itemName(ITEM_WATER)).toBe("Water");
  });

  it("falls back gracefully for an unknown id", () => {
    expect(item(999).name).toBe("Item 999");
    expect(machine(999).width).toBe(1);
    expect(machineName(0)).toBe("burner generator");
  });

  it("reports machine sizes used for placement", () => {
    expect(machineSize(0)).toEqual({ width: 1, height: 1 });
    expect(machineSize(1)).toEqual({ width: 2, height: 2 });
    expect(machineSize(2)).toEqual({ width: 3, height: 3 });
    expect(machine(2).maxPower).toBe(900000);
  });

  it("answers membership questions without a map", () => {
    expect(has(ITEMS, ITEM_GEAR_ID)).toBe(true);
    expect(has(ITEMS, 4242)).toBe(false);
  });
});

const ITEM_GEAR_ID = ITEMS.find((entry) => entry.key === "gear")?.id ?? -1;
