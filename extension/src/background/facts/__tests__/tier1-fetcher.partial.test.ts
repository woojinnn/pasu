import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { OracleNeed } from "../../oracle/oracle-snapshot";
import type { OracleEntry } from "../../types/host-snapshot";

const oracleMocks = vi.hoisted(() => ({
  buildOracleSnapshot: vi.fn(),
}));

vi.mock("../../oracle/oracle-snapshot", () => ({
  buildOracleSnapshot: oracleMocks.buildOracleSnapshot,
}));

import { fetchTier1, type Tier1Plan } from "../tier1-fetcher";

const ETHEREUM_KEY = "ethereum";
const TETHER_KEY = "tether";

function token(tokenKey: string) {
  return {
    chain_id: 1,
    address: tokenKey,
    symbol: tokenKey.toUpperCase(),
    decimals: 18,
    is_native: false,
  };
}

function plan(overrides: Partial<Tier1Plan> = {}): Tier1Plan {
  return {
    tokens_for_oracle: [],
    balances: [],
    allowances: [],
    clock_required: false,
    sig_oracle_requirements: [],
    ...overrides,
  };
}

describe("fetchTier1 partial oracle preservation", () => {
  let warnSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.useFakeTimers();
    oracleMocks.buildOracleSnapshot.mockReset();
    warnSpy = vi.spyOn(console, "warn").mockImplementation(() => undefined);
  });

  afterEach(() => {
    warnSpy.mockRestore();
    vi.useRealTimers();
  });

  it("returns the prices that resolved before the dimension timeout", async () => {
    const earlyPrice: OracleEntry = {
      token_key: `1:${ETHEREUM_KEY}`,
      usd_price: 3500,
      usd_per_unit: "3500",
      as_of_ts: 10,
      stale_sec: 0,
      sources: ["test"],
    };

    oracleMocks.buildOracleSnapshot.mockImplementation(
      (needs: readonly OracleNeed[]) => {
        const tokenKeys = needs.map((need) => need.address.toLowerCase());
        if (tokenKeys.includes(TETHER_KEY)) {
          return new Promise<OracleEntry[]>(() => undefined);
        }
        if (tokenKeys.includes(ETHEREUM_KEY)) {
          return new Promise<OracleEntry[]>((resolve) => {
            setTimeout(() => resolve([earlyPrice]), 50);
          });
        }
        return Promise.resolve([]);
      },
    );

    const resultPromise = fetchTier1(
      plan({
        tokens_for_oracle: [token(ETHEREUM_KEY), token(TETHER_KEY)],
        balances: [],
        allowances: [],
      }),
      vi.fn() as unknown as typeof fetch,
      10_000,
    );

    await vi.advanceTimersByTimeAsync(1_500);

    const result = await resultPromise;
    expect(result.oracle.map((entry) => entry.token_key)).toEqual([
      `1:${ETHEREUM_KEY}`,
    ]);
  });

  it("preserves resolved sig_oracle_requirements entries before the dimension timeout", async () => {
    const earlyPrice: OracleEntry = {
      token_key: `1:${ETHEREUM_KEY}`,
      usd_price: 3500,
      usd_per_unit: "3500",
      as_of_ts: 10,
      stale_sec: 0,
      sources: ["test"],
    };

    oracleMocks.buildOracleSnapshot.mockImplementation(
      (needs: readonly OracleNeed[]) => {
        const tokenKeys = needs.map((need) => need.address.toLowerCase());
        if (tokenKeys.includes(TETHER_KEY)) {
          return new Promise<OracleEntry[]>(() => undefined);
        }
        if (tokenKeys.includes(ETHEREUM_KEY)) {
          return new Promise<OracleEntry[]>((resolve) => {
            setTimeout(() => resolve([earlyPrice]), 50);
          });
        }
        return Promise.resolve([]);
      },
    );

    const resultPromise = fetchTier1(
      plan({
        sig_oracle_requirements: [
          {
            kind: "input",
            token: token(ETHEREUM_KEY),
            raw_amount: "1000000000000000000",
          },
          {
            kind: "input",
            token: token(TETHER_KEY),
            raw_amount: "2000000000000000000",
          },
        ],
      }),
      vi.fn() as unknown as typeof fetch,
      10_000,
    );

    await vi.advanceTimersByTimeAsync(1_500);

    const result = await resultPromise;
    expect(result.oracle.map((entry) => entry.token_key)).toEqual([
      `1:${ETHEREUM_KEY}`,
    ]);
  });
});
