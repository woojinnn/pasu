/**
 * HL order-time leverage collection for the `account_leverage` lowering
 * enrichment — the venue analogue of `registry/collect-token-decimals.ts`.
 *
 * The HL `order` wire carries no leverage; it is per-(user,asset) account state
 * the venue applies at fill. For an `hl_order`, the SW resolves the master
 * account ({@link resolveHlMaster}), maps the numeric `asset_index` to its coin
 * symbol, and fetches the effective leverage from `activeAssetData` — then
 * injects `{ "<asset_index>": <leverage> }` into the v2 evaluate input so the
 * lowering fills `context.leverage` and an order-leverage policy can fire.
 *
 * NON-FATAL by design (mirrors `collectTokenDecimals`): any miss (non-order
 * action, unknown master, spot asset, info-fetch error/timeout) yields `{}` and
 * the function NEVER throws — a transient HL hiccup must not flip a venue order
 * to deny-closed; the leverage field simply stays absent (policy dormant).
 */
import type { VenueOrderPayload } from "@lib/types";
import { defaultHlInfoClient, type HlInfoClient } from "./hl-info-client";
import { resolveHlMaster } from "./resolve-hl-master";

/**
 * The action tag whose context carries the order-leverage field. HL orders and
 * TWAPs now both decode to the unified `Perp::PlaceOrder` (`place_order`), so a
 * single tag covers both — a TWAP cannot evade an order-leverage cap.
 */
const ORDER_TAG = "place_order";
/** The leverage-change tag that triggers a cache invalidation (HL
 * updateLeverage now decodes to the generic `Perp::ChangeLeverage`). */
const UPDATE_LEVERAGE_TAG = "change_leverage";

function asAssetIndex(value: unknown): number | null {
  return typeof value === "number" && Number.isInteger(value) && value >= 0
    ? value
    : null;
}

/**
 * The numeric HL asset index for an order / twap payload — read from the raw
 * wire (`payload.hlAction`), since the built `Perp::PlaceOrder` body no longer
 * carries it (it carries `market.symbol` instead).
 */
function assetIndexFromPayload(payload: VenueOrderPayload): number | null {
  const wire = payload.hlAction;
  if (!wire) return null;
  if (wire.kind === "order") return asAssetIndex(wire.order.a);
  if (wire.kind === "twap_order") return asAssetIndex(wire.assetIndex);
  if (wire.kind === "update_leverage") return asAssetIndex(wire.assetIndex);
  return null;
}

/** The market symbol the built `Perp::PlaceOrder` body carries (the key the
 * lowering looks `account_leverage` up by). */
function bodyMarketSymbol(action: Record<string, unknown>): string | null {
  const market = action.market;
  if (market && typeof market === "object") {
    const symbol = (market as { symbol?: unknown }).symbol;
    if (typeof symbol === "string" && symbol.length > 0) return symbol;
  }
  return null;
}

/**
 * Resolve `{ "<market_symbol>": leverage }` for an order-class action
 * (`place_order`), or `{}` for any other action / unresolved input. `action` is
 * the built `Perp::PlaceOrder` body (`{ action:"place_order", market:{symbol},
 * … }`); the numeric asset index is read from `payload.hlAction`. The result is
 * keyed by the body's `market.symbol` so the lowering (which looks leverage up
 * by symbol) finds it.
 */
export async function collectHlLeverage(
  action: Record<string, unknown>,
  payload: VenueOrderPayload,
  client: HlInfoClient = defaultHlInfoClient(),
): Promise<Record<string, number>> {
  try {
    if (action.action !== ORDER_TAG) return {};
    const assetIndex = assetIndexFromPayload(payload);
    if (assetIndex === null) return {};
    const symbol = bodyMarketSymbol(action);
    if (symbol === null) return {};

    const master = await resolveHlMaster(payload);
    if (!master) {
      console.info(
        "[Pasu] HL order-leverage: no master account resolved " +
          "(vaultAddress / wallet_id / stored all empty) → leverage omitted, policy dormant",
        { assetIndex },
      );
      return {};
    }

    const coin = await client.coinForIndex(assetIndex);
    if (!coin) {
      console.info(
        "[Pasu] HL order-leverage: coin unresolved (spot index or meta miss) → leverage omitted",
        { assetIndex, master },
      );
      return {};
    }

    const leverage = await client.leverageFor(master, coin);
    if (leverage === null) {
      console.info(
        "[Pasu] HL order-leverage: activeAssetData returned no leverage → omitted",
        { master, coin },
      );
      return {};
    }

    console.info("[Pasu] HL order-leverage resolved", {
      master,
      coin,
      assetIndex,
      symbol,
      leverage,
    });
    // Keyed by the body's market symbol (what the lowering looks up by).
    return { [symbol]: leverage };
  } catch (err) {
    // Never let leverage collection break (or deny-close) the verdict path.
    console.warn("[Pasu] HL order-leverage collection threw (omitted)", {
      err: err instanceof Error ? err.message : String(err),
    });
    return {};
  }
}

/**
 * When the SW intercepts a leverage change (HL updateLeverage → the generic
 * `change_leverage` action), INVALIDATE the cached leverage for (master, coin)
 * so the NEXT order on that asset re-fetches the authoritative
 * `activeAssetData`. The numeric asset index is read from the wire payload.
 *
 * SECURITY: we deliberately do NOT seed the cache from the page-asserted wire
 * `leverage` value. That value is unauthenticated MAIN-world input; seeding a
 * deny-path cache from it would let an adversarial / compromised frontend
 * poison the cache with a low value — e.g. a (never-broadcast)
 * `updateLeverage{leverage:1}` — so a later genuine high-leverage order reads
 * the poisoned `1` and a `context.leverage > N` cap stays silent (under-block).
 * Invalidation forces a trusted re-read instead. Fire-and-forget; never throws.
 */
export async function noteHlLeverageUpdate(
  action: Record<string, unknown>,
  payload: VenueOrderPayload,
  client: HlInfoClient = defaultHlInfoClient(),
): Promise<void> {
  try {
    if (action.action !== UPDATE_LEVERAGE_TAG) return;
    const assetIndex = assetIndexFromPayload(payload);
    if (assetIndex === null) return;

    const master = await resolveHlMaster(payload);
    if (!master) return;
    const coin = await client.coinForIndex(assetIndex);
    if (!coin) return;

    // Invalidate (NOT set-from-wire) — see the SECURITY note above.
    client.invalidate(master, coin);
  } catch {
    // Best-effort — a failure just means the next order re-fetches anyway.
  }
}
