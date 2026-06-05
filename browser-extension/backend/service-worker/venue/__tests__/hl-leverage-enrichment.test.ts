/**
 * HL order-time leverage enrichment (Phase 2): the SW-local `activeAssetData`
 * path that fills the order `account_leverage` injected into the v2 evaluate
 * input.
 *
 * Covers the four modules:
 *   - hl-info-client     — meta (asset_index→coin) + activeAssetData (leverage)
 *                          caches, TTL/set/invalidate, miss→null
 *   - hl-master-store    — per-origin connected-account get/set/clear
 *   - resolve-hl-master  — vaultAddress > connected > null priority
 *   - collect-hl-leverage — hl_order → { idx: leverage }; best-effort `{}` misses
 *
 * `Browser.storage.local` is mocked (the standard `Map`-backed pattern); the HL
 * `/info` fetch is mocked via the client's injectable `fetchImpl`.
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
import {
  clearConnectedAccount,
  getConnectedAccount,
  setConnectedAccount,
} from "../hl-master-store";
import { resolveHlMaster } from "../resolve-hl-master";
import {
  collectHlLeverage,
  noteHlLeverageUpdate,
} from "../collect-hl-leverage";
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

/** A mock `/info` fetch that answers `meta` and `activeAssetData` by body type. */
function infoFetch(opts: {
  universe?: string[];
  leverage?: number | null;
  fail?: boolean;
}): ReturnType<typeof vi.fn<typeof fetch>> {
  return vi.fn(async (_url: unknown, init?: unknown) => {
    if (opts.fail) throw new Error("network down");
    const body = JSON.parse(
      ((init as RequestInit | undefined)?.body as string) ?? "{}",
    ) as { type?: string; user?: string; coin?: string };
    if (body.type === "meta") {
      const names = opts.universe ?? ["BTC", "ETH", "ATOM", "MATIC", "DYDX", "SOL"];
      return jsonResponse({ universe: names.map((name) => ({ name })) });
    }
    if (body.type === "activeAssetData") {
      if (opts.leverage == null) return jsonResponse({ user: body.user, coin: body.coin });
      return jsonResponse({
        user: body.user,
        coin: body.coin,
        leverage: { type: "cross", value: opts.leverage },
      });
    }
    return jsonResponse({});
  }) as unknown as ReturnType<typeof vi.fn<typeof fetch>>;
}

/** Minimal payload — only the fields resolve/collect read. */
function payload(over: Partial<VenueOrderPayload> = {}): VenueOrderPayload {
  return { hostname: HOST, ...over } as VenueOrderPayload;
}

beforeEach(() => {
  mocks.localStore.clear();
  vi.clearAllMocks();
});

describe("HlInfoClient.coinForIndex", () => {
  it("resolves perp index → symbol and caches the universe (one fetch)", async () => {
    const fetchImpl = infoFetch({});
    const client = new HlInfoClient({ fetchImpl });
    expect(await client.coinForIndex(0)).toBe("BTC");
    expect(await client.coinForIndex(5)).toBe("SOL");
    expect(fetchImpl).toHaveBeenCalledTimes(1); // universe cached
  });

  it("returns null for a spot index (>= 10000) without fetching", async () => {
    const fetchImpl = infoFetch({});
    const client = new HlInfoClient({ fetchImpl });
    expect(await client.coinForIndex(10042)).toBeNull();
    expect(fetchImpl).not.toHaveBeenCalled();
  });
});

describe("HlInfoClient.leverageFor", () => {
  it("reads activeAssetData leverage.value and caches per (user,coin)", async () => {
    const fetchImpl = infoFetch({ leverage: 26 });
    const client = new HlInfoClient({ fetchImpl });
    expect(await client.leverageFor(MASTER, "BTC")).toBe(26);
    expect(await client.leverageFor(MASTER, "BTC")).toBe(26);
    expect(fetchImpl).toHaveBeenCalledTimes(1); // cached
  });

  it("returns null on a network error (best-effort miss)", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({ fail: true }) });
    expect(await client.leverageFor(MASTER, "BTC")).toBeNull();
  });

  it("returns null when the shape lacks leverage.value", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({ leverage: null }) });
    expect(await client.leverageFor(MASTER, "BTC")).toBeNull();
  });

  it("set() seeds the cache so leverageFor needs no fetch", async () => {
    const fetchImpl = infoFetch({ leverage: 5 });
    const client = new HlInfoClient({ fetchImpl });
    client.set(MASTER, "BTC", 13);
    expect(await client.leverageFor(MASTER, "BTC")).toBe(13);
    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it("invalidate() forces a refetch", async () => {
    const fetchImpl = infoFetch({ leverage: 7 });
    const client = new HlInfoClient({ fetchImpl });
    client.set(MASTER, "BTC", 7);
    client.invalidate(MASTER, "BTC");
    expect(await client.leverageFor(MASTER, "BTC")).toBe(7);
    expect(fetchImpl).toHaveBeenCalledTimes(1);
  });
});

describe("hl-master-store", () => {
  it("round-trips a connected account, lowercased", async () => {
    await setConnectedAccount(HOST, MASTER.toUpperCase());
    expect(await getConnectedAccount(HOST)).toBe(MASTER.toLowerCase());
  });

  it("ignores an invalid address", async () => {
    await setConnectedAccount(HOST, "0xnotanaddress");
    expect(await getConnectedAccount(HOST)).toBeNull();
  });

  it("clears an origin's account", async () => {
    await setConnectedAccount(HOST, MASTER);
    await clearConnectedAccount(HOST);
    expect(await getConnectedAccount(HOST)).toBeNull();
  });
});

describe("resolveHlMaster", () => {
  const WALLET = "0x2222222222222222222222222222222222222222";

  it("prefers vaultAddress over wallet_id and the stored account", async () => {
    await setConnectedAccount(HOST, MASTER);
    const m = await resolveHlMaster(
      payload({
        vaultAddress: VAULT,
        wallet_id: { address: WALLET, chains: [] },
      }),
    );
    expect(m).toBe(VAULT.toLowerCase());
  });

  it("uses the fetch-hook-stamped wallet_id when there is no vault", async () => {
    await setConnectedAccount(HOST, MASTER); // store present...
    const m = await resolveHlMaster(
      payload({ wallet_id: { address: WALLET, chains: [] } }),
    );
    expect(m).toBe(WALLET.toLowerCase()); // ...but wallet_id wins over it
  });

  it("falls back to the stored connected account for the origin", async () => {
    await setConnectedAccount(HOST, MASTER);
    expect(await resolveHlMaster(payload())).toBe(MASTER.toLowerCase());
  });

  it("returns null when none is known", async () => {
    expect(await resolveHlMaster(payload())).toBeNull();
  });
});

describe("collectHlLeverage", () => {
  const order = (assetIndex: number): Record<string, unknown> => ({
    domain: "hyperliquid_core",
    action: "hl_order",
    asset_index: assetIndex,
    is_buy: true,
    price: "60000",
    size: "0.1",
    reduce_only: false,
    tif: "gtc",
  });

  it("returns { idx: leverage } for an hl_order with a resolvable master", async () => {
    await setConnectedAccount(HOST, MASTER);
    const client = new HlInfoClient({ fetchImpl: infoFetch({ leverage: 26 }) });
    const out = await collectHlLeverage(order(0), payload(), client);
    expect(out).toEqual({ "0": 26 });
  });

  it("returns {} for a non-order action", async () => {
    await setConnectedAccount(HOST, MASTER);
    const client = new HlInfoClient({ fetchImpl: infoFetch({ leverage: 26 }) });
    const withdraw = { action: "hl_withdraw", destination: VAULT, amount: "1" };
    expect(await collectHlLeverage(withdraw, payload(), client)).toEqual({});
  });

  it("returns {} when the master is unknown (best-effort dormancy)", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({ leverage: 26 }) });
    expect(await collectHlLeverage(order(0), payload(), client)).toEqual({});
  });

  it("returns {} for a spot asset_index (no perp leverage)", async () => {
    await setConnectedAccount(HOST, MASTER);
    const client = new HlInfoClient({ fetchImpl: infoFetch({ leverage: 26 }) });
    expect(await collectHlLeverage(order(10042), payload(), client)).toEqual({});
  });

  it("returns {} when the leverage lookup misses", async () => {
    await setConnectedAccount(HOST, MASTER);
    const client = new HlInfoClient({ fetchImpl: infoFetch({ leverage: null }) });
    expect(await collectHlLeverage(order(0), payload(), client)).toEqual({});
  });

  it("uses vaultAddress as the master when present", async () => {
    const client = new HlInfoClient({ fetchImpl: infoFetch({ leverage: 11 }) });
    const out = await collectHlLeverage(
      order(1),
      payload({ vaultAddress: VAULT }),
      client,
    );
    expect(out).toEqual({ "1": 11 });
  });

  it("ALSO enriches an hl_twap_order (closes the TWAP bypass of the order-leverage cap)", async () => {
    await setConnectedAccount(HOST, MASTER);
    const client = new HlInfoClient({ fetchImpl: infoFetch({ leverage: 26 }) });
    const twap: Record<string, unknown> = {
      domain: "hyperliquid_core",
      action: "hl_twap_order",
      asset_index: 0,
      is_buy: true,
      size: "10",
      reduce_only: false,
      minutes: 30,
      randomize: true,
    };
    expect(await collectHlLeverage(twap, payload(), client)).toEqual({ "0": 26 });
  });
});

describe("noteHlLeverageUpdate (invalidation, NOT page-seed)", () => {
  function activeAssetDataCalls(
    fetchImpl: ReturnType<typeof vi.fn<typeof fetch>>,
  ): number {
    return fetchImpl.mock.calls.filter((c) => {
      try {
        return (
          JSON.parse(((c[1] as RequestInit).body as string) ?? "{}").type ===
          "activeAssetData"
        );
      } catch {
        return false;
      }
    }).length;
  }

  it("invalidates the cache so the next order re-fetches authoritative leverage — the page wire value is NEVER served", async () => {
    await setConnectedAccount(HOST, MASTER);
    // Authoritative API value is 99; the page will lie and claim 1.
    const fetchImpl = infoFetch({ leverage: 99 });
    const client = new HlInfoClient({ fetchImpl });

    // Prime the cache with the authoritative value.
    expect(await client.leverageFor(MASTER, "BTC")).toBe(99);

    // A page-asserted updateLeverage claiming leverage:1 must NOT poison the
    // deny-path cache (the historical under-block vector).
    const update = {
      domain: "hyperliquid_core",
      action: "hl_update_leverage",
      asset_index: 0,
      is_cross: true,
      leverage: 1,
    };
    await noteHlLeverageUpdate(update, payload(), client);

    // The next read returns the AUTHORITATIVE 99 — never the wire-asserted 1 —
    // and a fresh activeAssetData fetch happened (cache was invalidated, not seeded).
    expect(await client.leverageFor(MASTER, "BTC")).toBe(99);
    expect(activeAssetDataCalls(fetchImpl)).toBeGreaterThanOrEqual(2);
  });

  it("does nothing for a non-updateLeverage action", async () => {
    await setConnectedAccount(HOST, MASTER);
    const fetchImpl = infoFetch({ leverage: 5 });
    const client = new HlInfoClient({ fetchImpl });
    client.set(MASTER, "BTC", 5);
    await noteHlLeverageUpdate(
      { domain: "hyperliquid_core", action: "hl_order", asset_index: 0 },
      payload(),
      client,
    );
    // Cache untouched → served from cache, no activeAssetData fetch.
    expect(await client.leverageFor(MASTER, "BTC")).toBe(5);
    expect(activeAssetDataCalls(fetchImpl)).toBe(0);
  });
});
