/**
 * Hyperliquid `/exchange` CORE action → v2 `ActionBody` + `ActionMeta`.
 *
 * The fetch hook intercepts the `/exchange` POST and hands each parsed action to
 * the service worker as a {@link VenueOrderPayload}. This converter turns that
 * into the exact JSON the v2 policy entry point (`evaluate_action_v2_json`)
 * deserializes — an `ActionBody::HyperliquidCore(...)` plus an off-chain-sig
 * `ActionMeta`.
 *
 * The emitted shape is byte-pinned to the Rust serde output: `ActionBody` is
 * doubly internally-tagged, so each body is `{ domain: "hyperliquid_core",
 * action: "hl_*", ...fields }`. `hl-order-to-action.test.ts` asserts this
 * converter reproduces the canonical JSON, and
 * `crates/policy-engine-wasm/tests/hl_exchange_deny_e2e.rs` feeds the same shape
 * through the real WASM entry point — so a serde drift on either side fails a
 * test rather than silently mis-deserializing at runtime.
 *
 * No live data is fetched: prices / sizes / amounts pass through as decimal
 * strings verbatim (fractional-safe — the engine models them as `Decimal`, not
 * `U256`), and the asset symbol is left for the Rust lowering to resolve (it
 * falls back to `ASSET-<index>` when unresolved).
 */

import type {
  HyperliquidOrderWire,
  VenueActionWire,
  VenueOrderPayload,
} from "@lib/types";

/** The off-chain venue chain id used for Hyperliquid in the v2 model. */
export const HL_CHAIN_ID = "hl-mainnet";

/** The `PerpVenue::Hyperliquid` chain id used in the generic `Perp::` model. */
export const HL_PERP_CHAIN = "hyperliquid:mainnet";

/** `tx.to` sentinel — Hyperliquid CORE actions have no on-chain settlement address. */
export const HL_TO_SENTINEL = "0x0000000000000000000000000000000000000000";

/** Result of {@link hlOrderToAction}: the two JSON inputs the v2 path needs. */
export interface HlActionInput {
  action: Record<string, unknown>;
  meta: Record<string, unknown>;
}

/** Normalize a Hyperliquid order-type object's tif to the engine's spelling. */
function tifFromWire(t: unknown): string {
  const tif = (t as { limit?: { tif?: string } } | undefined)?.limit?.tif;
  switch (tif) {
    case "Ioc":
      return "ioc";
    case "Alo": // Add-Liquidity-Only == post-only
      return "post_only";
    default:
      return "gtc";
  }
}

/** The `PerpVenue::Hyperliquid` object for the generic perp body. */
function hlPerpVenue(): Record<string, unknown> {
  return { name: "hyperliquid", chain: HL_PERP_CHAIN };
}

/**
 * A `MarketRef` for an HL asset. `symbol` falls back to `ASSET-<index>` when the
 * venue meta cache has not yet resolved the numeric index (matching the old HL
 * lowering); the venue is the HL coin universe.
 */
function hlMarketRef(
  assetIndex: number,
  symbol: string | undefined,
): Record<string, unknown> {
  return {
    symbol: symbol ?? `ASSET-${assetIndex}`,
    venue: { name: "hyperliquid" },
  };
}

/**
 * Map an HL trigger order's `{ isMarket, tpsl }` to the engine `StopOrderKind`
 * spelling. `tpsl == "tp"` ⇒ take-profit, else stop-loss; `isMarket` chooses the
 * market vs limit lane.
 */
function stopKind(isMarket: boolean, tpsl: unknown): string {
  if (tpsl === "tp") return isMarket ? "take_profit" : "take_profit_limit";
  return isMarket ? "stop_market" : "stop_limit";
}

/**
 * Build the discriminated `orderType` record from one wire order spec. A
 * `t.trigger` spec is a stop / take-profit (carries `triggerPx`, `isMarket`,
 * `tpsl`); otherwise it is a limit order (carries the tif). The typed Rust
 * `OrderType` enforces the per-kind required fields on decode.
 */
function orderTypeFromWire(o: HyperliquidOrderWire): Record<string, unknown> {
  const t = o.t as
    | {
        limit?: { tif?: string };
        trigger?: { triggerPx?: unknown; isMarket?: boolean; tpsl?: unknown };
      }
    | undefined;
  const trigger = t?.trigger;
  if (trigger) {
    const isMarket = trigger.isMarket === true;
    const ot: Record<string, unknown> = {
      kind: "stop",
      trigger_price: String(trigger.triggerPx ?? ""),
      order_kind: stopKind(isMarket, trigger.tpsl),
    };
    // A stop_limit / take_profit_limit fills at the order's limit price `p`;
    // a market-triggered stop carries no limit price.
    if (!isMarket) ot.limit_price = String(o.p);
    return ot;
  }
  return {
    kind: "limit",
    price: String(o.p),
    time_in_force: { kind: tifFromWire(o.t) },
  };
}

/** Build the `ActionBody::HyperliquidCore` JSON for one parsed CORE action. */
function actionBody(
  a: VenueActionWire,
  symbol: string | undefined,
): Record<string, unknown> {
  switch (a.kind) {
    case "order": {
      const o = a.order;
      // HL orders decode to the generic `Perp::PlaceOrder` (orderType
      // limit/stop). Fractional size flows through the `base_decimal` SizeSpec.
      return {
        domain: "perp",
        action: "place_order",
        venue: hlPerpVenue(),
        market: hlMarketRef(o.a, symbol),
        side: o.b ? "long" : "short",
        size: { kind: "base_decimal", amount: String(o.s) },
        reduce_only: o.r ?? false,
        order_type: orderTypeFromWire(o),
      };
    }
    case "update_leverage": {
      // HL updateLeverage decodes to the generic `Perp::ChangeLeverage`;
      // `newLeverage` is a decimal string (the engine lowers it to a Cedar
      // `decimal`). `isCross` carries no policy signal and is dropped.
      return {
        domain: "perp",
        action: "change_leverage",
        venue: hlPerpVenue(),
        market: hlMarketRef(a.assetIndex, symbol),
        new_leverage: String(a.leverage),
      };
    }
    case "withdraw":
      return {
        domain: "hyperliquid_core",
        action: "hl_withdraw",
        destination: a.destination,
        amount: String(a.amount),
      };
    case "usd_send":
      return {
        domain: "hyperliquid_core",
        action: "hl_usd_send",
        destination: a.destination,
        amount: String(a.amount),
      };
    case "spot_send":
      return {
        domain: "hyperliquid_core",
        action: "hl_spot_send",
        destination: a.destination,
        token: a.token,
        amount: String(a.amount),
      };
    case "usd_class_transfer":
      return {
        domain: "hyperliquid_core",
        action: "hl_usd_class_transfer",
        amount: String(a.amount),
        to_perp: a.toPerp,
      };
    case "send_asset":
      return {
        domain: "hyperliquid_core",
        action: "hl_send_asset",
        destination: a.destination,
        source_dex: a.sourceDex,
        destination_dex: a.destinationDex,
        token: a.token,
        amount: String(a.amount),
      };
    case "send_to_evm_with_data":
      return {
        domain: "hyperliquid_core",
        action: "hl_send_to_evm_with_data",
        token: a.token,
        amount: String(a.amount),
        source_dex: a.sourceDex,
        destination_recipient: a.destinationRecipient,
        data: a.data,
      };
    case "c_deposit":
      return {
        domain: "hyperliquid_core",
        action: "hl_c_deposit",
        wei: String(a.wei),
      };
    case "c_withdraw":
      return {
        domain: "hyperliquid_core",
        action: "hl_c_withdraw",
        wei: String(a.wei),
      };
    case "vault_transfer":
      return {
        domain: "hyperliquid_core",
        action: "hl_vault_transfer",
        vault_address: a.vaultAddress,
        is_deposit: a.isDeposit,
        usd: String(a.usd),
      };
    case "sub_account_transfer":
      return {
        domain: "hyperliquid_core",
        action: "hl_sub_account_transfer",
        sub_account_user: a.subAccountUser,
        is_deposit: a.isDeposit,
        usd: String(a.usd),
      };
    case "token_delegate":
      return {
        domain: "hyperliquid_core",
        action: "hl_token_delegate",
        validator: a.validator,
        is_undelegate: a.isUndelegate,
        wei: String(a.wei),
      };
    case "twap_order": {
      // A TWAP is the same `Perp::PlaceOrder` action with orderType "twap".
      return {
        domain: "perp",
        action: "place_order",
        venue: hlPerpVenue(),
        market: hlMarketRef(a.assetIndex, symbol),
        side: a.isBuy ? "long" : "short",
        size: { kind: "base_decimal", amount: String(a.size) },
        reduce_only: a.reduceOnly,
        order_type: {
          kind: "twap",
          duration_minutes: a.minutes,
          randomize: a.randomize,
        },
      };
    }
    case "update_isolated_margin": {
      // HL updateIsolatedMargin decodes to the generic `Perp::AdjustMargin`,
      // referenced by `(market, side)` (no position id at sign time). `ntli` is
      // the signed margin delta (negative = remove).
      return {
        domain: "perp",
        action: "adjust_margin",
        venue: hlPerpVenue(),
        market: hlMarketRef(a.assetIndex, symbol),
        side: a.isBuy ? "long" : "short",
        delta: String(a.ntli),
      };
    }
    case "unknown":
      return {
        domain: "hyperliquid_core",
        action: "hl_unknown",
        action_type: a.actionType,
      };
  }
}

/**
 * Convert a {@link VenueOrderPayload} into the `{ action, meta }` JSON pair the
 * v2 entry point consumes. Pure and synchronous.
 */
/** Fallback `submitted_at` (unix seconds) when the request carries no nonce. */
const HL_SUBMITTED_AT_FALLBACK = 1_738_000_000;

export function hlOrderToAction(payload: VenueOrderPayload): HlActionInput {
  const action = actionBody(payload.hlAction, payload.symbol);

  // HL `nonce` is a millisecond wall-clock timestamp; `ActionMeta.submitted_at`
  // is unix seconds. Threading the real nonce lets time-scoped policies see the
  // actual submission time instead of a fixed placeholder.
  const submittedAt =
    typeof payload.nonce === "number" && payload.nonce > 0
      ? Math.floor(payload.nonce / 1000)
      : HL_SUBMITTED_AT_FALLBACK;

  // NOTE: `submitter` stays a sentinel. The /exchange body carries no master
  // account address (only an agent signature + nonce), and the SW does not track
  // the connected account for the HL path. Recovering the real submitter (e.g.
  // ec-recover on user-signed actions) is deferred; for a single-user pre-sign
  // analyzer the high-value scoping fields are destination / amount, which ARE
  // modeled. See memory `project_hl_order_audit` (#2b).
  const meta: Record<string, unknown> = {
    submitted_at: submittedAt,
    submitter: "0x000000000000000000000000000000000000a01c",
    nature: {
      kind: "offchain_sig",
      domain: { name: "Hyperliquid", version: "1" },
      deadline: submittedAt + 600,
    },
  };

  return { action, meta };
}
