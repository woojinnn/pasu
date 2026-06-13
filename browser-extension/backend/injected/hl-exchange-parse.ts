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
 * Order-placing variants `modify` / `batchModify` are decoded as order legs (they
 * re-place a full order spec and can open exposure). Anything else routes through
 * the `hl_unknown` catch-all UNLESS it is in {@link BENIGN_PASS_THROUGH}
 * (high-frequency, fund-/permission-neutral actions like `cancel` /
 * `scheduleCancel`), which returns `null` and passes through unevaluated. So a
 * fund- or permission-moving action we have not explicitly modeled can never
 * silently pass the venue.
 */
import {
  RequestType,
  type HyperliquidOrderWire,
  type VenueActionWire,
  type VenueOrderPayload,
} from "@lib/types";

/**
 * Exact hosts whose `/exchange` POST we police. The live web app POSTs to the
 * `api-ui` gateway, not the bare `api.hyperliquid.xyz` documented for SDKs, so
 * both mainnet hosts (`api`, `api-ui`) and their testnet variants are covered.
 *
 * H1: matching is on the PARSED + NORMALIZED URL (host + exact path), not a
 * substring regex. DNS is case-insensitive and tolerates a trailing-dot FQDN
 * root and an explicit `:443` port, all of which reach the same server — a
 * case-sensitive substring regex let `API.HYPERLIQUID.XYZ/exchange` (and a
 * relative `/exchange`) slip past the hook entirely (a venue-gating bypass).
 */
const HL_EXCHANGE_HOSTS: ReadonlySet<string> = new Set([
  "api.hyperliquid.xyz",
  "api-ui.hyperliquid.xyz",
  "api.hyperliquid-testnet.xyz",
  "api-ui.hyperliquid-testnet.xyz",
]);

/** Normalize a URL hostname for allowlist comparison: `URL` already lowercases
 *  it, so we only strip a single trailing dot (the FQDN root `host.` form). */
function normalizeHost(hostname: string): string {
  return hostname.endsWith(".") ? hostname.slice(0, -1) : hostname;
}

/**
 * Parse `raw` into an absolute `URL`, resolving a relative URL against `base`
 * (callers pass `location.href` so `fetch("/exchange")` resolves to the page
 * origin). Returns `null` for an unparseable input rather than throwing.
 */
export function normalizeVenueUrl(raw: string, base?: string): URL | null {
  try {
    return new URL(raw, base);
  } catch {
    return null;
  }
}

/**
 * Venue endpoints we police. Each entry tests the NORMALIZED `URL`
 * (host allowlist + exact `/exchange` path) → venue id.
 */
export const VENUE_MATCHERS: { test: (url: URL) => boolean; venue: string }[] = [
  {
    test: (url) =>
      HL_EXCHANGE_HOSTS.has(normalizeHost(url.hostname)) &&
      url.pathname === "/exchange",
    venue: "hyperliquid",
  },
];

/**
 * Resolve `url` (absolute or, with `base`, relative) and return the matched
 * venue id, or `undefined` if it is not a policed venue endpoint / unparseable.
 */
export function matchVenue(url: string, base?: string): string | undefined {
  const parsed = normalizeVenueUrl(url, base);
  if (!parsed) return undefined;
  return VENUE_MATCHERS.find((m) => m.test(parsed))?.venue;
}

/**
 * `/exchange` action types that move no funds, grant no permission, and place no
 * order, and that the live app POSTs at high frequency (cancel / admin). These
 * pass through unevaluated (`null`) rather than routing to the `hl_unknown`
 * catch-all — gating them would add SW + WASM round-trips per cancel with no
 * security value. Order-placing `modify` / `batchModify` are deliberately NOT
 * here (they are decoded as order legs). Everything NOT here and NOT explicitly
 * modeled becomes `hl_unknown`, so a novel fund / permission / order action is
 * never silently allowed.
 */
const BENIGN_PASS_THROUGH: ReadonlySet<string> = new Set([
  "cancel",
  "cancelByCloid",
  // NOTE: `modify` / `batchModify` are NOT benign — they re-place a full order
  // spec (can open/grow exposure) and are decoded as order legs (see
  // `parseModify`), so they are deliberately absent from this set.
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

function unknownPayload(
  venue: string,
  endpoint: string,
  hostname: string,
  actionType: string,
  attribution: HlAttribution,
): VenueOrderPayload {
  return envelope(
    venue,
    endpoint,
    hostname,
    { kind: "unknown", actionType },
    attribution,
  );
}

/**
 * Build a {@link HyperliquidOrderWire} from a raw order object, or `null` when it
 * is not a valid order leg. An order-wire entry must at least carry the numeric
 * asset index `a` and the boolean side `b`. Shared by `order`, `modify` and
 * `batchModify` (all three carry the same order spec).
 */
function orderWireFrom(o: unknown): HyperliquidOrderWire | null {
  const order = asObject(o);
  if (!order || typeof order.a !== "number" || typeof order.b !== "boolean") {
    return null;
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
  return wire;
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
  if (!Array.isArray(orders) || orders.length === 0) {
    return [unknownPayload(venue, endpoint, hostname, "order", attribution)];
  }

  const payloads: VenueOrderPayload[] = [];
  let sawInvalidLeg = false;
  for (const o of orders) {
    const wire = orderWireFrom(o);
    if (!wire) {
      sawInvalidLeg = true;
      continue;
    }
    payloads.push(
      envelope(venue, endpoint, hostname, { kind: "order", order: wire }, attribution),
    );
  }
  if (sawInvalidLeg) {
    payloads.push(unknownPayload(venue, endpoint, hostname, "order", attribution));
  }
  return payloads.length > 0
    ? payloads
    : [unknownPayload(venue, endpoint, hostname, "order", attribution)];
}

/**
 * Parse a `{"type":"modify"}` / `{"type":"batchModify"}` action. Both REPLACE a
 * resting order with a full new order spec, so they can OPEN or grow exposure —
 * they are NOT benign no-ops. Every carried `order` is decoded exactly like a
 * freshly-placed order leg, so a no-new-short / reduce-only-lockdown policy sees
 * the resulting order.
 *
 * If NO valid order is carried (empty `modifies`, or an order spec HL would
 * itself reject as malformed), the request places no exposure, so it stays
 * benign (`null`, pass-through) — the bypass it closes is an OPENING ORDER,
 * which is only present when a leg decodes.
 */
function parseModify(
  venue: string,
  endpoint: string,
  hostname: string,
  action: Record<string, unknown>,
  attribution: HlAttribution,
): VenueOrderPayload[] | null {
  const legs: unknown[] = Array.isArray(action.modifies)
    ? (action.modifies as unknown[]).map((m) => asObject(m)?.order)
    : [action.order];

  const payloads: VenueOrderPayload[] = [];
  for (const o of legs) {
    const wire = orderWireFrom(o);
    if (wire) {
      payloads.push(
        envelope(venue, endpoint, hostname, { kind: "order", order: wire }, attribution),
      );
    }
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
  const actionType = action.type;

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
  const unknown = () => [
    unknownPayload(venue, endpoint, hostname, actionType, attribution),
  ];

  switch (actionType) {
    case "order":
      return parseOrders(venue, endpoint, hostname, action, attribution);

    case "modify":
    case "batchModify":
      // Both re-place a full order spec → decode as order leg(s), never benign.
      return parseModify(venue, endpoint, hostname, action, attribution);

    case "updateLeverage": {
      if (
        typeof action.asset !== "number" ||
        typeof action.isCross !== "boolean" ||
        typeof action.leverage !== "number"
      ) {
        return unknown();
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
        return unknown();
      }
      return one({
        kind: "withdraw",
        destination: action.destination,
        amount: String(action.amount),
      });
    }

    case "usdSend": {
      if (typeof action.destination !== "string" || action.amount === undefined) {
        return unknown();
      }
      return one({
        kind: "usd_send",
        destination: action.destination,
        amount: String(action.amount),
      });
    }

    case "spotSend": {
      if (
        typeof action.destination !== "string" ||
        typeof action.token !== "string" ||
        action.amount === undefined
      ) {
        return unknown();
      }
      return one({
        kind: "spot_send",
        destination: action.destination,
        token: action.token,
        amount: String(action.amount),
      });
    }

    case "usdClassTransfer": {
      if (action.amount === undefined) return unknown();
      return one({
        kind: "usd_class_transfer",
        amount: String(action.amount),
        toPerp: action.toPerp === true,
      });
    }

    case "sendAsset": {
      if (typeof action.destination !== "string" || action.amount === undefined) {
        return unknown();
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
        return unknown();
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
      if (action.wei === undefined) return unknown();
      return one({ kind: "c_deposit", wei: String(action.wei) });
    }

    case "cWithdraw": {
      if (action.wei === undefined) return unknown();
      return one({ kind: "c_withdraw", wei: String(action.wei) });
    }

    case "vaultTransfer": {
      if (typeof action.vaultAddress !== "string" || action.usd === undefined) {
        return unknown();
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
        return unknown();
      }
      return one({
        kind: "sub_account_transfer",
        subAccountUser: action.subAccountUser,
        isDeposit: action.isDeposit === true,
        usd: String(action.usd),
      });
    }

    case "tokenDelegate": {
      if (typeof action.validator !== "string" || action.wei === undefined) {
        return unknown();
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
        return unknown();
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
        return unknown();
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
      if (BENIGN_PASS_THROUGH.has(actionType)) return null;
      return unknown();
  }
}

/**
 * Read a fetch/XHR `BodyInit` to the UTF-8 string {@link parseHyperliquidExchangeOrders}
 * needs. Handles the encodings a venue POST can realistically carry (string /
 * Blob / ArrayBuffer / typed-array view / `URLSearchParams`). Returns `null`
 * when the body cannot be read to text here (FormData / ReadableStream /
 * Document / unknown) so the caller can fail CLOSED (HL-1): a non-string body
 * must never slip past the deny-closed guard by reaching the parser as a
 * non-string (which yields `null` = "not an order" = pass-through) or, for XHR,
 * by short-circuiting straight to the native `send()`. Lives here (the pure,
 * stream-free module) so it is unit-testable and shared by both hook branches.
 */
export async function coerceBodyToString(body: unknown): Promise<string | null> {
  if (body == null) return null;
  if (typeof body === "string") return body;
  try {
    if (typeof Blob !== "undefined" && body instanceof Blob) return await body.text();
    if (body instanceof ArrayBuffer) {
      return new TextDecoder().decode(new Uint8Array(body));
    }
    if (ArrayBuffer.isView(body)) {
      return new TextDecoder().decode(body as ArrayBufferView);
    }
    if (typeof URLSearchParams !== "undefined" && body instanceof URLSearchParams) {
      return body.toString();
    }
  } catch {
    return null;
  }
  return null; // FormData / ReadableStream / Document / unknown — not text here
}

/** Verdict for a whole venue `/exchange` POST body (deny-closed). */
export type VenueBodyDecision =
  | { kind: "passthrough" }
  | { kind: "allow"; payloads: VenueOrderPayload[] }
  | {
      kind: "deny";
      reason: "unreadable_body" | "policy";
      payloads: VenueOrderPayload[] | null;
    };

/**
 * Decide a venue `/exchange` POST from its raw body + an `evaluate` callback
 * (which asks the SW for a per-leg verdict). The SINGLE deny-closed body-gate
 * shared by BOTH the fetch and XHR branches of the MAIN-world hook, so the
 * HL-1-critical body handling is one real, unit-testable code path — not a
 * per-branch copy that can drift:
 *   - body unreadable to text (FormData / stream / …) → deny (an un-inspectable
 *     venue order must never pass — HL-1),
 *   - body not a recognized order action → passthrough (info/cancel/etc.),
 *   - any leg denied → deny (deny-closed: one denied leg blocks the batch),
 *   - else allow.
 * Side effects (recordVerdict / throw / event dispatch / execution reports /
 * in-page logging) stay in the caller; this function is pure modulo `evaluate`.
 */
export async function decideVenueBody(
  venue: string,
  endpoint: string,
  hostname: string,
  rawBody: unknown,
  evaluate: (payloads: VenueOrderPayload[]) => Promise<boolean>,
): Promise<VenueBodyDecision> {
  const bodyStr = await coerceBodyToString(rawBody);
  if (bodyStr === null) {
    return { kind: "deny", reason: "unreadable_body", payloads: null };
  }
  const payloads = parseHyperliquidExchangeOrders(
    venue,
    endpoint,
    hostname,
    bodyStr,
  );
  if (!payloads) return { kind: "passthrough" };
  const allowed = await evaluate(payloads);
  return allowed
    ? { kind: "allow", payloads }
    : { kind: "deny", reason: "policy", payloads };
}
