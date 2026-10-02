import type { ConnectionInfo } from "../net/connection";
import type { WorldStats } from "../state/world";
import { formatNumber, formatTick, setClass, setText } from "./dom";
import type { TileData, TilePos } from "../sim/terrain";

export interface HudElements {
  readonly connection: HTMLElement;
  readonly connectionDetail: HTMLElement;
  readonly tick: HTMLElement;
  readonly tickClock: HTMLElement;
  readonly power: HTMLElement;
  readonly position: HTMLElement;
  readonly hover: HTMLElement;
  readonly mode: HTMLElement;
  readonly events: HTMLElement;
}

export interface HudState {
  readonly connection: ConnectionInfo;
  readonly stats: WorldStats;
  readonly player: TilePos;
  readonly hover: TilePos | null;
  readonly hoverTile: TileData | null;
  readonly demo: boolean;
  readonly recentEvents: readonly string[];
}

export class Hud {
  constructor(private readonly elements: HudElements) {}

  update(state: HudState): void {
    const { connection, stats } = state;
    const demo = state.demo && connection.state === "idle";
    setText(this.elements.connection, demo ? "demo" : connection.state);
    setClass(this.elements.connection, "is-open", !demo && connection.state === "open");
    setClass(this.elements.connection, "is-error", !demo && connection.state === "failed");
    setText(
      this.elements.connectionDetail,
      demo ? "offline demo world, press C to connect a server" : connectionDetail(connection),
    );

    setText(this.elements.tick, `tick ${formatNumber(stats.tick)}`);
    setText(this.elements.tickClock, formatTick(stats.tick));
    setText(this.elements.power, powerText(stats));
    setText(this.elements.position, `${state.player.x}, ${state.player.y}`);
    setText(this.elements.mode, state.demo ? "demo" : "live");

    setText(this.elements.hover, hoverText(state));
    setClass(this.elements.hover, "is-hidden", state.hover === null);

    setText(
      this.elements.events,
      state.recentEvents.slice(-6).reverse().join("\n"),
    );
  }
}

function connectionDetail(connection: ConnectionInfo): string {
  if (connection.state === "open") {
    if (connection.roundTripMs === null) {
      return "connected";
    }
    return `${connection.roundTripMs.toFixed(0)} ms round trip`;
  }
  if (connection.state === "failed") {
    return `gave up after ${connection.attempt} attempts`;
  }
  if (connection.nextRetryMs !== null) {
    return `retrying in ${(connection.nextRetryMs / 1000).toFixed(1)}s`;
  }
  return connection.lastError ?? "not connected";
}

function powerText(stats: WorldStats): string {
  const produced = formatNumber(stats.powerProduced);
  const consumed = formatNumber(stats.powerConsumed);
  const ratio = stats.powerProduced > 0 ? stats.powerConsumed / stats.powerProduced : 0;
  const status = ratio > 1 ? "brownout" : ratio > 0.9 ? "saturated" : "ok";
  return `${produced} / ${consumed} W ${status}`;
}

function hoverText(state: HudState): string {
  const { hover, hoverTile } = state;
  if (hover === null || hoverTile === null) {
    return "";
  }
  if (hoverTile.water) {
    return `${hover.x}, ${hover.y} - water`;
  }
  if (hoverTile.ore > 0) {
    return `${hover.x}, ${hover.y} - resource ${hoverTile.resource}, ${hoverTile.ore} ore`;
  }
  return `${hover.x}, ${hover.y} - ground`;
}
