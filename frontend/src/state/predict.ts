import type { TilePos } from "../sim/terrain";

export interface MoveCommand {
  readonly dx: number;
  readonly dy: number;
}

export interface ReconciledMove {
  readonly position: TilePos;

  readonly acknowledged: number;

  readonly corrected: boolean;

  readonly errorTiles: number;
}

export function stepPosition(
  from: TilePos,
  move: MoveCommand,
  blocked: (pos: TilePos) => boolean,
): TilePos {
  if (move.dx === 0 && move.dy === 0) {
    return from;
  }
  const target: TilePos = { x: from.x + move.dx, y: from.y + move.dy };
  if (blocked(target)) {
    return from;
  }
  return target;
}

export class MovePredictor {
  private position: TilePos;
  private lastConfirmed: TilePos;
  private pending: MoveCommand[] = [];
  private readonly blocked: (pos: TilePos) => boolean;
  private nextTick = 0;

  constructor(start: TilePos, blocked: (pos: TilePos) => boolean) {
    this.position = start;
    this.lastConfirmed = start;
    this.blocked = blocked;
  }

  get predicted(): TilePos {
    return this.position;
  }

  get confirmed(): TilePos {
    return this.lastConfirmed;
  }

  get pendingCount(): number {
    return this.pending.length;
  }

  get pendingCommands(): readonly MoveCommand[] {
    return this.pending;
  }

  tick(currentTick: number, move: MoveCommand | null): void {
    if (currentTick < this.nextTick) {
      return;
    }
    this.nextTick = currentTick + 1;
    if (move !== null) {
      this.pending.push(move);
      this.position = stepPosition(this.position, move, this.blocked);
    }
  }

  reconcile(serverPosition: TilePos, acknowledged: number): ReconciledMove {
    const previous = this.position;
    this.lastConfirmed = serverPosition;
    const clamped = Math.max(0, Math.min(acknowledged, this.pending.length));
    this.pending.splice(0, clamped);

    let cursor = serverPosition;
    for (const command of this.pending) {
      cursor = stepPosition(cursor, command, this.blocked);
    }
    this.position = cursor;

    const errorTiles = Math.abs(previous.x - cursor.x) + Math.abs(previous.y - cursor.y);
    return {
      position: cursor,
      acknowledged: clamped,
      corrected: errorTiles > 0,
      errorTiles,
    };
  }

  reset(position: TilePos): void {
    this.position = position;
    this.lastConfirmed = position;
    this.pending = [];
  }
}
