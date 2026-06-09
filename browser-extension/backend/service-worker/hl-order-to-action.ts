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

import type { VenueActionWire, VenueOrderPayload } from "@lib/types";

/** The off-chain venue chain id used for Hyperliquid in the v2 model. */
export const HL_CHAIN_ID = "hl-mainnet";

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

/** Build the `ActionBody::HyperliquidCore` JSON for one parsed CORE action. */
function actionBody(
  a: VenueActionWire,
  symbol: string | undefined,
): Record<string, unknown> {
  switch (a.kind) {
    case "order": {
      const o = a.order;
      const body: Record<string, unknown> = {
        domain: "hyperliquid_core",
        action: "hl_order",
        asset_index: o.a,
        is_buy: o.b,
        price: String(o.p),
        size: String(o.s),
        reduce_only: o.r ?? false,
        tif: tifFromWire(o.t),
      };
      if (symbol !== undefined) body.symbol = symbol;
      return body;
    }
    case "update_leverage": {
      const body: Record<string, unknown> = {
        domain: "hyperliquid_core",
        action: "hl_update_leverage",
        asset_index: a.assetIndex,
        is_cross: a.isCross,
        leverage: a.leverage,
      };
      if (symbol !== undefined) body.symbol = symbol;
      return body;
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
      const body: Record<string, unknown> = {
        domain: "hyperliquid_core",
        action: "hl_twap_order",
        asset_index: a.assetIndex,
        is_buy: a.isBuy,
        size: String(a.size),
        reduce_only: a.reduceOnly,
        minutes: a.minutes,
        randomize: a.randomize,
      };
      if (symbol !== undefined) body.symbol = symbol;
      return body;
    }
    case "update_isolated_margin": {
      const body: Record<string, unknown> = {
        domain: "hyperliquid_core",
        action: "hl_update_isolated_margin",
        asset_index: a.assetIndex,
        is_buy: a.isBuy,
        ntli: String(a.ntli),
      };
      if (symbol !== undefined) body.symbol = symbol;
      return body;
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
