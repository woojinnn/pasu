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

/** The action tag the order-leverage field applies to (HlOrderContext only). */
const ORDER_TAG = "hl_order";
/** The leverage-change tag whose value we use to refresh the cache. */
const UPDATE_LEVERAGE_TAG = "hl_update_leverage";

function asAssetIndex(value: unknown): number | null {
  return typeof value === "number" && Number.isInteger(value) && value >= 0
    ? value
    : null;
}

/**
 * Resolve `{ "<asset_index>": leverage }` for an `hl_order`, or `{}` for any
 * other action / unresolved input. `action` is the built `ActionBody`
 * (`{ action: "hl_order", asset_index, ... }`).
 */
export async function collectHlLeverage(
  action: Record<string, unknown>,
  payload: VenueOrderPayload,
  client: HlInfoClient = defaultHlInfoClient(),
): Promise<Record<string, number>> {
  try {
    if (action.action !== ORDER_TAG) return {};
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
 * When the SW sees an `hl_update_leverage`, seed the leverage cache for
 * (master, coin) with the just-set value so the NEXT order on that asset sees
 * the fresh leverage even within the cache TTL (free invalidation — we already
 * have the new value from the intercepted POST). Fire-and-forget; never throws.
 */
export async function noteHlLeverageUpdate(
  action: Record<string, unknown>,
  payload: VenueOrderPayload,
  client: HlInfoClient = defaultHlInfoClient(),
): Promise<void> {
  try {
    if (action.action !== UPDATE_LEVERAGE_TAG) return;
    const assetIndex = asAssetIndex(action.asset_index);
    const leverage = action.leverage;
    if (assetIndex === null || typeof leverage !== "number") return;

    const master = await resolveHlMaster(payload);
    if (!master) return;
    const coin = await client.coinForIndex(assetIndex);
    if (!coin) return;

    client.set(master, coin, Math.trunc(leverage));
  } catch {
    // Best-effort cache refresh — a failure just means the next order re-fetches.
  }
}
