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
 * Action tags whose context carries the order-leverage field. A TWAP opens the
 * SAME leveraged perp exposure as a regular order, so it is enriched too — else
 * an order-leverage cap on HlOrder is trivially evaded by routing the exposure
 * through a TWAP (a first-class HL UI order type).
 */
const ORDER_TAGS = new Set(["hl_order", "hl_twap_order"]);
/** The leverage-change tag that triggers a cache invalidation. */
const UPDATE_LEVERAGE_TAG = "hl_update_leverage";

function asAssetIndex(value: unknown): number | null {
  return typeof value === "number" && Number.isInteger(value) && value >= 0
    ? value
    : null;
}

/**
 * Resolve `{ "<asset_index>": leverage }` for an order-class action
 * (`hl_order` / `hl_twap_order`), or `{}` for any other action / unresolved
 * input. `action` is the built `ActionBody` (`{ action, asset_index, ... }`).
 */
export async function collectHlLeverage(
  action: Record<string, unknown>,
  payload: VenueOrderPayload,
  client: HlInfoClient = defaultHlInfoClient(),
): Promise<Record<string, number>> {
  try {
    if (typeof action.action !== "string" || !ORDER_TAGS.has(action.action)) {
      return {};
    }
    const assetIndex = asAssetIndex(action.asset_index);
    if (assetIndex === null) return {};

    const master = await resolveHlMaster(payload);
    if (!master) {
      console.info(
        "[Scopeball] HL order-leverage: no master account resolved " +
          "(vaultAddress / wallet_id / stored all empty) → leverage omitted, policy dormant",
        { assetIndex },
      );
      return {};
    }

    const coin = await client.coinForIndex(assetIndex);
    if (!coin) {
      console.info(
        "[Scopeball] HL order-leverage: coin unresolved (spot index or meta miss) → leverage omitted",
        { assetIndex, master },
      );
      return {};
    }

    const leverage = await client.leverageFor(master, coin);
    if (leverage === null) {
      console.info(
        "[Scopeball] HL order-leverage: activeAssetData returned no leverage → omitted",
        { master, coin },
      );
      return {};
    }

    console.info("[Scopeball] HL order-leverage resolved", {
      master,
      coin,
      assetIndex,
      leverage,
    });
    return { [String(assetIndex)]: leverage };
  } catch (err) {
    // Never let leverage collection break (or deny-close) the verdict path.
    console.warn("[Scopeball] HL order-leverage collection threw (omitted)", {
      err: err instanceof Error ? err.message : String(err),
    });
    return {};
  }
}

/**
 * When the SW intercepts an `hl_update_leverage`, INVALIDATE the cached leverage
 * for (master, coin) so the NEXT order on that asset re-fetches the
 * authoritative `activeAssetData`.
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
    const assetIndex = asAssetIndex(action.asset_index);
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
