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
  parseHyperliquidExchangeOrders,
} from "./hl-exchange-parse";

const FETCH_INSTALL_STATE = Symbol.for(
  "__scopeball_fetch_hook_install_state__",
);
/** Per-XHR-instance metadata captured at `open()` for use in `send()`. */
const XHR_META = Symbol.for("__scopeball_xhr_meta__");

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
      ww.__scopeball_intercepts__ =
        ((ww.__scopeball_intercepts__ as number) ?? 0) + 1;
      ww.__scopeball_last_verdict__ = { url, venue, allowed, at: Date.now() };
    } catch {
      /* ignore */
    }
  }

  function parseVenuePayloads(
    url: string,
    venue: string,
    body: unknown,
  ): VenueOrderPayload[] | null {
    const payloads = parseHyperliquidExchangeOrders(
      venue,
      url,
      location.hostname,
      body,
    ) as VenueOrderPayload[] | null;
    if (payloads) {
      // Devtools: the in-page parsed result (one entry per guarded leg), visible
      // in the PAGE console on the venue site + queryable from a probe via
      // `window.__scopeball_last_parse__`. (The fully-normalized ActionBody is
      // logged SW-side; this is the wire-level parse the page actually produced.)
      const actions = payloads.map((p) => ({ ...p.hlAction }));
      // eslint-disable-next-line no-console
      console.info("[Scopeball] HL /exchange parsed (in-page):", { url, venue, actions });
      try {
        const ww = window as unknown as Record<string, unknown>;
        ww.__scopeball_last_parse__ = { url, venue, actions, at: Date.now() };
      } catch {
        /* ignore */
      }
    }
    return payloads;
  }

  // Evaluate every order in a POST body; return false if ANY is denied
  // (deny-closed for batches).
  async function evaluatePayloads(
    payloads: VenueOrderPayload[],
  ): Promise<boolean> {
    if (!payloads) return true; // not an order action → out of scope, allow
    for (const payload of payloads) {
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
          const body =
            init?.body != null
              ? init.body
              : input instanceof Request
                ? await input.clone().text()
                : null;
          if (body != null) {
            const payloads = parseVenuePayloads(url, venue, body);
            if (payloads) {
              const allowed = await evaluatePayloads(payloads);
              recordVerdict(url, venue, allowed);
              if (!allowed) {
                throw new Error("Scopeball: venue order blocked by policy");
              }
              allowedPayloads = payloads;
            }
          }
        }
      } catch (err) {
        if (err instanceof Error && err.message.startsWith("Scopeball:")) {
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
            "[Scopeball] fetch-hook fault on a venue request — blocking (fail-closed)",
            err,
          );
          throw new Error("Scopeball: venue order blocked (fail-closed)");
        }
        console.warn("[Scopeball] fetch-hook non-fatal error", err);
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

      // Not a venue-order POST → behave exactly like native send.
      if (
        !meta ||
        !meta.venue ||
        meta.method !== "POST" ||
        body == null ||
        typeof body !== "string"
      ) {
        return originalSend.call(
          this,
          body as Parameters<XMLHttpRequest["send"]>[0],
        );
      }

      // Venue-order POST: defer the real send until the async verdict resolves.
      const xhr = this;
      const { url, venue } = meta;
      void (async () => {
        let allowed = true;
        let payloads: VenueOrderPayload[] | null = null;
        try {
          payloads = parseVenuePayloads(url, venue, body);
          allowed = payloads ? await evaluatePayloads(payloads) : true;
        } catch (err) {
          // Fail-CLOSED (D6): a fault while evaluating a venue order (bridge
          // down, parse throw, SW unreachable, …) must BLOCK, not waive the
          // order through. This is a venue-order POST (guarded above), so a
          // failed verdict path defaults to deny — matching the fetch path and
          // the SW lifecycle's deny-closed contract.
          console.error(
            "[Scopeball] xhr-hook fault — blocking (fail-closed)",
            err,
          );
          allowed = false;
        }
        recordVerdict(url, venue, allowed);
        if (allowed) {
          if (payloads) {
            const reportedPayloads = payloads;
            let reported = false;
            const reportOnce = () => {
              if (reported) return;
              reported = true;
              reportXhrResponse(xhr, reportedPayloads);
            };
            xhr.addEventListener("loadend", reportOnce);
            xhr.addEventListener("error", reportOnce);
          }
          originalSend.call(xhr, body);
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
  ).__scopeball_fetch_hook_loaded__ = true;
} catch {
  /* no window (SW/node) — ignore */
}

try {
  install();
} catch (err) {
  try {
    (
      window as unknown as Record<PropertyKey, unknown>
    ).__scopeball_fetch_hook_error__ =
      err instanceof Error ? err.message : String(err);
  } catch {
    /* ignore */
  }
  console.error("[Scopeball] fetch-hook install failed", err);
}
