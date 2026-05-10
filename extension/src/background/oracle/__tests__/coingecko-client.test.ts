import { afterEach, describe, expect, it, vi } from "vitest";
import {
  fetchNativeUsdPrices,
  fetchUsdPrices,
  nativePriceLastUpdatedAt,
  priceLastUpdatedAt,
} from "../coingecko-client";

const WETH = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const USDC = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

describe("coingecko-client", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns an empty Map for empty token input without calling fetch", async () => {
    const fetchMock = vi.fn();
    const result = await fetchUsdPrices(
      1,
      [],
      fetchMock as unknown as typeof fetch,
    );
    expect(result).toEqual(new Map());
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("fetches lowercased deduped token addresses and preserves last_updated_at metadata", async () => {
    const fetchMock = vi.fn(async (url: string) => {
      expect(url).toContain("/simple/token_price/ethereum");
      const parsed = new URL(url);
      expect(parsed.searchParams.get("contract_addresses")).toBe(
        `${WETH},${USDC}`,
      );
      return new Response(
        JSON.stringify({
          [WETH]: { usd: 3500.42, last_updated_at: 1_700_000_000 },
          [USDC]: { usd: 1, last_updated_at: 1_700_000_010 },
        }),
      );
    });

    const result = await fetchUsdPrices(
      1,
      [WETH.toUpperCase(), USDC, WETH],
      fetchMock as any,
    );
    expect(result.get(WETH)).toBe(3500.42);
    expect(result.get(USDC)).toBe(1);
    expect(priceLastUpdatedAt(result, WETH)).toBe(1_700_000_000_000);
  });

  it("rejects with OracleFetchError when fetch throws", async () => {
    const cause = new TypeError("network down");
    vi.stubGlobal("fetch", () =>
      Promise.reject(cause),
    );

    await expect(fetchUsdPrices(1, [WETH])).rejects.toMatchObject({
      name: "OracleFetchError",
      cause,
      tokenKeys: [`1:${WETH}`],
    });
  });

  it("rejects with the status code on non-2xx", async () => {
    vi.stubGlobal(
      "fetch",
      async () => new Response("rate limited", { status: 429 }),
    );

    await expect(fetchUsdPrices(1, [WETH])).rejects.toMatchObject({
      name: "OracleFetchError",
      status: 429,
      cause: "rate limited",
      tokenKeys: [`1:${WETH}`],
    });
  });

  it("batches token price requests at 30 addresses per request", async () => {
    const many = Array.from({ length: 45 }, (_, index) => {
      const suffix = index.toString(16).padStart(40, "0");
      return `0x${suffix.slice(-40)}`;
    });
    const fetchMock = vi.fn(async (url: string) => {
      const count =
        new URL(url).searchParams.get("contract_addresses")?.split(",")
          .length ?? 0;
      expect(count).toBeLessThanOrEqual(30);
      return new Response("{}");
    });

    await fetchUsdPrices(1, many, fetchMock as any);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("fetches native prices with /simple/price ids and maps shared ids back to chains", async () => {
    const fetchMock = vi.fn(async (url: string) => {
      expect(url).toContain("/simple/price");
      const ids = new URL(url).searchParams.get("ids");
      expect(ids).toContain("ethereum");
      expect(ids).toContain("matic-network");
      return new Response(
        JSON.stringify({
          ethereum: { usd: 3500, last_updated_at: 1_700_000_000 },
          "matic-network": { usd: 1.1, last_updated_at: 1_700_000_020 },
        }),
      );
    });

    const result = await fetchNativeUsdPrices([1, 10, 137], fetchMock as any);
    expect(result.get(1)).toBe(3500);
    expect(result.get(10)).toBe(3500);
    expect(result.get(137)).toBe(1.1);
    expect(nativePriceLastUpdatedAt(result, 137)).toBe(1_700_000_020_000);
  });
});
