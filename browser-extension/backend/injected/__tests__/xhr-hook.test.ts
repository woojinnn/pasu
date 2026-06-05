/**
 * Unit test for the XHR branch of the MAIN-world venue hook.
 *
 * `@metamask/post-message-stream` (imported by `fetch-hook.ts`) throws at module
 * load under happy-dom (no `MessageEvent.prototype.source`), so we cannot import
 * `install()` directly. We therefore reproduce ONLY the thin prototype-patch
 * plumbing (`prototype.open` / `prototype.send`) here — but the actual
 * deny-closed body decision runs the REAL shared `decideVenueBody` from
 * `hl-exchange-parse.ts` (the same function `fetch-hook.ts` calls), so the
 * HL-1-critical body handling (string vs Blob/ArrayBuffer vs unreadable) is
 * tested against production code, not a divergent copy. Pins:
 *   - allow                         → native send IS called,
 *   - deny (policy)                 → native send NOT called + an `error` event,
 *   - deny (unreadable body, HL-1)  → native send NOT called (no bypass),
 *   - non-string order (Blob/ArrayBuffer) → coerced + evaluated, NOT bypassed,
 *   - non-venue POST                → straight to native send, no verdict.
 */
import { describe, it, expect, vi } from "vitest";
import type { VenueOrderPayload } from "@lib/types";
import {
  matchVenue,
  parseHyperliquidExchangeOrders,
  decideVenueBody,
  type VenueBodyDecision,
} from "../hl-exchange-parse";

const XHR_META = Symbol.for("__scopeball_xhr_meta__");

/**
 * Minimal XHR-like class with `open`/`send`/`dispatchEvent` on the PROTOTYPE
 * and a tiny event registry. `nativeSend` is spied via a prototype method so
 * the hook's `proto.send` patch shadows it (as with the real prototype).
 */
function makeFakeXHRClass(nativeSend: (body?: unknown) => void) {
  class FakeXHR {
    private listeners: Record<string, ((e: Event) => void)[]> = {};
    open(_method: string, _url: string | URL): void {
      /* captured by the hook's patched open */
    }
    send(body?: unknown): void {
      nativeSend(body);
    }
    addEventListener(type: string, l: (e: Event) => void): void {
      (this.listeners[type] ??= []).push(l);
    }
    dispatchEvent(e: Event): boolean {
      (this.listeners[e.type] ?? []).forEach((l) => l(e));
      return true;
    }
  }
  return FakeXHR as unknown as typeof XMLHttpRequest;
}

// Reproduce ONLY the prototype-patch plumbing; the body decision delegates to
// the REAL shared `decideVenueBody`. `evaluate` is the per-leg verdict callback
// (what `fetch-hook.ts` wires to the SW stream). Mirrors fetch-hook.ts:
//   - native-send unless this is a venue POST with a (non-null) body,
//   - otherwise defer the real send behind `decideVenueBody`,
//   - fail-CLOSED on a thrown fault (D6).
function installXhrHook(
  XHRClass: typeof XMLHttpRequest,
  evaluate: (payloads: VenueOrderPayload[]) => Promise<boolean>,
) {
  const proto = XHRClass.prototype;
  const originalOpen = proto.open;
  const originalSend = proto.send;

  proto.open = function (this: XMLHttpRequest, method: string, url: string | URL) {
    const u = typeof url === "string" ? url : url.toString();
    (this as unknown as Record<PropertyKey, unknown>)[XHR_META] = {
      method: String(method).toUpperCase(),
      url: u,
      venue: matchVenue(u),
    };
    return (originalOpen as (...a: unknown[]) => unknown).apply(this, [method, url]);
  } as typeof proto.open;

  proto.send = function (this: XMLHttpRequest, body?: Document | BodyInit | null) {
    const meta = (this as unknown as Record<PropertyKey, unknown>)[XHR_META] as
      | { method: string; url: string; venue?: string }
      | undefined;
    // No `typeof body !== "string"` short-circuit (HL-1): non-string venue
    // bodies must be coerced + evaluated, not native-sent unevaluated.
    if (!meta || !meta.venue || meta.method !== "POST" || body == null) {
      return originalSend.call(
        this,
        body as Parameters<XMLHttpRequest["send"]>[0],
      );
    }
    const xhr = this;
    const { url, venue } = meta;
    void (async () => {
      let decision: VenueBodyDecision;
      try {
        decision = await decideVenueBody(
          venue,
          url,
          "app.hyperliquid.xyz",
          body,
          evaluate,
        );
      } catch {
        decision = { kind: "deny", reason: "policy", payloads: null };
      }
      if (decision.kind !== "deny") {
        originalSend.call(xhr, body as Parameters<XMLHttpRequest["send"]>[0]);
        return;
      }
      xhr.dispatchEvent(new Event("error"));
      xhr.dispatchEvent(new Event("loadend"));
    })();
    return undefined;
  } as typeof proto.send;

  return () => {
    proto.open = originalOpen;
    proto.send = originalSend;
  };
}

const ORDER_BODY = JSON.stringify({
  action: {
    type: "order",
    orders: [{ a: 0, b: false, p: "60000", s: "0.1", r: false, t: { limit: { tif: "Gtc" } } }],
    grouping: "na",
  },
  nonce: 1,
});

describe("XHR venue-order hook mechanics", () => {
  it("DENY: native send is NOT called and an error event fires", async () => {
    const nativeSend = vi.fn();
    const FakeXHR = makeFakeXHRClass(nativeSend);
    const restore = installXhrHook(FakeXHR, async () => false);

    const x = new FakeXHR();
    let errored = false;
    x.addEventListener("error", () => (errored = true));
    x.open("POST", "https://api.hyperliquid.xyz/exchange");
    x.send(ORDER_BODY);
    await new Promise((r) => setTimeout(r, 0));

    expect(nativeSend).not.toHaveBeenCalled();
    expect(errored).toBe(true);
    restore();
  });

  it("ALLOW: native send IS called with the body", async () => {
    const nativeSend = vi.fn();
    const FakeXHR = makeFakeXHRClass(nativeSend);
    const restore = installXhrHook(FakeXHR, async () => true);

    const x = new FakeXHR();
    x.open("POST", "https://api.hyperliquid.xyz/exchange");
    x.send(ORDER_BODY);
    await new Promise((r) => setTimeout(r, 0));

    expect(nativeSend).toHaveBeenCalledWith(ORDER_BODY);
    restore();
  });

  it("PASS-THROUGH: non-venue POST goes straight to native send (no verdict)", async () => {
    const nativeSend = vi.fn();
    const evaluate = vi.fn(async () => false);
    const FakeXHR = makeFakeXHRClass(nativeSend);
    const restore = installXhrHook(FakeXHR, evaluate);

    const x = new FakeXHR();
    x.open("POST", "https://example.com/api");
    x.send("hello");
    await new Promise((r) => setTimeout(r, 0));

    expect(nativeSend).toHaveBeenCalledWith("hello");
    expect(evaluate).not.toHaveBeenCalled();
    restore();
  });

  it("parser still recognizes the order body (sanity)", () => {
    const out = parseHyperliquidExchangeOrders(
      "hyperliquid",
      "https://api.hyperliquid.xyz/exchange",
      "app.hyperliquid.xyz",
      ORDER_BODY,
    );
    expect(out).toHaveLength(1);
    expect(out![0].hlAction).toMatchObject({ kind: "order", order: { b: false } });
  });

  // ── HL-1 regression: non-string / unreadable venue bodies ────────────────
  // Before the fix, the XHR guard short-circuited any non-string body straight
  // to the native send (bypassing all policy). These pin that a non-string
  // order is now coerced + evaluated, and an un-inspectable body fails CLOSED.

  it("HL-1: a Blob order body is coerced + evaluated (not bypassed); deny blocks", async () => {
    const nativeSend = vi.fn();
    const evaluate = vi.fn(async () => false);
    const FakeXHR = makeFakeXHRClass(nativeSend);
    const restore = installXhrHook(FakeXHR, evaluate);

    const x = new FakeXHR();
    let errored = false;
    x.addEventListener("error", () => (errored = true));
    x.open("POST", "https://api.hyperliquid.xyz/exchange");
    x.send(new Blob([ORDER_BODY]));
    await new Promise((r) => setTimeout(r, 0));

    // The non-string body was decoded → evaluate ran → deny → blocked.
    expect(evaluate).toHaveBeenCalledOnce();
    expect(nativeSend).not.toHaveBeenCalled();
    expect(errored).toBe(true);
    restore();
  });

  it("HL-1: an ArrayBuffer order body is coerced + evaluated; allow sends", async () => {
    const nativeSend = vi.fn();
    const evaluate = vi.fn(async () => true);
    const FakeXHR = makeFakeXHRClass(nativeSend);
    const restore = installXhrHook(FakeXHR, evaluate);

    const buf = new TextEncoder().encode(ORDER_BODY).buffer;
    const x = new FakeXHR();
    x.open("POST", "https://api.hyperliquid.xyz/exchange");
    x.send(buf);
    await new Promise((r) => setTimeout(r, 0));

    expect(evaluate).toHaveBeenCalledOnce();
    expect(nativeSend).toHaveBeenCalledWith(buf);
    restore();
  });

  it("HL-1: an UNREADABLE venue body (FormData) is denied without bypass", async () => {
    const nativeSend = vi.fn();
    const evaluate = vi.fn(async () => true); // would ALLOW if it were ever asked
    const FakeXHR = makeFakeXHRClass(nativeSend);
    const restore = installXhrHook(FakeXHR, evaluate);

    const x = new FakeXHR();
    let errored = false;
    x.addEventListener("error", () => (errored = true));
    x.open("POST", "https://api.hyperliquid.xyz/exchange");
    x.send(new FormData());
    await new Promise((r) => setTimeout(r, 0));

    // Un-inspectable body → deny-closed BEFORE any verdict; never native-sent.
    expect(evaluate).not.toHaveBeenCalled();
    expect(nativeSend).not.toHaveBeenCalled();
    expect(errored).toBe(true);
    restore();
  });
});
