import { MovePredictor, stepPosition } from "../src/state/predict";
import type { TilePos } from "../src/sim/terrain";

const never = () => false;
const water = (blocked: TilePos[]) => (pos: TilePos) =>
  blocked.some((tile) => tile.x === pos.x && tile.y === pos.y);

const origin: TilePos = { x: 0, y: 0 };

describe("stepPosition", () => {
  it("moves exactly one tile per step", () => {
    expect(stepPosition(origin, { dx: 1, dy: 0 }, never)).toEqual({ x: 1, y: 0 });
    expect(stepPosition(origin, { dx: 0, dy: -1 }, never)).toEqual({ x: 0, y: -1 });
  });

  it("stays put on a blocked tile", () => {
    expect(stepPosition(origin, { dx: 1, dy: 0 }, water([{ x: 1, y: 0 }]))).toEqual(origin);
  });

  it("treats an empty input as no movement", () => {
    expect(stepPosition(origin, { dx: 0, dy: 0 }, never)).toEqual(origin);
  });

  it("does not create a new object when blocked", () => {
    const from: TilePos = { x: 4, y: 4 };
    expect(stepPosition(from, { dx: 1, dy: 0 }, water([{ x: 5, y: 4 }]))).toBe(from);
  });
});

describe("MovePredictor", () => {
  it("predicts locally before the server answers", () => {
    const predictor = new MovePredictor(origin, never);
    predictor.tick(1, { dx: 1, dy: 0 });
    predictor.tick(2, { dx: 0, dy: 1 });
    expect(predictor.predicted).toEqual({ x: 1, y: 1 });
    expect(predictor.pendingCount).toBe(2);
    expect(predictor.confirmed).toEqual(origin);
  });

  it("does nothing when the tick counter goes backwards", () => {
    const predictor = new MovePredictor(origin, never);
    predictor.tick(5, { dx: 1, dy: 0 });
    predictor.tick(3, { dx: 1, dy: 0 });
    expect(predictor.predicted).toEqual({ x: 1, y: 0 });
    expect(predictor.pendingCount).toBe(1);
  });

  it("accepts no command for a tick where the player held still", () => {
    const predictor = new MovePredictor(origin, never);
    predictor.tick(1, null);
    expect(predictor.predicted).toEqual(origin);
    expect(predictor.pendingCount).toBe(0);
  });

  it("keeps the prediction when the server confirms it", () => {
    const predictor = new MovePredictor(origin, never);
    predictor.tick(1, { dx: 1, dy: 0 });
    predictor.tick(2, { dx: 1, dy: 0 });
    const result = predictor.reconcile({ x: 2, y: 0 }, 2);
    expect(result.corrected).toBe(false);
    expect(result.errorTiles).toBe(0);
    expect(predictor.predicted).toEqual({ x: 2, y: 0 });
    expect(predictor.pendingCount).toBe(0);
    expect(predictor.confirmed).toEqual({ x: 2, y: 0 });
  });

  it("replays unacknowledged input, leaving the visible position alone", () => {
    const predictor = new MovePredictor(origin, never);
    predictor.tick(1, { dx: 1, dy: 0 });
    predictor.tick(2, { dx: 1, dy: 0 });
    predictor.tick(3, { dx: 1, dy: 0 });
    expect(predictor.predicted).toEqual({ x: 3, y: 0 });

    const result = predictor.reconcile({ x: 1, y: 0 }, 1);
    expect(result.acknowledged).toBe(1);
    expect(result.corrected).toBe(false);
    expect(result.errorTiles).toBe(0);
    expect(predictor.predicted).toEqual({ x: 3, y: 0 });
    expect(predictor.pendingCount).toBe(2);
  });

  it("snaps to the server position when everything is acknowledged", () => {
    const predictor = new MovePredictor(origin, never);
    predictor.tick(1, { dx: 1, dy: 0 });
    const result = predictor.reconcile({ x: -5, y: 9 }, 1);
    expect(result.corrected).toBe(true);
    expect(predictor.predicted).toEqual({ x: -5, y: 9 });
  });

  it("never replays more than it has pending", () => {
    const predictor = new MovePredictor(origin, never);
    predictor.tick(1, { dx: 1, dy: 0 });
    const result = predictor.reconcile({ x: 1, y: 0 }, 99);
    expect(result.acknowledged).toBe(1);
    expect(predictor.pendingCount).toBe(0);
  });

  it("respects water while replaying", () => {
    const predictor = new MovePredictor(origin, water([{ x: 2, y: 0 }]));
    predictor.tick(1, { dx: 1, dy: 0 });
    predictor.tick(2, { dx: 1, dy: 0 });
    predictor.tick(3, { dx: 1, dy: 0 });
    expect(predictor.predicted).toEqual({ x: 1, y: 0 });
    const result = predictor.reconcile({ x: 0, y: 0 }, 0);
    expect(predictor.predicted).toEqual({ x: 1, y: 0 });
    expect(result.corrected).toBe(false);
  });

  it("clears history on reset", () => {
    const predictor = new MovePredictor(origin, never);
    predictor.tick(1, { dx: 1, dy: 0 });
    predictor.reset({ x: 100, y: 100 });
    expect(predictor.predicted).toEqual({ x: 100, y: 100 });
    expect(predictor.confirmed).toEqual({ x: 100, y: 100 });
    expect(predictor.pendingCount).toBe(0);
  });

  it("reports the size of a visible correction", () => {
    const predictor = new MovePredictor(origin, never);
    predictor.tick(1, { dx: 1, dy: 0 });
    const result = predictor.reconcile({ x: 0, y: 1 }, 0);
    expect(result.corrected).toBe(true);
    expect(result.errorTiles).toBe(1);
  });

  it("is deterministic for the same input stream", () => {
    const run = () => {
      const predictor = new MovePredictor(origin, never);
      const script = [
        { dx: 1, dy: 0 },
        { dx: 1, dy: 0 },
        null,
        { dx: 0, dy: 1 },
      ];
      script.forEach((move, index) => predictor.tick(index + 1, move));
      predictor.reconcile({ x: 2, y: 0 }, 2);
      predictor.tick(5, { dx: 0, dy: 1 });
      return predictor.predicted;
    };
    expect(run()).toEqual(run());
  });
});
