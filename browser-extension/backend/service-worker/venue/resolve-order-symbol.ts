/**
 * HL order-time SYMBOL resolution — patch the human asset symbol into the built
 * perp body's `market.symbol` before evaluation.
 *
 * The HL order wire carries only the numeric asset index (`order.a` /
 * `assetIndex`); it has no coin name. `hlOrderToAction` therefore builds the body
 * with an `ASSET-<index>` placeholder symbol. Resolving the real name needs the
 * HL `meta` universe (asset_index → name), which is an async fetch — disallowed
 * in the injected MAIN-world parse but fine here in the SW. This resolves the
 * name (e.g. 0 → "BTC") via the (cached) universe and OVERWRITES the placeholder,
 * so a policy that matches on `context.market.symbol` (e.g. an order-symbol
 * allowlist) sees "BTC" rather than "ASSET-0".
 *
 * Consistency: the enrichment collectors (`collect-hl-leverage` /
 * `collect-order-enrichment`) key their per-market maps by this same
 * `market.symbol`, so the patch MUST land before they run — the orchestrator
 * awaits this resolution immediately before firing them. After the patch both
 * the lowering and the enrichment agree on one symbol.
 *
 * BEST-EFFORT, in place, NEVER throws (mirrors `collectHlLeverage`): a non-market
 * action, a spot index, a meta miss or a fetch error leaves the `ASSET-<index>`
 * placeholder intact — the body stays internally consistent (the collectors key
 * by it), the order is still evaluated, and only symbol-specific policies are
 * affected. A miss must not flip a venue order to deny-closed.
 */
import type { VenueOrderPayload } from "@lib/types";
import {
  assetIndexFromPayload,
  bodyMarketSymbol,
} from "./collect-hl-leverage";
import { defaultHlInfoClient, type HlInfoClient } from "./hl-info-client";

/**
 * Resolve the asset symbol for a market-bearing perp action and patch it into
 * `action.market.symbol` in place. Returns the resolved coin (e.g. "BTC"), or
 * `null` when nothing was resolved (the placeholder is then left untouched).
 *
 * `action` is the built perp body (`place_order` / `change_leverage` /
 * `adjust_margin`, each carrying `market.symbol`); the numeric asset index is
 * read from `payload.hlAction` (the signed truth for which asset the order
 * targets, so its `coinForIndex` name is authoritative).
 */
export async function resolveOrderSymbol(
  action: Record<string, unknown>,
  payload: VenueOrderPayload,
  client: HlInfoClient = defaultHlInfoClient(),
): Promise<string | null> {
  try {
    // Only market-bearing bodies carry a `market.symbol` placeholder to resolve.
    if (bodyMarketSymbol(action) === null) return null;
    const assetIndex = assetIndexFromPayload(payload);
    if (assetIndex === null) return null;

    const coin = await client.coinForIndex(assetIndex);
    if (!coin) {
      // Spot index / meta miss / fetch error → keep the `ASSET-<index>`
      // placeholder. The collectors key by it, so the body stays internally
      // consistent; only symbol-specific policies stay dormant.
      console.info(
        "[Dambi] HL order symbol: index unresolved (spot index or meta miss) → placeholder kept",
        { assetIndex },
      );
      return null;
    }

    // Patch in place so (a) the lowering reads the real symbol and (b) the
    // enrichment collectors (which run next and key by `market.symbol`) key by
    // the SAME resolved symbol. The asset index is the signed truth, so its
    // resolved name always matches what the order targets.
    (action.market as { symbol?: unknown }).symbol = coin;
    console.info("[Dambi] HL order symbol resolved", { assetIndex, coin });
    return coin;
  } catch (err) {
    // Never let symbol resolution break (or deny-close) the verdict path.
    console.warn("[Dambi] HL order symbol resolution threw (placeholder kept)", {
      err: err instanceof Error ? err.message : String(err),
    });
    return null;
  }
}
