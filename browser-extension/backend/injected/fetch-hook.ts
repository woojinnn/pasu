/**
 * MAIN-world network hook for off-chain venue orders (`fetch` + `XMLHttpRequest`).
 *
 * Hyperliquid (and other off-chain venues) never route an order through
 * `window.ethereum` — the dApp signs with an agent key and POSTs the order
 * straight to `https://api.hyperliquid.xyz/exchange`. The provider proxy
 * therefore never sees it. This hook wraps BOTH `window.fetch` and
 * `XMLHttpRequest` (axios/older clients use XHR) in the page's MAIN world,
 * intercepts that POST, and — for each order in the body — asks the service
 * worker (via a DEDICATED post-message channel, see `Identifier.FETCH_INPAGE`)
 * for a policy verdict BEFORE the request is allowed to proceed. A deny makes
 * the wrapped call fail, so the order never reaches the venue.
 *
 * Mirrors `proxy-injected-providers.ts`:
 *   - a single install guard (Symbol on `window`) so SPA re-injection / bfcache
 *     restore cannot double-wrap (double-eval each request);
 *   - its own `WindowPostMessageStream` channel (NOT the provider proxy's) so
 *     the two handshakes never race and cork each other;
 *   - `await` the verdict and block on deny — the only thing that actually stops
 *     the request.
 *
 * The pure body→order parser lives in `hl-exchange-parse.ts` (no DOM / stream
 * imports) so it is unit-testable without triggering this install side effect.
 *
 * Deny-closed: any order whose verdict is `false` blocks the WHOLE POST (a
 * batch can carry TP/SL siblings; we never let a batch through because only one
 * leg was allowed). A body that is not a recognizable order action passes
 * through untouched — non-order traffic (info/cancel/etc.) is out of scope.
 *
 * ## fetch vs XHR blocking
 * `fetch` is promise-based, so the Proxy `apply` trap simply `await`s the
 * verdict before calling the original. `XHR.send()` returns synchronously, so
 * we cannot await inline; instead we DEFER the real `send()` — capture its
 * args, run the async verdict, then either invoke the original `send()` (allow)
 * or synthesize an error event + abort (deny). Either way the dApp's request
 * does not hit the network until the policy clears it.
 */

import { WindowPostMessageStream } from "@metamask/post-message-stream";
import { Identifier } from "@lib/identifier";
import {
  sendToStreamAndAwaitResponse,
  sendToStreamAndDisregard,
} from "@lib/messages";
import type { MessageData, VenueOrderPayload } from "@lib/types";
import { buildHyperliquidExecutionReport } from "./hl-execution-report";
import {
  matchVenue,
  decideVenueBody,
  type VenueBodyDecision,
} from "./hl-exchange-parse";

const FETCH_INSTALL_STATE = Symbol.for(
  "__pasu_fetch_hook_install_state__",
);
/** Per-XHR-instance metadata captured at `open()` for use in `send()`. */
const XHR_META = Symbol.for("__pasu_xhr_meta__");

type WritableStream = WindowPostMessageStream & { write(data: unknown): void };

interface XhrMeta {
  method: string;
  url: string;
  venue: string | undefined;
}

function install(): void {
  // Skip in any non-page context (the SW / node / unit tests have no real
  // `window.fetch` to wrap).
  if (typeof window === "undefined" || typeof window.fetch !== "function") {
    return;
  }
  const w = window as unknown as Record<PropertyKey, unknown>;
  if (w[FETCH_INSTALL_STATE]) return; // already installed in this realm
  const stream = new WindowPostMessageStream({
    name: Identifier.FETCH_INPAGE,
    target: Identifier.FETCH_CONTENT_SCRIPT,
  }) as WritableStream;
  w[FETCH_INSTALL_STATE] = { stream };

  function recordVerdict(url: string, venue: string, allowed: boolean): void {
    // Diagnostic beacon (MAIN-world visible): record what the hook decided so a
    // probe can prove a block came from policy, not a coincidental network
    // error. Harmless in production.
    try {
      const ww = window as unknown as Record<string, unknown>;
      ww.__pasu_intercepts__ =
        ((ww.__pasu_intercepts__ as number) ?? 0) + 1;
      ww.__pasu_last_verdict__ = { url, venue, allowed, at: Date.now() };
    } catch {
      /* ignore */
    }
  }

  function logInPageParse(
    url: string,
    venue: string,
    payloads: VenueOrderPayload[],
  ): void {
    // Devtools: the in-page parsed result (one entry per guarded leg), visible
    // in the PAGE console on the venue site + queryable from a probe via
    // `window.__pasu_last_parse__`. (The fully-normalized ActionBody is
    // logged SW-side; this is the wire-level parse the page actually produced.)
    const actions = payloads.map((p) => ({ ...p.hlAction }));
    // eslint-disable-next-line no-console
    console.info("[Pasu] HL /exchange parsed (in-page):", { url, venue, actions });
    try {
      const ww = window as unknown as Record<string, unknown>;
      ww.__pasu_last_parse__ = { url, venue, actions, at: Date.now() };
    } catch {
      /* ignore */
    }
  }

  // ── connected master-account capture (order-time leverage enrichment) ──────
  // The /exchange body carries no master account (the order is agent-signed),
  // but the dApp connected a normal EVM wallet via `window.ethereum`. We read
  // that account (non-prompting `eth_accounts`, which the provider proxy passes
  // through ungated) and stamp it onto each venue payload as `wallet_id`, so the
  // SW can key the `activeAssetData` leverage lookup to the right account.
  // Best-effort + cached: a read failure / no wallet just leaves `wallet_id`
  // unset — leverage enrichment then stays dormant (never blocks the order).
  interface InpageEthProvider {
    request?: (args: { method: string }) => Promise<unknown>;
    on?: (event: string, cb: (...args: unknown[]) => void) => void;
  }
  const ethProvider = (): InpageEthProvider | undefined =>
    (window as unknown as { ethereum?: InpageEthProvider }).ethereum;

  let connectedAccount: string | null = null;
  const setAccountFrom = (accts: unknown): void => {
    connectedAccount =
      Array.isArray(accts) && typeof accts[0] === "string"
        ? accts[0].toLowerCase()
        : null;
  };
  async function ensureConnectedAccount(): Promise<void> {
    if (connectedAccount) return;
    try {
      const eth = ethProvider();
      if (!eth?.request) return;
      setAccountFrom(await eth.request({ method: "eth_accounts" }));
    } catch {
      /* no wallet / read failed → leverage enrichment stays dormant */
    }
  }
  // Keep the cache fresh when the user connects / switches accounts.
  try {
    ethProvider()?.on?.("accountsChanged", (...args: unknown[]) =>
      setAccountFrom(args[0]),
    );
  } catch {
    /* provider has no event emitter → rely on lazy reads */
  }

  // Evaluate every order in a POST body; return false if ANY is denied
  // (deny-closed for batches).
  async function evaluatePayloads(
    payloads: VenueOrderPayload[],
  ): Promise<boolean> {
    if (!payloads) return true; // not an order action → out of scope, allow
    // Resolve the connected master once (cached); used to key the leverage
    // lookup. Best-effort — never blocks or fails the verdict.
    await ensureConnectedAccount();
    for (const payload of payloads) {
      if (connectedAccount && !payload.wallet_id) {
        payload.wallet_id = { address: connectedAccount, chains: [] };
      }
      const ok = await sendToStreamAndAwaitResponse(
        stream,
        payload as MessageData,
      );
      if (!ok) return false; // deny-closed: one denied leg blocks the batch
    }
    return true;
  }

  function emitExecutionReports(
    payloads: VenueOrderPayload[],
    observation: {
      httpStatus: number;
      responseJson?: unknown;
      responseText?: string;
    },
  ): void {
    for (const [statusIndex, payload] of payloads.entries()) {
      sendToStreamAndDisregard(
        stream,
        buildHyperliquidExecutionReport(payload, {
          ...observation,
          statusIndex,
        }) as MessageData,
      );
    }
  }

  async function reportFetchResponse(
    response: Response,
    payloads: VenueOrderPayload[],
  ): Promise<void> {
    let responseJson: unknown;
    let responseText: string | undefined;
    try {
      responseText = await response.text();
      if (responseText) responseJson = JSON.parse(responseText);
    } catch {
      // Reporting must never affect the dApp response path.
    }
    const observation: {
      httpStatus: number;
      responseJson?: unknown;
      responseText?: string;
    } = {
      httpStatus: response.status,
    };
    if (responseJson !== undefined) observation.responseJson = responseJson;
    if (responseText !== undefined) observation.responseText = responseText;
    emitExecutionReports(payloads, observation);
  }

  function reportXhrResponse(
    xhr: XMLHttpRequest,
    payloads: VenueOrderPayload[],
  ): void {
    let responseText: string | undefined;
    let responseJson: unknown;
    try {
      responseText =
        typeof xhr.responseText === "string" ? xhr.responseText : undefined;
      if (responseText) responseJson = JSON.parse(responseText);
    } catch {
      // Reporting must never affect XHR event delivery.
    }
    const observation: {
      httpStatus: number;
      responseJson?: unknown;
      responseText?: string;
    } = {
      httpStatus: xhr.status || 0,
    };
    if (responseJson !== undefined) observation.responseJson = responseJson;
    if (responseText !== undefined) observation.responseText = responseText;
    emitExecutionReports(payloads, observation);
  }

  // ── fetch ─────────────────────────────────────────────────────────────────
  const originalFetch = window.fetch;
  window.fetch = new Proxy(originalFetch, {
    apply: async (target, thisArg, args: Parameters<typeof fetch>) => {
      let allowedPayloads: VenueOrderPayload[] | null = null;
      try {
        const [input, init] = args;
        const url =
          typeof input === "string"
            ? input
            : input instanceof URL
              ? input.toString()
              : (input as Request)?.url ?? "";
        const method = (
          init?.method ??
          (typeof input === "object" && "method" in (input as Request)
            ? (input as Request).method
            : "GET")
        ).toUpperCase();
        const venue = matchVenue(url);
        if (venue && method === "POST") {
          // The body can ride on `init.body` OR on a `Request` first arg
          // (`fetch(new Request(url, { body }))`). Read both so a Request-shaped
          // call cannot smuggle an order past the guard.
          const rawBody =
            init?.body != null
              ? init.body
              : input instanceof Request
                ? await input.clone().text()
                : null;
          if (rawBody != null) {
            // HL-1: the SHARED deny-closed gate coerces ANY BodyInit to text (a
            // non-string body must not slip past), parses, and evaluates — one
            // real code path for both fetch + XHR.
            const decision = await decideVenueBody(
              venue,
              url,
              location.hostname,
              rawBody,
              evaluatePayloads,
            );
            if (decision.kind === "deny") {
              recordVerdict(url, venue, false);
              if (decision.payloads) logInPageParse(url, venue, decision.payloads);
              throw new Error(
                decision.reason === "unreadable_body"
                  ? "Pasu: venue order blocked (unreadable body)"
                  : "Pasu: venue order blocked by policy",
              );
            }
            if (decision.kind === "allow") {
              recordVerdict(url, venue, true);
              logInPageParse(url, venue, decision.payloads);
              allowedPayloads = decision.payloads;
            }
            // passthrough → not a recognized order; proceed untouched.
          }
        }
      } catch (err) {
        if (err instanceof Error && err.message.startsWith("Pasu:")) {
          throw err; // the block — must propagate
        }
        // Fail-CLOSED (D6): a venue-order POST whose evaluation FAULTED (bridge
        // down, parse throw, SW unreachable, …) must NOT slip through. Only a
        // *venue* request reaches here as a match; block it. Non-venue traffic
        // does not match and proceeds untouched.
        const [input] = args;
        const url =
          typeof input === "string"
            ? input
            : input instanceof URL
              ? input.toString()
              : (input as Request)?.url ?? "";
        if (matchVenue(url)) {
          console.error(
            "[Pasu] fetch-hook fault on a venue request — blocking (fail-closed)",
            err,
          );
          throw new Error("Pasu: venue order blocked (fail-closed)");
        }
        console.warn("[Pasu] fetch-hook non-fatal error", err);
      }
      try {
        const response = (await Reflect.apply(
          target,
          thisArg,
          args,
        )) as Response;
        if (allowedPayloads) {
          void reportFetchResponse(response.clone(), allowedPayloads);
        }
        return response;
      } catch (err) {
        if (allowedPayloads) {
          emitExecutionReports(allowedPayloads, {
            httpStatus: 0,
            responseText: err instanceof Error ? err.message : String(err),
          });
        }
        throw err;
      }
    },
  });

  // ── XMLHttpRequest ──────────────────────────────────────────────────────────
  const XHR = window.XMLHttpRequest;
  if (typeof XHR === "function" && XHR.prototype) {
    const proto = XHR.prototype;
    const originalOpen = proto.open;
    const originalSend = proto.send;

    // Capture (method, url) at open() so send() can match the venue.
    proto.open = function (
      this: XMLHttpRequest,
      method: string,
      url: string | URL,
    ) {
      try {
        const u = typeof url === "string" ? url : url.toString();
        const meta: XhrMeta = {
          method: String(method).toUpperCase(),
          url: u,
          venue: matchVenue(u),
        };
        (this as unknown as Record<PropertyKey, unknown>)[XHR_META] = meta;
      } catch {
        /* ignore — fall through to native open */
      }
      // eslint-disable-next-line prefer-rest-params
      return (originalOpen as (...a: unknown[]) => unknown).apply(
        this,
        arguments as unknown as unknown[],
      );
    } as typeof proto.open;

    proto.send = function (
      this: XMLHttpRequest,
      body?: Document | BodyInit | null,
    ) {
      const meta = (this as unknown as Record<PropertyKey, unknown>)[
        XHR_META
      ] as XhrMeta | undefined;

      // Not a venue-order POST → behave exactly like native send. A non-string
      // body is NO LONGER short-circuited here (HL-1): a Blob / ArrayBuffer
      // order would otherwise reach the native send unevaluated. Venue POSTs
      // with ANY body are deferred + coerced below.
      if (!meta || !meta.venue || meta.method !== "POST" || body == null) {
        return originalSend.call(
          this,
          body as Parameters<XMLHttpRequest["send"]>[0],
        );
      }

      // Venue-order POST: defer the real send until the async verdict resolves.
      const xhr = this;
      const { url, venue } = meta;
      void (async () => {
        let decision: VenueBodyDecision;
        try {
          // HL-1: the SAME shared deny-closed gate as the fetch path — coerce
          // ANY body to text (an unreadable venue body is un-inspectable →
          // deny, never reaching the native send), parse, evaluate.
          decision = await decideVenueBody(
            venue,
            url,
            location.hostname,
            body,
            evaluatePayloads,
          );
        } catch (err) {
          // Fail-CLOSED (D6): a fault while evaluating a venue order (bridge
          // down, parse throw, SW unreachable, …) must BLOCK, not waive the
          // order through — matches the fetch path + the SW lifecycle's
          // deny-closed contract.
          console.error(
            "[Pasu] xhr-hook fault — blocking (fail-closed)",
            err,
          );
          decision = { kind: "deny", reason: "policy", payloads: null };
        }
        const parsedPayloads =
          decision.kind === "passthrough" ? null : decision.payloads;
        if (parsedPayloads) logInPageParse(url, venue, parsedPayloads);
        const allowed = decision.kind !== "deny";
        recordVerdict(url, venue, allowed);
        if (allowed) {
          if (decision.kind === "allow") {
            const reportedPayloads = decision.payloads;
            let reported = false;
            const reportOnce = () => {
              if (reported) return;
              reported = true;
              reportXhrResponse(xhr, reportedPayloads);
            };
            xhr.addEventListener("loadend", reportOnce);
            xhr.addEventListener("error", reportOnce);
          }
          originalSend.call(
            xhr,
            body as Parameters<XMLHttpRequest["send"]>[0],
          );
          return;
        }
        // DENY: do not call the native send. Simulate a failed request so the
        // dApp's XHR error/loadend handlers fire (mirrors a network failure).
        try {
          xhr.dispatchEvent(new Event("error"));
          xhr.dispatchEvent(new Event("loadend"));
        } catch {
          /* ignore — at minimum the request never went out */
        }
      })();
      // Synchronous return like native send(); the real I/O happens (or not)
      // in the deferred task above.
      return undefined;
    } as typeof proto.send;
  }
}

try {
  // Beacon (before any guard) so a probe can confirm the MAIN-world script
  // actually executed in the page realm, independent of install success.
  (
    window as unknown as Record<PropertyKey, unknown>
  ).__pasu_fetch_hook_loaded__ = true;
} catch {
  /* no window (SW/node) — ignore */
}

try {
  install();
} catch (err) {
  try {
    (
      window as unknown as Record<PropertyKey, unknown>
    ).__pasu_fetch_hook_error__ =
      err instanceof Error ? err.message : String(err);
  } catch {
    /* ignore */
  }
  console.error("[Pasu] fetch-hook install failed", err);
}
