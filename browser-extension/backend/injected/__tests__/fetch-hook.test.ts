import { describe, it, expect } from "vitest";

import { matchVenue, parseHyperliquidExchangeOrders } from "../hl-exchange-parse";
import { RequestType } from "@lib/types";

const ENDPOINT = "https://api.hyperliquid.xyz/exchange";
const HOST = "app.hyperliquid.xyz";

function parse(body: unknown) {
  return parseHyperliquidExchangeOrders("hyperliquid", ENDPOINT, HOST, body);
}

describe("parseHyperliquidExchangeOrders", () => {
  it("extracts a single order from a stringified /exchange body", () => {
    const body = JSON.stringify({
      action: {
        type: "order",
        orders: [
          { a: 0, b: false, p: "60000", s: "0.1", r: false, t: { limit: { tif: "Gtc" } } },
        ],
        grouping: "na",
      },
      nonce: 1_738_000_000_000,
      signature: { r: "0x", s: "0x", v: 28 },
    });

    const out = parse(body);
    expect(out).not.toBeNull();
    expect(out).toHaveLength(1);
    expect(out![0]).toMatchObject({
      type: RequestType.VENUE_ORDER,
      venue: "hyperliquid",
      endpoint: ENDPOINT,
      hostname: HOST,
      hlAction: { kind: "order", order: { a: 0, b: false, p: "60000", s: "0.1", r: false } },
    });
  });

  it("extracts every leg of a multi-order (TP/SL) batch", () => {
    const out = parse({
      action: {
        type: "order",
        orders: [
          { a: 0, b: false, p: "60000", s: "0.1", r: false, t: { limit: { tif: "Gtc" } } },
          { a: 0, b: true, p: "55000", s: "0.1", r: true, t: { trigger: { isMarket: true } } },
        ],
        grouping: "normalTpsl",
      },
    });
    expect(out).toHaveLength(2);
    expect(out![1].hlAction).toMatchObject({ kind: "order", order: { r: true } });
  });

  it("parses the v1 fund-movement / leverage action subset (D4)", () => {
    expect(
      parse({ action: { type: "updateLeverage", asset: 0, isCross: true, leverage: 10 } })![0]
        .hlAction,
    ).toEqual({ kind: "update_leverage", assetIndex: 0, isCross: true, leverage: 10 });
    expect(
      parse({ action: { type: "withdraw3", destination: "0xabc", amount: "5" } })![0].hlAction,
    ).toEqual({ kind: "withdraw", destination: "0xabc", amount: "5" });
    expect(
      parse({ action: { type: "usdSend", destination: "0xdef", amount: "9" } })![0].hlAction,
    ).toEqual({ kind: "usd_send", destination: "0xdef", amount: "9" });
    // approveAgent is no longer a modeled action — it falls through to the
    // hl_unknown catch-all (deny-closed), like any other unmodeled /exchange type.
    expect(
      parse({ action: { type: "approveAgent", agentAddress: "0x123", agentName: "bot" } })![0]
        .hlAction,
    ).toEqual({ kind: "unknown", actionType: "approveAgent" });
  });

  it("returns null for benign/out-of-scope actions and unknown for malformed guarded actions", () => {
    expect(parse({ action: { type: "cancel", cancels: [] } })).toBeNull();
    expect(parse({ action: { type: "batchModify", modifies: [] } })).toBeNull();
    // updateLeverage missing required isCross: guarded action, not pass-through.
    expect(parse({ action: { type: "updateLeverage", asset: 0, leverage: 10 } })![0].hlAction)
      .toEqual({ kind: "unknown", actionType: "updateLeverage" });
    expect(parse({ type: "meta" })).toBeNull();
    expect(parse("not json")).toBeNull();
    expect(parse(undefined)).toBeNull();
    expect(parse({})).toBeNull();
  });

  it("keeps valid order legs and adds unknown for malformed guarded legs", () => {
    const out = parse({
      action: {
        type: "order",
        orders: [
          { notAnOrder: true },
          { a: 3, b: true, p: "1", s: "1" },
        ],
      },
    });
    expect(out).toHaveLength(2);
    expect(out![0].hlAction).toMatchObject({ kind: "order", order: { a: 3 } });
    expect(out![1].hlAction).toEqual({ kind: "unknown", actionType: "order" });
  });

  it("routes all-malformed order legs to unknown rather than null", () => {
    const out = parse({ action: { type: "order", orders: [{ x: 1 }, { y: 2 }] } });
    expect(out).toHaveLength(1);
    expect(out![0].hlAction).toEqual({ kind: "unknown", actionType: "order" });
  });
});

describe("matchVenue host coverage", () => {
  it("matches the live `api-ui` gateway the web app actually uses", () => {
    // Regression: the production app POSTs to api-ui.hyperliquid.xyz, NOT the
    // bare api.hyperliquid.xyz documented for SDKs. Missing `-ui` let every
    // real order slip past the hook.
    expect(matchVenue("https://api-ui.hyperliquid.xyz/exchange")).toBe("hyperliquid");
  });

  it("matches the bare api host and testnet variants", () => {
    expect(matchVenue("https://api.hyperliquid.xyz/exchange")).toBe("hyperliquid");
    expect(matchVenue("https://api-ui.hyperliquid-testnet.xyz/exchange")).toBe("hyperliquid");
    expect(matchVenue("https://api.hyperliquid-testnet.xyz/exchange")).toBe("hyperliquid");
  });

  it("does NOT match info endpoints or unrelated hosts", () => {
    expect(matchVenue("https://api-ui.hyperliquid.xyz/info")).toBeUndefined();
    expect(matchVenue("https://evil.xyz/exchange")).toBeUndefined();
    expect(matchVenue("https://notapi.hyperliquid.xyz.evil.com/exchange")).toBeUndefined();
  });
});
