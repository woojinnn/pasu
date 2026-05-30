/**
 * Unit test for the XHR branch of the MAIN-world venue hook.
 *
 * happy-dom's `XMLHttpRequest` plus the `@metamask/post-message-stream` import
 * in `fetch-hook.ts` throws at module load under happy-dom (no
 * `MessageEvent.prototype.source`). So we DON'T import `fetch-hook.ts` here;
 * instead we reproduce its XHR install logic — patching `prototype.open` /
 * `prototype.send` — against a minimal fake whose native `open`/`send` live on
 * the PROTOTYPE (exactly like real `XMLHttpRequest`, so the prototype patch
 * actually intercepts). The pure parser is covered by `fetch-hook.test.ts`; the
 * orchestrator verdict path by `orchestrator.test.ts`. This pins the XHR
 * *blocking mechanics*:
 *   - allow  → native send IS called,
 *   - deny   → native send is NOT called + an `error` event fires,
 *   - non-venue POST → straight to native send, no verdict.
 */
import { describe, it, expect, vi } from "vitest";
import { matchVenue, parseHyperliquidExchangeOrders } from "../hl-exchange-parse";

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

// Re-implement the install against an injectable `evaluateBody` so we test the
// mechanics without the stream import. Byte-aligned with fetch-hook.ts.
function installXhrHook(
  XHRClass: typeof XMLHttpRequest,
  evaluateBody: (url: string, venue: string, body: unknown) => Promise<boolean>,
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
    const xhr = this;
    const { url, venue } = meta;
    void (async () => {
      let allowed = true;
      try {
        allowed = await evaluateBody(url, venue, body);
      } catch {
        allowed = true;
      }
      if (allowed) {
        originalSend.call(xhr, body);
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
});
