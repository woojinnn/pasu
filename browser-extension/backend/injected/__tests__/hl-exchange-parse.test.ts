/**
 * Parser routing contract for `parseHyperliquidExchangeOrders` — the in-page
 * boundary that decides which `/exchange` actions reach the policy engine.
 *
 * The security-critical invariant (closes the silent-allow gap): an `/exchange`
 * action that is NOT one of the high-frequency, fund-/permission-neutral
 * `BENIGN_PASS_THROUGH` types and is NOT explicitly modeled must route to the
 * `hl_unknown` catch-all (a `VenueOrderPayload`), NEVER `null`. Only benign
 * types pass through as `null` (unevaluated).
 */
import { describe, it, expect } from "vitest";

import { parseHyperliquidExchangeOrders } from "../hl-exchange-parse";

const URL = "https://api-ui.hyperliquid.xyz/exchange";
const HOST = "app.hyperliquid.xyz";

function parse(action: unknown) {
  return parseHyperliquidExchangeOrders("hyperliquid", URL, HOST, {
    action,
    nonce: 1_700_000_000_000,
    signature: { r: "0x1", s: "0x2", v: 27 },
  });
}

describe("parseHyperliquidExchangeOrders — catch-all routing", () => {
  it("routes an unmodeled, non-benign action to the hl_unknown catch-all (NOT null)", () => {
    const payloads = parse({ type: "convertToMultiSigUser", signers: [], threshold: 2 });
    expect(payloads).not.toBeNull();
    expect(payloads).toHaveLength(1);
    expect(payloads![0].hlAction).toEqual({
      kind: "unknown",
      actionType: "convertToMultiSigUser",
    });
  });

  it("passes a benign high-frequency action through as null (unevaluated)", () => {
    expect(parse({ type: "cancel", cancels: [{ a: 0, o: 123 }] })).toBeNull();
    expect(parse({ type: "modify", oid: 1, order: {} })).toBeNull();
    expect(parse({ type: "scheduleCancel", time: 0 })).toBeNull();
  });

  it("still parses a modeled order action (regression)", () => {
    const payloads = parse({
      type: "order",
      orders: [{ a: 0, b: true, p: "95000", s: "0.05", r: false, t: { limit: { tif: "Gtc" } } }],
      grouping: "na",
    });
    expect(payloads).toHaveLength(1);
    expect(payloads![0].hlAction.kind).toBe("order");
  });

  it("returns null for a non-action body (not an /exchange action)", () => {
    expect(parse(undefined)).toBeNull();
    expect(parseHyperliquidExchangeOrders("hyperliquid", URL, HOST, { foo: 1 })).toBeNull();
  });
});
