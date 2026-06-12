/**
 * HL order-time ENRICHMENT collection — the data plane for the order-risk policy
 * surface, beyond the bare `leverage` (which keeps its own `collect-hl-leverage`
 * path / `account_leverage` injection). The venue analogue of
 * `registry/collect-token-decimals.ts`, extended to three concurrent HL `/info`
 * queries.
 *
 * For a `place_order` (incl. TWAP) the SW resolves the master account, then —
 * once master + coin are known — fires the info queries CONCURRENTLY:
 *   - `meta`             → this market's `maxLeverage` tier (cached, ~free).
 *   - `activeAssetData`  → `leverageType` (cross/isolated) + `markPx`. SHARES the
 *                          fetch with the leverage path (`leverageFor` delegates
 *                          to `activeAssetDataFor`) so it is not fetched twice.
 *   - `clearinghouseState` → account margin health + this market's existing
 *                          position (PnL / liquidation proximity).
 * It then computes the pre-scaled comparable `Long`s the lowering emits as
 * `Perp::PlaceOrderContext` siblings (USD = integer dollars, ratios = bps).
 *
 * NON-FATAL by design (mirrors `collectHlLeverage` / `collectTokenDecimals`): any
 * miss (non-order action, unknown master, info-fetch error/timeout, no position)
 * yields a partial / empty object and the function NEVER throws — a transient HL
 * hiccup must not flip a venue order to deny-closed; each unresolved field is
 * simply omitted (a `context has <field>` policy stays dormant).
 */
import type { VenueOrderPayload } from "@lib/types";
import {
  assetIndexFromPayload,
  bodyMarketSymbol,
} from "./collect-hl-leverage";
import { defaultHlInfoClient, type HlInfoClient } from "./hl-info-client";
import { resolveHlMaster } from "./resolve-hl-master";

/** The action tag whose context carries the order-enrichment fields. */
const ORDER_TAG = "place_order";

/** Per-market enrichment wire (snake_case → Rust `MarketEnrichment`). */
interface MarketEnrichmentWire {
  max_leverage?: number;
  leverage_type?: string;
  notional_usd?: number;
  position_roe_bps?: number;
  liquidation_distance_bps?: number;
  has_open_position?: boolean;
}

/** Account-wide enrichment wire (snake_case → Rust `AccountEnrichment`). */
interface AccountEnrichmentWire {
  account_value_usd?: number;
  margin_used_ratio_bps?: number;
}

/** The `order_enrichment` v2-input object (→ Rust `OrderEnrichment`). */
export interface OrderEnrichmentWire {
  markets?: Record<string, MarketEnrichmentWire>;
  account?: AccountEnrichmentWire;
}

/** Order size (base units) the built `Perp::PlaceOrder` body carries. */
function orderSize(action: Record<string, unknown>): number | null {
  const size = action.size;
  if (size && typeof size === "object") {
    const amount = (size as { amount?: unknown }).amount;
    if (typeof amount === "string" && amount.length > 0) {
      const n = Number(amount);
      return Number.isFinite(n) ? n : null;
    }
    if (typeof amount === "number" && Number.isFinite(amount)) return amount;
  }
  return null;
}

/** Round to a finite integer, or `undefined` if the input is not usable. */
function roundOrUndef(n: number | null | undefined): number | undefined {
  return typeof n === "number" && Number.isFinite(n) ? Math.round(n) : undefined;
}

/**
 * Drop `undefined`-valued keys so the wire omits them (→ serde `None`). The
 * return type strips `undefined` from each value (the keys are genuinely absent
 * at runtime), satisfying `exactOptionalPropertyTypes`.
 */
function compact<T extends Record<string, unknown>>(
  o: T,
): { [K in keyof T]?: Exclude<T[K], undefined> } {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(o)) if (v !== undefined) out[k] = v;
  return out as { [K in keyof T]?: Exclude<T[K], undefined> };
}

/**
 * Resolve the `order_enrichment` object for an order-class action, or `{}` for
 * any other action / unresolved input. `action` is the built `Perp::PlaceOrder`
 * body; the numeric asset index is read from `payload.hlAction`. Per-market
 * fields are keyed by the body's `market.symbol` (the key the lowering looks up
 * by — the same convention as `account_leverage`).
 */
export async function collectOrderEnrichment(
  action: Record<string, unknown>,
  payload: VenueOrderPayload,
  client: HlInfoClient = defaultHlInfoClient(),
): Promise<OrderEnrichmentWire> {
  try {
    if (action.action !== ORDER_TAG) return {};
    const symbol = bodyMarketSymbol(action);
    const assetIndex = assetIndexFromPayload(payload);
    if (symbol === null) return {};

    const master = await resolveHlMaster(payload);
    if (!master) return {};

    // coin needs the (cached) meta universe; resolve it before the per-coin
    // activeAssetData fetch. clearinghouseState (per-user) + maxLeverage (cached
    // meta) need no coin, so they fire concurrently from the start.
    const [coin, maxLeverage, chState] = await Promise.all([
      assetIndex !== null ? client.coinForIndex(assetIndex) : Promise.resolve(null),
      assetIndex !== null
        ? client.maxLeverageForIndex(assetIndex)
        : Promise.resolve(null),
      client.clearinghouseStateFor(master),
    ]);

    const assetData = coin ? await client.activeAssetDataFor(master, coin) : null;
    const markPx = assetData?.markPx ?? null;

    // ── per-market enrichment (keyed by body symbol) ──
    const size = orderSize(action);
    const notionalUsd =
      size !== null && markPx !== null ? roundOrUndef(size * markPx) : undefined;

    const position =
      coin && chState ? chState.positions.get(coin) ?? null : null;
    const hasOpenPosition = chState
      ? coin
        ? chState.positions.has(coin)
        : undefined
      : undefined;
    const positionRoeBps =
      position && position.returnOnEquity !== null
        ? roundOrUndef(position.returnOnEquity * 10_000)
        : undefined;
    const liquidationDistanceBps =
      position && position.liquidationPx !== null && markPx !== null && markPx > 0
        ? roundOrUndef(
            (Math.abs(markPx - position.liquidationPx) / markPx) * 10_000,
          )
        : undefined;

    const market: MarketEnrichmentWire = compact({
      max_leverage: maxLeverage ?? undefined,
      leverage_type: assetData?.leverageType ?? undefined,
      notional_usd: notionalUsd,
      position_roe_bps: positionRoeBps,
      liquidation_distance_bps: liquidationDistanceBps,
      has_open_position: hasOpenPosition,
    });

    // ── account-wide enrichment ──
    // The margin-utilization ratio's denominator is the SPOT-AWARE total
    // collateral (committed + usable), NOT perp-only `marginSummary.accountValue`.
    // HL backs new perp orders with spot USDC (`activeAssetData.availableToTrade`,
    // proven live to be USD collateral, leverage NOT applied), so an isolated
    // account whose small perp equity is fully committed is NOT out of margin —
    // perp-only would read 100% and over-warn (the 0x676f…9a54 false positive).
    // Absent `availableToTrade` → ratio OMITTED (margin-health policy stays
    // dormant) rather than falling back to the perp-only FP.
    const accountValue = chState?.accountValue ?? null;
    const totalMarginUsed = chState?.totalMarginUsed ?? null;
    const availableCollateral = assetData?.availableToTrade ?? null;
    const totalCollateral =
      totalMarginUsed !== null && availableCollateral !== null
        ? totalMarginUsed + availableCollateral
        : null;
    const account: AccountEnrichmentWire = compact({
      account_value_usd: roundOrUndef(accountValue),
      margin_used_ratio_bps:
        totalMarginUsed !== null && totalCollateral !== null && totalCollateral > 0
          ? roundOrUndef((totalMarginUsed / totalCollateral) * 10_000)
          : undefined,
    });

    const out: OrderEnrichmentWire = {};
    if (Object.keys(market).length > 0) out.markets = { [symbol]: market };
    if (Object.keys(account).length > 0) out.account = account;
    return out;
  } catch (err) {
    console.warn("[Dambi] HL order-enrichment collection threw (omitted)", {
      err: err instanceof Error ? err.message : String(err),
    });
    return {};
  }
}
