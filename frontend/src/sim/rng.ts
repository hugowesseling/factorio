const MASK64 = 0xffff_ffff_ffff_ffffn;
const MASK32 = 0xffff_ffffn;
const MULTIPLIER = 6364136223846793005n;
const MIX_MULTIPLIER = 0xff51afd7ed558ccdn;
const INCREMENT = 0x243f6a8885a308d3n;
const GOLDEN = 0x9e3779b97f4a7c15n;

export const RNG_GOLDEN = GOLDEN;

export function toSeed(value: number | bigint): bigint {
  return (typeof value === "bigint" ? value : BigInt(value)) & MASK64;
}

export class Pcg32 {
  private state: bigint;
  private readonly inc: bigint;

  constructor(seed: number | bigint) {
    this.state = (toSeed(seed) + GOLDEN) & MASK64;
    this.inc = INCREMENT | 1n;
  }

  nextU32(): number {
    const old = this.state;
    this.state = (old * MULTIPLIER + this.inc) & MASK64;
    const xored = ((old >> 33n) ^ old) & MASK64;
    const mixed = (xored * MIX_MULTIPLIER) & MASK64;
    return Number(((mixed >> 16n) ^ mixed) & MASK32);
  }

  range(min: number, max: number): number {
    if (max <= min) {
      return min;
    }
    return min + (this.nextU32() % (max - min));
  }
}

export function chunkKey(x: number, y: number): bigint {
  return ((BigInt(x >>> 0) << 32n) | BigInt(y >>> 0)) & MASK64;
}

export function rotateLeft(value: bigint, amount: bigint): bigint {
  const bits = value & MASK64;
  const shift = amount & 63n;
  return ((bits << shift) | (bits >> (64n - shift))) & MASK64;
}
