/**
 * Pure parser: a Hyperliquid `/exchange` POST body → one
 * {@link VenueOrderPayload} per guarded CORE action.
 *
 * Kept in its own module (no DOM / `@metamask/post-message-stream` imports) so
 * it is trivially unit-testable and so importing it never triggers the
 * MAIN-world `fetch` install side effect in `fetch-hook.ts`.
 *
 * v1 guards the high-risk subset — `order`, `updateLeverage`, and the three
 * fund-movement / delegation actions (`withdraw3`, `usdSend`, `approveAgent`).
 * Every other action type (cancel, batchModify, info, …) returns `null` and is
 * passed through untouched.
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

/** Coerce an unknown to a plain object, or `undefined`. */
function asObject(v: unknown): Record<string, unknown> | undefined {
  return v && typeof v === "object" && !Array.isArray(v)
    ? (v as Record<string, unknown>)
    : undefined;
}

/** Wrap one parsed CORE action in a `VenueOrderPayload` envelope. */
function envelope(
  venue: string,
  endpoint: string,
  hostname: string,
  hlAction: VenueActionWire,
): VenueOrderPayload {
  return {
    type: RequestType.VENUE_ORDER,
    chainId: 0,
    hostname,
    venue,
    endpoint,
    hlAction,
    // `symbol` is resolved SW-side from the venue meta cache (the wire only has
    // the numeric index); omitted here.
  };
}

/** Parse the `orders[]` of a `{"type":"order"}` action — one payload per leg. */
function parseOrders(
  venue: string,
  endpoint: string,
  hostname: string,
  action: Record<string, unknown>,
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
    payloads.push(envelope(venue, endpoint, hostname, { kind: "order", order: wire }));
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

  const one = (a: VenueActionWire): VenueOrderPayload[] => [
    envelope(venue, endpoint, hostname, a),
  ];

  switch (action.type) {
    case "order":
      return parseOrders(venue, endpoint, hostname, action);

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

    default:
      // Out of scope (cancel / batchModify / info / …) — pass through.
      return null;
  }
}
