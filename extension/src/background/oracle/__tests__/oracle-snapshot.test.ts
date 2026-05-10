import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { store } from "../price-cache";
import { NATIVE_TOKEN_ADDRESS, buildOracleSnapshot } from "../oracle-snapshot";

const WETH = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const DAI = "0x6b175474e89094c44da98b954eedeac495271d0f";
const storage = new Map<string, unknown>();

function installChromeStorageMock(): void {
  Object.defineProperty(globalThis, "chrome", {
    configurable: true,
    value: {
      storage: {
        local: {
          get: vi.fn(async (keys: string | string[]) => {
            const out: Record<string, unknown> = {};
            for (const key of Array.isArray(keys) ? keys : [keys])
              out[key] = storage.get(key);
            return out;
          }),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [key, value] of Object.entries(entries))
              storage.set(key, value);
          }),
          remove: vi.fn(async (keys: string | string[]) => {
            for (const key of Array.isArray(keys) ? keys : [keys])
              storage.delete(key);
          }),
        },
      },
    },
  });
}

describe("buildOracleSnapshot", () => {
  beforeEach(() => {
    storage.clear();
    installChromeStorageMock();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("returns cache hits without fetching", async () => {
    await store(1, new Map([[WETH, 3500]]), 10_000, new Map([[WETH, 7_000]]));
    const fetchMock = vi.fn();

    const result = await buildOracleSnapshot(
      [{ chainId: 1, address: WETH }],
      fetchMock as any,
      10_000,
    );
    expect(fetchMock).not.toHaveBeenCalled();
    expect(result).toEqual([
      expect.objectContaining({
        token_key: `1:${WETH}`,
        usd_price: 3500,
        usd_per_unit: "3500",
        as_of_ts: 7,
        stale_sec: 3,
      }),
    ]);
  });

  it("fetches misses from CoinGecko and stores them for the next call", async () => {
    const fetchMock = vi.fn(
      async () =>
        new Response(
          JSON.stringify({ [WETH]: { usd: 3500, last_updated_at: 7 } }),
        ),
    );

    const first = await buildOracleSnapshot(
      [{ chainId: 1, address: WETH }],
      fetchMock as any,
      10_000,
    );
    const second = await buildOracleSnapshot(
      [{ chainId: 1, address: WETH }],
      fetchMock as any,
      11_000,
    );
    expect(first[0]).toMatchObject({
      token_key: `1:${WETH}`,
      usd_price: 3500,
      stale_sec: 3,
    });
    expect(second[0]).toMatchObject({
      token_key: `1:${WETH}`,
      usd_price: 3500,
      stale_sec: 4,
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("uses the native price path and threads the sentinel address into token_key", async () => {
    const fetchMock = vi.fn(async (url: string) => {
      expect(url).toContain("/simple/price");
      return new Response(
        JSON.stringify({ ethereum: { usd: 3500, last_updated_at: 8 } }),
      );
    });

    const result = await buildOracleSnapshot(
      [{ chainId: 1, address: NATIVE_TOKEN_ADDRESS, isNative: true }],
      fetchMock as any,
      10_000,
    );
    expect(result).toEqual([
      expect.objectContaining({
        token_key: `1:${NATIVE_TOKEN_ADDRESS.toLowerCase()}`,
        usd_price: 3500,
        stale_sec: 2,
      }),
    ]);
  });

  it("fails open when CoinGecko returns no price data", async () => {
    const fetchMock = vi.fn(async () => new Response("{}"));
    const result = await buildOracleSnapshot(
      [{ chainId: 1, address: WETH }],
      fetchMock as any,
      10_000,
    );
    expect(result).toEqual([]);
  });

  it("logs CoinGecko fetch failures while preserving resolved entries", async () => {
    await store(1, new Map([[WETH, 3500]]), 10_000, new Map([[WETH, 7_000]]));
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const fetchMock = vi.fn(
      async () => new Response("rate limited", { status: 429 }),
    );

    const result = await buildOracleSnapshot(
      [
        { chainId: 1, address: WETH },
        { chainId: 1, address: DAI },
      ],
      fetchMock as any,
      10_000,
    );

    expect(result).toEqual([
      expect.objectContaining({ token_key: `1:${WETH}` }),
    ]);
    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn).toHaveBeenCalledWith(
      "[Scopeball SW] CoinGecko fetch failed",
      expect.objectContaining({
        tokenKeys: [`1:${DAI}`],
        status: 429,
        cause: expect.stringContaining("rate limited"),
      }),
    );
  });
});
