import { describe, expect, it } from "vitest";

import {
  MAX_SQRT_RATIO,
  MAX_TICK,
  MIN_SQRT_RATIO,
  MIN_TICK,
  getSqrtRatioAtTick,
  tickFromTickCumulatives,
} from "../uniswap-tick-math";

/**
 * Reference vectors taken from the Uniswap v3-core test snapshot
 * (`test/__snapshots__/TickMath.spec.ts.snap`). Each value is the deterministic
 * `getSqrtRatioAtTick(tick)` output, NOT a pool slot0 reading. The tolerance
 * is ±1 to account for the Q64.96 last-bit rounding documented in the
 * algorithm.
 */
const TICK_REFERENCE_VECTORS: ReadonlyArray<readonly [number, bigint]> = [
  // tick 85176 - "WETH/USDC range" magnitude (canonical port value; matches the
  // v3-core algorithm output, distinct from a live pool's slot0 snapshot).
  [85176, 5602223755577321903022134995689n],
  // tick 250000 - exact match with v3-core snapshot fixture `tick 250000 result`.
  [250000, 21246587762933397357449903968194344n],
  // tick -250000 - exact match with v3-core snapshot `tick -250000 result`.
  [-250000, 295440463448801648376846n],
];

function absDiff(a: bigint, b: bigint): bigint {
  return a > b ? a - b : b - a;
}

describe("TickMath.getSqrtRatioAtTick", () => {
  it("returns 2^96 for tick 0", () => {
    expect(getSqrtRatioAtTick(0)).toBe(79228162514264337593543950336n);
  });

  it("returns MIN_SQRT_RATIO for MIN_TICK", () => {
    expect(getSqrtRatioAtTick(MIN_TICK)).toBe(MIN_SQRT_RATIO);
  });

  it("returns MAX_SQRT_RATIO for MAX_TICK", () => {
    expect(getSqrtRatioAtTick(MAX_TICK)).toBe(MAX_SQRT_RATIO);
  });

  it("monotonically increases with tick", () => {
    const r0 = getSqrtRatioAtTick(0);
    const r1 = getSqrtRatioAtTick(1);
    const rNeg1 = getSqrtRatioAtTick(-1);
    expect(r1).toBeGreaterThan(r0);
    expect(rNeg1).toBeLessThan(r0);
  });

  it("rejects out-of-range ticks", () => {
    expect(() => getSqrtRatioAtTick(MAX_TICK + 1)).toThrow();
    expect(() => getSqrtRatioAtTick(MIN_TICK - 1)).toThrow();
    expect(() => getSqrtRatioAtTick(1.5)).toThrow();
  });

  it.each(TICK_REFERENCE_VECTORS)(
    "matches the v3-core reference vector for tick %i (±1)",
    (tick, expected) => {
      const actual = getSqrtRatioAtTick(tick);
      expect(absDiff(actual, expected) <= 1n).toBe(true);
    },
  );

  it("satisfies the reciprocal identity getSqrtRatioAtTick(t) * getSqrtRatioAtTick(-t) ≈ 2^192", () => {
    // The Q64.96 product of opposite ticks should equal 2^192 modulo last-bit
    // rounding accumulated across the two rounded sqrts. Allow a relative
    // tolerance of 1e-12 of 2^192 (well within Uniswap's published rounding
    // bounds) - this is an independent ground-truth check that doesn't
    // require a fixture lookup.
    const tick = 85176;
    const product = getSqrtRatioAtTick(tick) * getSqrtRatioAtTick(-tick);
    const target = 1n << 192n;
    const tolerance = target / 10n ** 12n;
    expect(absDiff(product, target) <= tolerance).toBe(true);
  });
});

describe("tickFromTickCumulatives", () => {
  it("returns the tick exactly when divisible", () => {
    // 200 ticks * 1800 seconds = 360000 cumulative units
    const tick = tickFromTickCumulatives([0n, 360_000n], 1800);
    expect(tick).toBe(200);
  });

  it("rounds negative deltas toward negative infinity", () => {
    // -100 / 1800 with non-zero remainder -> -1 not 0
    const tick = tickFromTickCumulatives([0n, -100n], 1800);
    expect(tick).toBe(-1);
  });

  it("does not adjust when negative delta divides evenly", () => {
    const tick = tickFromTickCumulatives([0n, -1800n], 1800);
    expect(tick).toBe(-1);
  });
});
