/**
 * Pure parser: a Hyperliquid `/exchange` POST body → one
 * {@link VenueOrderPayload} per guarded CORE action.
 *
 * Kept in its own module (no DOM / `@metamask/post-message-stream` imports) so
 * it is trivially unit-testable and so importing it never triggers the
 * MAIN-world `fetch` install side effect in `fetch-hook.ts`.
 *
 * Guards the full high-risk CORE surface: orders (`order` / `twapOrder`),
 * leverage / margin (`updateLeverage` / `updateIsolatedMargin`), every
 * fund-movement (`withdraw3` / `usdSend` / `spotSend` / `sendAsset` /
 * `sendToEvmWithData` / `usdClassTransfer` / `vaultTransfer` /
 * `subAccountTransfer` / `cDeposit` / `cWithdraw`) and permission / delegation
 * (`approveAgent` / `approveBuilderFee` / `tokenDelegate`).
 *
 * Anything else routes through the `hl_unknown` catch-all UNLESS it is in
 * {@link BENIGN_PASS_THROUGH} (high-frequency, fund-/permission-neutral actions
 * like `cancel` / `modify` / `scheduleCancel`), which returns `null` and passes
 * through unevaluated. So a fund- or permission-moving action we have not
 * explicitly modeled can never silently pass the venue.
 */
import {
  RequestType,
  type HyperliquidOrderWire,
  type VenueActionWire,
  type VenueOrderPayload,
} from "@lib/types";

/**
 * Venue endpoints we police. Each entry maps a URL test → venue id.
 *
 * The live web app actually POSTs to `api-ui.hyperliquid.xyz/exchange` (the
 * `-ui` gateway), not the bare `api.hyperliquid.xyz` documented for SDKs — so
 * the host pattern matches an optional `-ui` (and `-testnet`) sub-label. Both
 * mainnet hosts (`api`, `api-ui`) and their testnet variants are covered.
 */
export const VENUE_MATCHERS: { test: (url: string) => boolean; venue: string }[] =
  [
    {
      test: (url) =>
        /(^|\/\/)api(-ui)?\.hyperliquid(-testnet)?\.xyz\/exchange\b/.test(url),
      venue: "hyperliquid",
    },
  ];

export function matchVenue(url: string): string | undefined {
  return VENUE_MATCHERS.find((m) => m.test(url))?.venue;
}

/**
 * `/exchange` action types that move no funds and grant no permission, and that
 * the live app POSTs at high frequency (order lifecycle / admin). These pass
 * through unevaluated (`null`) rather than routing to the `hl_unknown` catch-all
 * — gating them would add SW + WASM round-trips per cancel/modify with no
 * security value. Everything NOT here and NOT explicitly modeled becomes
 * `hl_unknown`, so a novel fund / permission action is never silently allowed.
 */
const BENIGN_PASS_THROUGH: ReadonlySet<string> = new Set([
  "cancel",
  "cancelByCloid",
  "modify",
  "batchModify",
  "twapCancel",
  "scheduleCancel",
  "noop",
  "reserveRequestWeight",
  "setReferrer",
  "createSubAccount",
  "subAccountModify",
  "vaultModify",
  "spotUser",
  "evmUserModify",
]);

/** Coerce an unknown to a plain object, or `undefined`. */
function asObject(v: unknown): Record<string, unknown> | undefined {
  return v && typeof v === "object" && !Array.isArray(v)
    ? (v as Record<string, unknown>)
    : undefined;
}

/** Request-level attribution shared by every leg of one `/exchange` POST. */
interface HlAttribution {
  /** `nonce` — a millisecond wall-clock timestamp. */
  nonce?: number;
  /** `vaultAddress` when the order is placed on behalf of a vault. */
  vaultAddress?: string;
}

/** Wrap one parsed CORE action in a `VenueOrderPayload` envelope. */
function envelope(
  venue: string,
  endpoint: string,
  hostname: string,
  hlAction: VenueActionWire,
  attribution: HlAttribution,
): VenueOrderPayload {
  const p: VenueOrderPayload = {
    type: RequestType.VENUE_ORDER,
    chainId: 0,
    hostname,
    venue,
    endpoint,
    hlAction,
    // `symbol` is resolved SW-side from the venue meta cache (the wire only has
    // the numeric index); omitted here.
  };
  if (attribution.nonce !== undefined) p.nonce = attribution.nonce;
  if (attribution.vaultAddress !== undefined) p.vaultAddress = attribution.vaultAddress;
  return p;
}

/** Parse the `orders[]` of a `{"type":"order"}` action — one payload per leg. */
function parseOrders(
  venue: string,
  endpoint: string,
  hostname: string,
  action: Record<string, unknown>,
  attribution: HlAttribution,
): VenueOrderPayload[] | null {
  const orders = action.orders;
  if (!Array.isArray(orders) || orders.length === 0) return null;

  const payloads: VenueOrderPayload[] = [];
  for (const o of orders) {
    const order = asObject(o);
    // An order-wire entry must at least carry the numeric asset index `a` and
    // the boolean side `b`; anything else is not an order leg.
    if (!order || typeof order.a !== "number" || typeof order.b !== "boolean") {
      continue;
    }
    const wire: HyperliquidOrderWire = {
      a: order.a,
      b: order.b,
      p: String(order.p ?? ""),
      s: String(order.s ?? ""),
      r: typeof order.r === "boolean" ? order.r : false,
      t: order.t,
    };
    if (typeof order.c === "string") wire.c = order.c;
    payloads.push(
      envelope(venue, endpoint, hostname, { kind: "order", order: wire }, attribution),
    );
  }
  return payloads.length > 0 ? payloads : null;
}

/**
 * Parse a Hyperliquid `/exchange` POST body into one {@link VenueOrderPayload}
 * per guarded CORE action. Returns `null` when the body is not one of the
 * guarded action types — those are out of scope and pass through untouched.
 *
 * (Name kept for import stability; it now parses the full v1 action subset, not
 * just orders.)
 */
export function parseHyperliquidExchangeOrders(
  venue: string,
  endpoint: string,
  hostname: string,
  rawBody: unknown,
): VenueOrderPayload[] | null {
  let body: unknown = rawBody;
  if (typeof rawBody === "string") {
    try {
      body = JSON.parse(rawBody);
    } catch {
      return null;
    }
  }
  const root = asObject(body);
  if (!root) return null;

  const action = asObject(root.action);
  if (!action || typeof action.type !== "string") return null;

  // Request-level attribution shared by every leg: the `nonce` (a ms wall-clock
  // timestamp → `submitted_at`) and `vaultAddress` (on-behalf-of, when present).
  const attribution: HlAttribution = {};
  if (typeof root.nonce === "number") attribution.nonce = root.nonce;
  if (typeof root.vaultAddress === "string" && root.vaultAddress.length > 0) {
    attribution.vaultAddress = root.vaultAddress;
  }

  const one = (a: VenueActionWire): VenueOrderPayload[] => [
    envelope(venue, endpoint, hostname, a, attribution),
  ];

  switch (action.type) {
    case "order":
      return parseOrders(venue, endpoint, hostname, action, attribution);

    case "updateLeverage": {
      if (
        typeof action.asset !== "number" ||
        typeof action.isCross !== "boolean" ||
        typeof action.leverage !== "number"
      ) {
        return null;
      }
      return one({
        kind: "update_leverage",
        assetIndex: action.asset,
        isCross: action.isCross,
        leverage: action.leverage,
      });
    }

    case "withdraw3": {
      if (typeof action.destination !== "string" || action.amount === undefined) {
        return null;
      }
      return one({
        kind: "withdraw",
        destination: action.destination,
        amount: String(action.amount),
      });
    }

    case "usdSend": {
      if (typeof action.destination !== "string" || action.amount === undefined) {
        return null;
      }
      return one({
        kind: "usd_send",
        destination: action.destination,
        amount: String(action.amount),
      });
    }

    case "approveAgent": {
      if (typeof action.agentAddress !== "string") return null;
      const a: VenueActionWire = {
        kind: "approve_agent",
        agentAddress: action.agentAddress,
      };
      if (typeof action.agentName === "string") a.agentName = action.agentName;
      return one(a);
    }

    case "spotSend": {
      if (
        typeof action.destination !== "string" ||
        typeof action.token !== "string" ||
        action.amount === undefined
      ) {
        return null;
      }
      return one({
        kind: "spot_send",
        destination: action.destination,
        token: action.token,
        amount: String(action.amount),
      });
    }

    case "usdClassTransfer": {
      if (action.amount === undefined) return null;
      return one({
        kind: "usd_class_transfer",
        amount: String(action.amount),
        toPerp: action.toPerp === true,
      });
    }

    case "sendAsset": {
      if (typeof action.destination !== "string" || action.amount === undefined) {
        return null;
      }
      return one({
        kind: "send_asset",
        destination: action.destination,
        sourceDex: typeof action.sourceDex === "string" ? action.sourceDex : "",
        destinationDex:
          typeof action.destinationDex === "string" ? action.destinationDex : "",
        token: typeof action.token === "string" ? action.token : "",
        amount: String(action.amount),
      });
    }

    case "sendToEvmWithData": {
      if (
        typeof action.destinationRecipient !== "string" ||
        action.amount === undefined
      ) {
        return null;
      }
      return one({
        kind: "send_to_evm_with_data",
        token: typeof action.token === "string" ? action.token : "",
        amount: String(action.amount),
        sourceDex: typeof action.sourceDex === "string" ? action.sourceDex : "",
        destinationRecipient: action.destinationRecipient,
        data: typeof action.data === "string" ? action.data : "",
      });
    }

    case "cDeposit": {
      if (action.wei === undefined) return null;
      return one({ kind: "c_deposit", wei: String(action.wei) });
    }

    case "cWithdraw": {
      if (action.wei === undefined) return null;
      return one({ kind: "c_withdraw", wei: String(action.wei) });
    }

    case "vaultTransfer": {
      if (typeof action.vaultAddress !== "string" || action.usd === undefined) {
        return null;
      }
      return one({
        kind: "vault_transfer",
        vaultAddress: action.vaultAddress,
        isDeposit: action.isDeposit === true,
        usd: String(action.usd),
      });
    }

    case "subAccountTransfer": {
      if (typeof action.subAccountUser !== "string" || action.usd === undefined) {
        return null;
      }
      return one({
        kind: "sub_account_transfer",
        subAccountUser: action.subAccountUser,
        isDeposit: action.isDeposit === true,
        usd: String(action.usd),
      });
    }

    case "approveBuilderFee": {
      if (typeof action.builder !== "string" || action.maxFeeRate === undefined) {
        return null;
      }
      return one({
        kind: "approve_builder_fee",
        maxFeeRate: String(action.maxFeeRate),
        builder: action.builder,
      });
    }

    case "tokenDelegate": {
      if (typeof action.validator !== "string" || action.wei === undefined) {
        return null;
      }
      return one({
        kind: "token_delegate",
        validator: action.validator,
        isUndelegate: action.isUndelegate === true,
        wei: String(action.wei),
      });
    }

    case "twapOrder": {
      const twap = asObject(action.twap);
      if (!twap || typeof twap.a !== "number" || typeof twap.b !== "boolean") {
        return null;
      }
      return one({
        kind: "twap_order",
        assetIndex: twap.a,
        isBuy: twap.b,
        size: String(twap.s ?? ""),
        reduceOnly: twap.r === true,
        minutes: typeof twap.m === "number" ? twap.m : Number(twap.m ?? 0),
        randomize: twap.t === true,
      });
    }

    case "updateIsolatedMargin": {
      if (typeof action.asset !== "number" || action.ntli === undefined) {
        return null;
      }
      return one({
        kind: "update_isolated_margin",
        assetIndex: action.asset,
        isBuy: action.isBuy === true,
        ntli: String(action.ntli),
      });
    }

    default:
      // BENIGN_PASS_THROUGH (cancel / modify / schedule / admin) is high-frequency
      // and moves no funds / grants no permission → return null (out of scope, not
      // evaluated). EVERY OTHER unrecognized type falls to the `hl_unknown`
      // catch-all so a fund / permission action we have not modeled can never pass
      // the venue unevaluated (closes the silent-allow gap).
      if (BENIGN_PASS_THROUGH.has(action.type)) return null;
      return one({ kind: "unknown", actionType: action.type });
  }
}
