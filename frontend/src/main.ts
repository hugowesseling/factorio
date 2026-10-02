import "./style.css";

import {
  ITEMS,
  ITEM_COAL,
  ITEM_COPPER_ORE,
  ITEM_METAL_ORE,
  ITEM_STONE_ORE,
  MACHINES,
  MACHINE_ASSEMBLER,
  MACHINE_FURNACE,
  MACHINE_GENERATOR,
  type MachineDef,
} from "./content";
import { createCamera, resizeCamera, visibleTileBounds, zoomCamera } from "./render/camera";
import { Renderer } from "./render/renderer";
import { InputController } from "./input/controls";
import { createConnection, type Connection } from "./net/connection";
import { applyServerMessage, removeEntityFromEvent, SnapshotClock, TICK_MS } from "./net/snapshot";
import { DEFAULT_VIEW_RADIUS, desiredChunks, missingChunks, staleChunks } from "./net/interest";
import { IntentKind } from "./net/protocol";
import { MovePredictor } from "./state/predict";
import { Store } from "./state/stores";
import { EntityKind, WorldStore } from "./state/world";
import { buildDemoScene, DemoSim, LAYOUT } from "./sim/demo";
import { requireElement } from "./ui/dom";
import { Hud } from "./ui/hud";
import { Hotbar, machineEntry, type HotbarEntry } from "./ui/hotbar";
import { drawMinimap, MINIMAP_TILES, renderInventory, renderTooltip } from "./ui/panels";
import { describeError, diag, diagReady, diagState } from "./ui/diagnostics";

const DEFAULT_SEED = 1;
const LOCAL_PLAYER = 0;
const DEFAULT_SERVER_URL = "ws://127.0.0.1:9000/ws";

interface HotbarState {
  readonly entries: readonly HotbarEntry[];
  readonly selected: number;
  readonly counts: ReadonlyMap<number, number>;
}

diag("info", "booting client");
diagState("booting…");

const seed = readSeedFromUrl() ?? DEFAULT_SEED;
const serverUrl = readServerFromUrl() ?? DEFAULT_SERVER_URL;
const autoConnect = new URLSearchParams(window.location.search).get("connect") !== "0";
diag("info", `world seed ${seed}, server ${serverUrl}, autoConnect ${autoConnect}`);
const world = new WorldStore(seed);
let demo = new DemoSim(world);
let inDemoWorld = true;
buildDemoScene(world);
diag("ok", "demo world generated");

const camera = createCamera(window.innerWidth, window.innerHeight);
camera.scale = 0.25;

const canvas = requireElement<HTMLCanvasElement>("stage");
const minimapCanvas = requireElement<HTMLCanvasElement>("minimap-canvas");
const alerts = requireElement("alerts");

let renderer: Renderer | null = null;
try {
  renderer = new Renderer(canvas);
  diag("ok", "WebGL2 renderer initialized");
} catch (error) {
  renderer = null;
  diag("error", `renderer failed: ${describeError(error)}`);
  diag("warn", "continuing in HUD-only mode so diagnostics and networking keep working");
  showFatal(error);
}

const hotbarState = new Store<HotbarState>({ entries: [], selected: 0, counts: new Map() });
const selected = new Store<{ x: number; y: number } | null>(null);
const showGrid = new Store(false);
const demoMode = new Store(true);

const predictor = new MovePredictor({ x: LAYOUT.spawn.x, y: LAYOUT.spawn.y }, pos =>
  world.entityAt(pos) !== undefined || !world.isSelectable(pos),
);
centerOnPlayer();

const clock = new SnapshotClock();
const connection: Connection = createConnection(serverUrl, `client-${Math.floor(Math.random() * 1000)}`);
const hud = new Hud({
  connection: requireElement("hud-status"),
  connectionDetail: requireElement("hud-detail"),
  tick: requireElement("hud-tick"),
  tickClock: requireElement("hud-clock"),
  power: requireElement("hud-power"),
  position: requireElement("hud-position"),
  hover: requireElement("hud-hover"),
  mode: requireElement("hud-mode"),
  events: requireElement("hud-events"),
});

const hotbar = new Hotbar({
  root: requireElement("hotbar"),
  slots: createHotbarSlots(requireElement("hotbar"), 10),
});

const inventory = {
  root: requireElement("inventory-slots"),
  slots: createSlots(requireElement("inventory-slots"), 8),
};
const tooltip = {
  root: requireElement("tooltip"),
  title: requireElement("tooltip-title"),
  body: requireElement("tooltip-body"),
};

const input = new InputController({
  target: canvas,
  keyTarget: window,
  camera,
  onHotbar: slot => {
    hotbarState.update(current => ({ ...current, selected: slot }));
  },
  onToggleGrid: () => showGrid.update(value => !value),
  onToggleConnection: () => toggleConnection(),
  onCycleBuild: direction => {
    hotbarState.update(current => {
      const size = current.entries.length || 1;
      return { ...current, selected: (current.selected + direction + size) % size };
    });
  },
  onCancel: () => selected.set(null),
  onAction: action => handleAction(action),
});

connection.onMessage = message => {
  if (message.tag === "hello") {
    enterLiveWorld(message.seed);
    world.pushEvent(`connected to ${serverUrl}`);
  } else {
    inDemoWorld = false;
    demoMode.set(false);
  }
  if (message.tag === "event") {
    removeEntityFromEvent(world, message);
  }
  applyServerMessage(world, clock, message);
};
connection.onClose = () => {
  enterDemoWorld();
  world.pushEvent("server connection lost, running the demo world");
  diag("warn", "disconnected from the server, running the demo world");
};

let lastConnectionKey = "";
connection.info.subscribe(info => {
  const key = `${info.state}|${info.attempt}|${info.lastError ?? ""}|${info.nextRetryMs ?? ""}`;
  if (key === lastConnectionKey) {
    return;
  }
  lastConnectionKey = key;
  const detail = info.lastError === null ? "" : `: ${info.lastError}`;
  const retry = info.nextRetryMs === null ? "" : ` (retry in ${info.nextRetryMs} ms)`;
  const level = info.state === "open" ? "ok" : info.state === "failed" ? "error" : "info";
  diag(level, `connection ${info.state}${detail}${retry}`);
});
diag("info", `input attached (keyboard on window, pointer on canvas)`);

function enterDemoWorld(): void {
  world.reset(seed);
  buildDemoScene(world);
  demo = new DemoSim(world);
  inDemoWorld = true;
  demoMode.set(true);
}

function enterLiveWorld(serverSeed: number): void {
  if (inDemoWorld || world.seed !== serverSeed) {
    world.reset(serverSeed);
  }
  inDemoWorld = false;
  demoMode.set(false);
}

initHotbar();
syncChunkResidency();
hotbarState.subscribe(() => renderHotbar());
renderHotbar();
renderInventory(inventory, inventoryCounts());

if (autoConnect) {
  connection.connect();
}

let previousFrame = performance.now();
let accumulator = 0;
let fps = 0;
let frames = 0;
let fpsClock = previousFrame;
let announcedReady = false;

requestAnimationFrame(frame);

function frame(now: number): void {
  const delta = Math.min(250, now - previousFrame);
  previousFrame = now;
  accumulator += delta;

  while (accumulator >= TICK_MS) {
    accumulator -= TICK_MS;
    step();
  }

  const hover = input.hoveredTile;
  clock.sample();
  if (renderer !== null) {
    renderer.resize(camera);
    renderer.render(
      {
        world,
        camera,
        hover,
        selected: selected.value,
        showGrid: showGrid.value,
        localPlayer: LOCAL_PLAYER,
      },
      visibleTileBounds(camera, 1),
    );
  }

  frames += 1;
  if (now - fpsClock >= 500) {
    fps = (frames * 1000) / (now - fpsClock);
    frames = 0;
    fpsClock = now;
  }

  drawMinimap({
    element: minimapCanvas,
    size: MINIMAP_TILES,
    world,
    camera,
    localPlayer: LOCAL_PLAYER,
  });
  updateUi(hover);

  if (!announcedReady) {
    announcedReady = true;
    diag("ok", renderer === null ? "render loop running (no WebGL2)" : "render loop running");
    diagReady();
  }

  requestAnimationFrame(frame);
}

function step(): void {
  const tick = world.currentStats.tick + 1;
  const move = input.moveIntent();

  if (demoMode.value) {
    demo.step();
    predictor.tick(tick, move);
    demo.syncPlayer(predictor, LOCAL_PLAYER);
  } else {
    connection.sendIntent(LOCAL_PLAYER, { kind: IntentKind.Move, x: move?.dx ?? 0, y: move?.dy ?? 0, arg: 0 });
    predictor.tick(tick, move);
    demo.syncPlayer(predictor, LOCAL_PLAYER);
    const result = predictor.reconcile(
      world.player(LOCAL_PLAYER) === undefined
        ? predictor.predicted
        : { x: world.player(LOCAL_PLAYER)?.x ?? 0, y: world.player(LOCAL_PLAYER)?.y ?? 0 },
      1,
    );
    if (result.corrected) {
      world.pushEvent(`corrected by ${result.errorTiles} tile(s)`);
    }
  }
}

function handleAction(action: string): void {
  switch (action) {
    case "zoom-in":
      zoomCamera(camera, 0.8);
      return;
    case "zoom-out":
      zoomCamera(camera, 1.25);
      return;
    case "select": {
      const tile = input.hoveredTile;
      if (tile !== null) {
        selected.set(tile);
      }
      return;
    }
    case "remove": {
      const tile = input.hoveredTile;
      if (tile !== null && world.isSelectable(tile)) {
        world.removeEntity(EntityKind.Belt, 0);
        sendIntent(IntentKind.Remove, tile.x, tile.y);
      }
      return;
    }
    case "mine": {
      const tile = input.hoveredTile;
      if (tile !== null) {
        sendIntent(IntentKind.Mine, tile.x, tile.y);
      }
      return;
    }
    case "interact":
    case "rotate":
    case "map":
    case "debug":
      return;
    default:
      return;
  }
}

function sendIntent(kind: IntentKind, x: number, y: number, arg = 0): void {
  if (demoMode.value) {
    world.pushEvent(`${intentName(kind)} ${x},${y} (demo only)`);
    return;
  }
  connection.sendIntent(LOCAL_PLAYER, { kind, x, y, arg });
}

function toggleConnection(): void {
  if (connection.info.value.state === "idle") {
    connection.connect();
    return;
  }
  connection.disconnect();
  demoMode.set(true);
}

function updateUi(hover: { x: number; y: number } | null): void {
  hud.update({
    connection: connection.info.value,
    stats: world.currentStats,
    player: predictor.predicted,
    hover,
    hoverTile: hover === null ? null : world.tile(hover),
    demo: demoMode.value,
    recentEvents: world.recentEvents,
  });

  requireElement("hud-chunks").textContent = `${world.residentChunks} chunks`;
  requireElement("hud-fps").textContent = `${fps.toFixed(0)} fps`;

  renderTooltip(tooltip, hover, world, camera);
  renderInventory(inventory, inventoryCounts());
}

function syncChunkResidency(): void {
  const focus = predictor.predicted;
  const chunk = {
    x: Math.floor(focus.x / 32),
    y: Math.floor(focus.y / 32),
  };
  const wanted = desiredChunks(chunk, { viewRadius: DEFAULT_VIEW_RADIUS });
  const resident: string[] = [];
  for (let dy = -3; dy <= 3; dy += 1) {
    for (let dx = -3; dx <= 3; dx += 1) {
      const candidate = { x: chunk.x + dx, y: chunk.y + dy };
      if (world.hasChunk(candidate)) {
        resident.push(`${candidate.x},${candidate.y}`);
      }
    }
  }

  for (const key of staleChunks(resident, wanted)) {
    const [x = "0", y = "0"] = key.split(",");
    world.releaseChunk({ x: Number.parseInt(x, 10), y: Number.parseInt(y, 10) });
  }
  for (const key of missingChunks(resident, wanted)) {
    const [x = "0", y = "0"] = key.split(",");
    world.chunk({ x: Number.parseInt(x, 10), y: Number.parseInt(y, 10) });
  }
}

function initHotbar(): void {
  const machines: readonly MachineDef[] = [
    machineDef(MACHINE_GENERATOR),
    machineDef(MACHINE_FURNACE),
    machineDef(MACHINE_ASSEMBLER),
  ];
  hotbarState.set({
    entries: [
      { kind: "item", id: ITEM_COAL, label: "coal", count: 250 },
      { kind: "item", id: ITEM_METAL_ORE, label: "metal ore", count: 400 },
      { kind: "item", id: ITEM_COPPER_ORE, label: "copper ore", count: 350 },
      { kind: "item", id: ITEM_STONE_ORE, label: "stone ore", count: 300 },
      ...machines.map(def => machineEntry(def, 10)),
    ],
    selected: 0,
    counts: new Map([
      [ITEM_COAL, 250],
      [ITEM_METAL_ORE, 400],
      [ITEM_COPPER_ORE, 350],
      [ITEM_STONE_ORE, 300],
    ]),
  });
}

function renderHotbar(): void {
  hotbar.update({ entries: hotbarState.value.entries, selected: hotbarState.value.selected });
}

function inventoryCounts(): { item: number; count: number }[] {
  const counts = hotbarState.value.counts;
  return ITEMS.filter(item => counts.has(item.id))
    .slice(0, 8)
    .map(item => ({ item: item.id, count: counts.get(item.id) ?? 0 }));
}

function intentName(kind: IntentKind): string {
  switch (kind) {
    case IntentKind.Move:
      return "move";
    case IntentKind.PlaceBelt:
      return "place belt";
    case IntentKind.PlaceInserter:
      return "place inserter";
    case IntentKind.PlaceMachine:
      return "place machine";
    case IntentKind.Remove:
      return "remove";
    case IntentKind.Mine:
      return "mine";
    case IntentKind.Collect:
      return "collect";
    case IntentKind.Research:
      return "research";
    default:
      return "intent";
  }
}

function machineDef(prototype: number): MachineDef {
  const def = MACHINES.find(entry => entry.prototype === prototype);
  if (def === undefined) {
    throw new Error(`machine prototype ${prototype} is missing from the content manifest`);
  }
  return def;
}

function createHotbarSlots(root: HTMLElement, count: number): HTMLElement[] {
  const section = document.createElement("div");
  section.id = "hotbar-slots";
  section.style.display = "flex";
  section.style.gap = "4px";
  const slots: HTMLElement[] = [];
  for (let index = 0; index < count; index += 1) {
    const slot = document.createElement("button");
    slot.type = "button";
    slots.push(slot);
    section.append(slot);
  }
  root.append(section);
  return slots;
}

function createSlots(root: HTMLElement, count: number): HTMLElement[] {
  const slots: HTMLElement[] = [];
  for (let index = 0; index < count; index += 1) {
    const slot = document.createElement("div");
    slots.push(slot);
    root.append(slot);
  }
  return slots;
}

function centerOnPlayer(): void {
  camera.x = predictor.predicted.x + 0.5;
  camera.y = predictor.predicted.y + 0.5;
  resizeCamera(camera, window.innerWidth, window.innerHeight);
}

function readSeedFromUrl(): number | null {
  const raw = new URLSearchParams(window.location.search).get("seed");
  if (raw === null) {
    return null;
  }
  const parsed = Number.parseInt(raw, 10);
  return Number.isInteger(parsed) ? parsed : null;
}

function readServerFromUrl(): string | null {
  const raw = new URLSearchParams(window.location.search).get("server");
  return raw !== null && raw.length > 0 ? raw : null;
}

function showFatal(error: unknown): void {
  const message = error instanceof Error ? error.message : String(error);
  const alert = document.createElement("div");
  alert.className = "alert";
  alert.textContent = message;
  alerts.append(alert);
}
