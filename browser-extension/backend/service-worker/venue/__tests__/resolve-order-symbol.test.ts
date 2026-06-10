/**
 * HL order-time SYMBOL resolution: `resolveOrderSymbol` patches the real asset
 * name (HL meta universe) into the built perp body's `market.symbol`, replacing
 * the `ASSET-<index>` wire placeholder so a symbol-matching policy (e.g. an
 * order-symbol allowlist) sees "BTC" not "ASSET-0".
 *
 * Verifies:
 *   - resolves + patches in place for every market-bearing wire kind
 *     (order / twap_order / update_leverage / update_isolated_margin)
 *   - best-effort dormancy (spot index / meta miss / fetch error / non-market
 *     action) leaves the placeholder and NEVER throws
 *   - CONSISTENCY: after the patch, `collectOrderEnrichment` keys its per-market
 *     map by the SAME resolved symbol (the regression this whole wiring prevents)
 *   - LIVE (HL_LIVE=1): the real meta universe maps 0→BTC / 1→ETH / 5→SOL
 *
 * The HL `/info` fetch is mocked via the client's injectable `fetchImpl`.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

// The import chain (resolve-hl-master → hl-master-store) touches
// `browser.storage`; mock the polyfill (the standard Map-backed pattern) so the
// module loads outside a real extension.
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
import { resolveOrderSymbol } from "../resolve-order-symbol";
import { collectOrderEnrichment } from "../collect-order-enrichment";
import type { VenueOrderPayload } from "@lib/types";

const HOST = "app.hyperliquid.xyz";
const VAULT = "0x1111111111111111111111111111111111111111";

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

/**
 * A mock `/info` fetch answering `meta` (universe), `activeAssetData` and
 * `clearinghouseState` by body type. `fail` throws (network error).
 */
function infoFetch(opts: {
  universe?: string[];
  markPx?: string;
  fail?: boolean;
}): ReturnType<typeof vi.fn<typeof fetch>> {
  return vi.fn(async (_url: unknown, init?: unknown) => {
    if (opts.fail) throw new Error("network down");
    const body = JSON.parse(
      ((init as RequestInit | undefined)?.body as string) ?? "{}",
    ) as { type?: string; user?: string; coin?: string };
    if (body.type === "meta") {
      const names = opts.universe ?? ["BTC", "ETH", "ATOM", "MATIC", "DYDX", "SOL"];
      return jsonResponse({
        universe: names.map((name) => ({ name, maxLeverage: 20 })),
      });
    }
    if (body.type === "activeAssetData") {
      return jsonResponse({
        user: body.user,
        coin: body.coin,
        leverage: { type: "cross", value: 5 },
        markPx: opts.markPx ?? "60000",
      });
    }
    if (body.type === "clearinghouseState") {
      return jsonResponse({
        marginSummary: { accountValue: "1000", totalMarginUsed: "100" },
        assetPositions: [],
      });
    }
    return jsonResponse({});
  }) as unknown as ReturnType<typeof vi.fn<typeof fetch>>;
}

/** Minimal payload — only the fields resolve/collect read. */
function payload(over: Partial<VenueOrderPayload> = {}): VenueOrderPayload {
  return { hostname: HOST, ...over } as VenueOrderPayload;
}

/** The built perp body, `market.symbol` carrying the `ASSET-<index>` placeholder. */
function body(
  symbol: string,
  over: Partial<Record<string, unknown>> = {},
): Record<string, unknown> {
  return {
    domain: "perp",
    action: "place_order",
    venue: { name: "hyperliquid", chain: "hyperliquid:mainnet" },
    market: { symbol, venue: { name: "hyperliquid" } },
    side: "long",
    size: { kind: "base_decimal", amount: "0.1" },
    reduce_only: false,
    order_type: { kind: "limit", price: "60000", time_in_force: { kind: "gtc" } },
    ...over,
  };
}

const orderPayload = (
  assetIndex: number,
  over: Partial<VenueOrderPayload> = {},
): VenueOrderPayload =>
  payload({
    hlAction: {
      kind: "order",
      order: { a: assetIndex, b: true, p: "60000", s: "0.1", r: false, t: { limit: { tif: "Gtc" } } },
    },
    ...over,
  } as Partial<VenueOrderPayload>);

function symbolOf(action: Record<string, unknown>): unknown {
  return (action.market as { symbol?: unknown }).symbol;
}

beforeEach(() => {
  mocks.localStore.clear();
  vi.clearAllMocks();
});

describe("resolveOrderSymbol", () => {
  it("resolves a perp index → coin and patches market.symbol in place", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({}) });
    const action = body("ASSET-0");
    const resolved = await resolveOrderSymbol(action, orderPayload(0), client);
    expect(resolved).toBe("BTC");
    expect(symbolOf(action)).toBe("BTC");
  });

  it("resolves a higher index (5 → SOL)", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({}) });
    const action = body("ASSET-5");
    expect(await resolveOrderSymbol(action, orderPayload(5), client)).toBe("SOL");
    expect(symbolOf(action)).toBe("SOL");
  });

  it("keeps the placeholder for a spot index (>= 10000, no perp meta)", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({}) });
    const action = body("ASSET-10042");
    expect(await resolveOrderSymbol(action, orderPayload(10042), client)).toBeNull();
    expect(symbolOf(action)).toBe("ASSET-10042"); // untouched
  });

  it("keeps the placeholder on a meta miss (index past the universe)", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({ universe: ["BTC", "ETH"] }) });
    const action = body("ASSET-9");
    expect(await resolveOrderSymbol(action, orderPayload(9), client)).toBeNull();
    expect(symbolOf(action)).toBe("ASSET-9");
  });

  it("keeps the placeholder on a fetch error and NEVER throws", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({ fail: true }) });
    const action = body("ASSET-0");
    expect(await resolveOrderSymbol(action, orderPayload(0), client)).toBeNull();
    expect(symbolOf(action)).toBe("ASSET-0");
  });

  it("is a no-op for a non-market action (no market.symbol)", async () => {
    const fetchImpl = infoFetch({});
    const client = new HlInfoClient({ fetchImpl });
    const withdraw = { domain: "hyperliquid_core", action: "hl_withdraw", destination: VAULT, amount: "1" };
    expect(await resolveOrderSymbol(withdraw, payload(), client)).toBeNull();
    expect(fetchImpl).not.toHaveBeenCalled(); // never even fetches meta
  });

  it("resolves a TWAP order (reads the asset index from the twap wire)", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({}) });
    const action = body("ASSET-1", {
      order_type: { kind: "twap", duration_minutes: 30, randomize: true },
    });
    const twapPayload = payload({
      hlAction: {
        kind: "twap_order",
        assetIndex: 1,
        isBuy: true,
        size: "10",
        reduceOnly: false,
        minutes: 30,
        randomize: true,
      },
    } as Partial<VenueOrderPayload>);
    expect(await resolveOrderSymbol(action, twapPayload, client)).toBe("ETH");
    expect(symbolOf(action)).toBe("ETH");
  });

  it("resolves a change_leverage body (update_leverage wire)", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({}) });
    const action: Record<string, unknown> = {
      domain: "perp",
      action: "change_leverage",
      venue: { name: "hyperliquid", chain: "hyperliquid:mainnet" },
      market: { symbol: "ASSET-5", venue: { name: "hyperliquid" } },
      new_leverage: "10",
    };
    const p = payload({
      hlAction: { kind: "update_leverage", assetIndex: 5, isCross: true, leverage: 10 },
    } as Partial<VenueOrderPayload>);
    expect(await resolveOrderSymbol(action, p, client)).toBe("SOL");
    expect(symbolOf(action)).toBe("SOL");
  });

  it("resolves an adjust_margin body (update_isolated_margin wire)", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({}) });
    const action: Record<string, unknown> = {
      domain: "perp",
      action: "adjust_margin",
      venue: { name: "hyperliquid", chain: "hyperliquid:mainnet" },
      market: { symbol: "ASSET-0", venue: { name: "hyperliquid" } },
      side: "long",
      delta: "100",
    };
    const p = payload({
      hlAction: { kind: "update_isolated_margin", assetIndex: 0, isBuy: true, ntli: "100" },
    } as Partial<VenueOrderPayload>);
    expect(await resolveOrderSymbol(action, p, client)).toBe("BTC");
    expect(symbolOf(action)).toBe("BTC");
  });

  // ── CONSISTENCY: the whole point — after the patch, the enrichment collector
  //    keys its per-market map by the SAME resolved symbol (so the lowering, which
  //    reads `market.symbol`, finds it). Without the patch the enrichment would be
  //    keyed by "ASSET-0" and the lowering would look up "BTC" → silent drop. ──
  it("patches symbol so collectOrderEnrichment keys its market by the resolved name", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({ markPx: "60000" }) });
    const action = body("ASSET-0");
    const p = orderPayload(0, { vaultAddress: VAULT }); // resolvable master

    await resolveOrderSymbol(action, p, client);
    expect(symbolOf(action)).toBe("BTC");

    const enr = await collectOrderEnrichment(action, p, client);
    // markets is keyed by the patched symbol, NOT "ASSET-0".
    expect(enr.markets).toBeDefined();
    expect(Object.keys(enr.markets ?? {})).toEqual(["BTC"]);
    // and it carries computed fields (notional = 0.1 × 60000 = 6000).
    expect(enr.markets?.BTC?.notional_usd).toBe(6000);
  });
});

// ── LIVE (opt-in: HL_LIVE=1) — real meta universe index → name ───────────────
const LIVE = process.env.HL_LIVE === "1";
(LIVE ? describe : describe.skip)("resolveOrderSymbol (LIVE real HL meta)", () => {
  it("resolves real perp indices 0→BTC, 1→ETH, 5→SOL", async () => {
    const client = new HlInfoClient({}); // real fetch, real api.hyperliquid.xyz
    for (const [idx, name] of [[0, "BTC"], [1, "ETH"], [5, "SOL"]] as const) {
      const action = body(`ASSET-${idx}`);
      const resolved = await resolveOrderSymbol(action, orderPayload(idx), client);
      expect(resolved).toBe(name);
      expect(symbolOf(action)).toBe(name);
    }
  }, 15_000);
});
