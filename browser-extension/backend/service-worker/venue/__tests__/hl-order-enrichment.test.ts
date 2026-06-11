/**
 * HL order-time ENRICHMENT (the data plane beyond bare leverage): the
 * `{meta, activeAssetData, clearinghouseState}` reads + the computed
 * `order_enrichment` object injected into the v2 evaluate input.
 *
 * Covers:
 *   - hl-info-client      — maxLeverageForIndex (meta), activeAssetDataFor
 *                           (leverage/type/markPx, SHARED with leverageFor),
 *                           clearinghouseStateFor (margin summary + positions)
 *   - collect-order-enrichment — place_order → { markets:{sym:{…}}, account:{…} };
 *                           best-effort `{}` on any miss; correct bps/USD math
 *
 * `Browser.storage.local` is mocked; the HL `/info` fetch is mocked via the
 * client's injectable `fetchImpl`, answering all three `type`s by body.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    browser: {
      runtime: { getURL: (p: string) => `chrome-extension://x/${p}` },
      storage: {
        local: {
          get: vi.fn(async (key: string) => ({ [key]: localStore.get(key) })),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(entries)) localStore.set(k, v);
          }),
        },
      },
    },
  };
});

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

import { HlInfoClient } from "../hl-info-client";
import { setConnectedAccount } from "../hl-master-store";
import { collectOrderEnrichment } from "../collect-order-enrichment";
import type { VenueOrderPayload } from "@lib/types";

const HOST = "app.hyperliquid.xyz";
const MASTER = "0x000000000000000000000000000000000000a01c";
const VAULT = "0x1111111111111111111111111111111111111111";

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

/** Mock `/info` answering meta + activeAssetData + clearinghouseState by type. */
function infoFetch(opts: {
  fail?: boolean;
  /** Override the clearinghouseState response (e.g. no positions). */
  positions?: Array<Record<string, unknown>>;
  noClearinghouse?: boolean;
  /** Override `marginSummary` (e.g. an isolated account with ~$0 perp equity). */
  marginSummary?: Record<string, unknown>;
  /**
   * activeAssetData `availableToTrade` collateral (USD). Default "34000" makes
   * `totalMarginUsed + availableToTrade == accountValue` for the cross fixture.
   * `null` omits the field entirely (the SW then can't compute a spot-aware ratio).
   */
  availableToTrade?: string | null;
} = {}): ReturnType<typeof vi.fn<typeof fetch>> {
  const markPx: Record<string, string> = { BTC: "60000", ETH: "3000" };
  return vi.fn(async (_url: unknown, init?: unknown) => {
    if (opts.fail) throw new Error("network down");
    const body = JSON.parse(
      ((init as RequestInit | undefined)?.body as string) ?? "{}",
    ) as { type?: string; user?: string; coin?: string };
    if (body.type === "meta") {
      return jsonResponse({
        universe: [
          { name: "BTC", maxLeverage: 50 },
          { name: "ETH", maxLeverage: 25 },
          { name: "SOL", maxLeverage: 20 },
        ],
      });
    }
    if (body.type === "activeAssetData") {
      const coin = body.coin ?? "BTC";
      const avail =
        opts.availableToTrade === null
          ? {}
          : {
              availableToTrade: [
                opts.availableToTrade ?? "34000",
                opts.availableToTrade ?? "34000",
              ],
            };
      return jsonResponse({
        user: body.user,
        coin,
        leverage: { type: "cross", value: 26 },
        ...avail,
        markPx: markPx[coin] ?? "100",
      });
    }
    if (body.type === "clearinghouseState") {
      if (opts.noClearinghouse) return jsonResponse({});
      return jsonResponse({
        marginSummary: opts.marginSummary ?? {
          accountValue: "50000",
          totalMarginUsed: "16000",
        },
        assetPositions: opts.positions ?? [
          {
            position: {
              coin: "BTC",
              szi: "0.5",
              returnOnEquity: "-0.15",
              liquidationPx: "55000",
            },
          },
        ],
      });
    }
    return jsonResponse({});
  }) as unknown as ReturnType<typeof vi.fn<typeof fetch>>;
}

/** The built `Perp::PlaceOrder` body. */
const order = (symbol: string, size = "0.1"): Record<string, unknown> => ({
  domain: "perp",
  action: "place_order",
  venue: { name: "hyperliquid", chain: "hyperliquid:mainnet" },
  market: { symbol, venue: { name: "hyperliquid" } },
  side: "long",
  size: { kind: "base_decimal", amount: size },
  reduce_only: false,
  order_type: { kind: "limit", price: "60000", time_in_force: { kind: "gtc" } },
});

const orderPayload = (
  assetIndex: number,
  over: Partial<VenueOrderPayload> = {},
): VenueOrderPayload =>
  ({
    hostname: HOST,
    hlAction: {
      kind: "order",
      order: { a: assetIndex, b: true, p: "60000", s: "0.1", r: false, t: { limit: { tif: "Gtc" } } },
    },
    ...over,
  }) as VenueOrderPayload;

beforeEach(() => {
  mocks.localStore.clear();
  vi.clearAllMocks();
});

describe("HlInfoClient new reads", () => {
  it("maxLeverageForIndex reads the meta universe tier", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch() });
    expect(await client.maxLeverageForIndex(0)).toBe(50);
    expect(await client.maxLeverageForIndex(1)).toBe(25);
    expect(await client.maxLeverageForIndex(10042)).toBeNull(); // spot
  });

  it("activeAssetDataFor returns leverage + type + markPx, shared with leverageFor", async () => {
    const fetchImpl = infoFetch();
    const client = new HlInfoClient({ fetchImpl });
    const d = await client.activeAssetDataFor(MASTER, "BTC");
    expect(d).toEqual({
      leverage: 26,
      leverageType: "cross",
      markPx: 60000,
      availableToTrade: 34000,
    });
    // leverageFor reads the SAME cache entry → no second activeAssetData fetch.
    expect(await client.leverageFor(MASTER, "BTC")).toBe(26);
    const adCalls = fetchImpl.mock.calls.filter((c) => {
      try {
        return (
          JSON.parse(((c[1] as RequestInit).body as string) ?? "{}").type ===
          "activeAssetData"
        );
      } catch {
        return false;
      }
    }).length;
    expect(adCalls).toBe(1);
  });

  it("activeAssetDataFor parses availableToTrade collateral (min of buy/sell)", async () => {
    // availableToTrade is HL's per-(user,coin) USABLE COLLATERAL in USD (spot
    // included), NOT a leveraged notional cap (maxTradeSzs is that). The min of
    // the [buy, sell] pair is the conservative opening-direction figure.
    const client = new HlInfoClient({ fetchImpl: infoFetch() });
    const d = await client.activeAssetDataFor(MASTER, "BTC");
    expect(d?.availableToTrade).toBe(34000);
  });

  it("clearinghouseStateFor parses margin summary + per-coin positions", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch() });
    const s = await client.clearinghouseStateFor(MASTER);
    expect(s?.accountValue).toBe(50000);
    expect(s?.totalMarginUsed).toBe(16000);
    const btc = s?.positions.get("BTC");
    expect(btc).toEqual({ returnOnEquity: -0.15, liquidationPx: 55000, szi: 0.5 });
    expect(s?.positions.has("ETH")).toBe(false);
  });

  it("clearinghouseStateFor returns nulls/empty on a bare shape", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({ noClearinghouse: true }) });
    const s = await client.clearinghouseStateFor(MASTER);
    expect(s?.accountValue).toBeNull();
    expect(s?.positions.size).toBe(0);
  });
});

describe("collectOrderEnrichment", () => {
  it("computes every field for a BTC order with an open position", async () => {
    await setConnectedAccount(HOST, MASTER);
    const client = new HlInfoClient({ fetchImpl: infoFetch() });
    const out = await collectOrderEnrichment(order("BTC"), orderPayload(0), client);
    expect(out).toEqual({
      markets: {
        BTC: {
          max_leverage: 50,
          leverage_type: "cross",
          notional_usd: 6000, // 0.1 × 60000
          position_roe_bps: -1500, // -0.15 × 10000
          liquidation_distance_bps: 833, // |60000-55000|/60000 × 10000
          has_open_position: true,
        },
      },
      account: {
        account_value_usd: 50000,
        margin_used_ratio_bps: 3200, // 16000/50000 × 10000
      },
    });
  });

  it("omits position fields and sets has_open_position=false with no position in this market", async () => {
    await setConnectedAccount(HOST, MASTER);
    const client = new HlInfoClient({ fetchImpl: infoFetch() }); // positions only has BTC
    const out = await collectOrderEnrichment(order("ETH"), orderPayload(1), client);
    expect(out.markets?.ETH).toEqual({
      max_leverage: 25,
      leverage_type: "cross",
      notional_usd: 300, // 0.1 × 3000
      has_open_position: false,
    });
    expect(out.markets?.ETH).not.toHaveProperty("position_roe_bps");
    expect(out.markets?.ETH).not.toHaveProperty("liquidation_distance_bps");
    expect(out.account).toEqual({ account_value_usd: 50000, margin_used_ratio_bps: 3200 });
  });

  it("counts spot collateral via availableToTrade, not perp-only accountValue", async () => {
    // Real isolated-account shape (0x676f…9a54): perp marginSummary.accountValue
    // is $0.96 fully committed to a BTC isolated position, while $116 of spot
    // USDC is usable as collateral (HL availableToTrade). The perp-only ratio
    // (0.96/0.96 = 100%) is a false positive; the spot-aware ratio is ~0.8%.
    await setConnectedAccount(HOST, MASTER);
    const client = new HlInfoClient({
      fetchImpl: infoFetch({
        marginSummary: { accountValue: "0.958269", totalMarginUsed: "0.958269" },
        availableToTrade: "116.453994",
        positions: [],
      }),
    });
    const out = await collectOrderEnrichment(order("ETH"), orderPayload(1), client);
    // 0.958269 / (0.958269 + 116.453994) × 10000 = 81.6 → 82  (was 10000)
    expect(out.account?.margin_used_ratio_bps).toBe(82);
  });

  it("omits margin_used_ratio_bps when availableToTrade is absent (no perp-only fallback)", async () => {
    // Without availableToTrade the spot-aware denominator is unknown; rather than
    // fall back to the perp-only accountValue (which reintroduces the FP), the
    // ratio is omitted → the margin-health policy stays dormant (best-effort).
    await setConnectedAccount(HOST, MASTER);
    const client = new HlInfoClient({
      fetchImpl: infoFetch({
        marginSummary: { accountValue: "0.958269", totalMarginUsed: "0.958269" },
        availableToTrade: null,
        positions: [],
      }),
    });
    const out = await collectOrderEnrichment(order("ETH"), orderPayload(1), client);
    expect(out.account ?? {}).not.toHaveProperty("margin_used_ratio_bps");
  });

  it("uses vaultAddress as the master", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch() });
    const out = await collectOrderEnrichment(
      order("BTC"),
      orderPayload(0, { vaultAddress: VAULT }),
      client,
    );
    expect(out.markets?.BTC?.max_leverage).toBe(50);
  });

  it("returns {} for a non-order action", async () => {
    await setConnectedAccount(HOST, MASTER);
    const client = new HlInfoClient({ fetchImpl: infoFetch() });
    const withdraw = { action: "hl_withdraw", destination: VAULT, amount: "1" };
    expect(await collectOrderEnrichment(withdraw, orderPayload(0), client)).toEqual({});
  });

  it("returns {} when the master is unknown (best-effort dormancy)", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch() });
    expect(await collectOrderEnrichment(order("BTC"), orderPayload(0), client)).toEqual({});
  });

  it("never throws on a network failure (best-effort)", async () => {
    await setConnectedAccount(HOST, MASTER);
    const client = new HlInfoClient({ fetchImpl: infoFetch({ fail: true }) });
    const out = await collectOrderEnrichment(order("BTC"), orderPayload(0), client);
    // No coin / markPx / clearinghouse resolved → fully empty, but never throws.
    expect(out).toEqual({});
  });
});

// LIVE end-to-end against the real HL `/info` endpoint (gated by HL_LIVE=1 so it
// never runs in CI). Drives the REAL `collectOrderEnrichment` (real `fetch`,
// real `{meta, activeAssetData, clearinghouseState}` queries) against a real
// account that holds open positions, and asserts the full `order_enrichment`
// parses + computes. Proves real order info → e2e parse.
const LIVE = process.env.HL_LIVE === "1";
(LIVE ? describe : describe.skip)("LIVE: real HL /info → order_enrichment", () => {
  it("fetches + parses a populated order_enrichment from the real endpoint", async () => {
    // A real Hyperliquid account with many open positions (incl. BTC).
    const REAL = "0x010461c14e146ac35fe42271bdc1134ee31c703a";
    const client = new HlInfoClient({ fetchImpl: globalThis.fetch, timeoutMs: 8000 });
    const out = await collectOrderEnrichment(
      order("BTC", "0.1"),
      orderPayload(0, { vaultAddress: REAL }),
      client,
    );
    // eslint-disable-next-line no-console
    console.log("LIVE order_enrichment:", JSON.stringify(out));
    const btc = out.markets?.BTC;
    expect(btc?.max_leverage).toBeGreaterThan(0); // meta
    expect(typeof btc?.leverage_type).toBe("string"); // activeAssetData
    expect(btc?.notional_usd).toBeGreaterThan(0); // 0.1 × real markPx
    expect(btc?.has_open_position).toBe(true); // clearinghouseState (this account holds BTC)
    expect(typeof btc?.position_roe_bps).toBe("number"); // returnOnEquity × 10000
    expect(typeof btc?.liquidation_distance_bps).toBe("number"); // |markPx-liqPx|/markPx × 10000
    expect(out.account?.account_value_usd).toBeGreaterThan(0); // marginSummary.accountValue
    expect(out.account?.margin_used_ratio_bps).toBeGreaterThanOrEqual(0); // totalMarginUsed/accountValue
  }, 30000);
});
