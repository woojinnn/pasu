import { describe, expect, it } from "vitest";

import {
  buildHyperliquidExecutionReport,
  classifyHyperliquidExchangeResponse,
} from "../hl-execution-report";
import { RequestType, type VenueOrderPayload } from "@lib/types";

const payload: VenueOrderPayload = {
  type: RequestType.VENUE_ORDER,
  chainId: 0,
  hostname: "app.hyperliquid.xyz",
  venue: "hyperliquid",
  endpoint: "https://api.hyperliquid.xyz/exchange",
  hlAction: {
    kind: "order",
    order: {
      a: 0,
      b: true,
      p: "200",
      s: "1",
      r: false,
      t: { limit: { tif: "Gtc" } },
      c: "0x00000000000000000000000000000001",
    },
  },
  symbol: "SPCX",
};

describe("Hyperliquid execution report classification", () => {
  it("maps an accepted /exchange response to a venue_accepted report", () => {
    const outcome = classifyHyperliquidExchangeResponse(200, {
      status: "ok",
      response: {
        type: "order",
        data: { statuses: [{ resting: { oid: 987654321 } }] },
      },
    });

    expect(outcome).toEqual({
      kind: "venue_accepted",
      venue: "hyperliquid",
      venue_order_id: "987654321",
      client_order_id: undefined,
    });
  });

  it("maps venue errors to venue_rejected with a reason", () => {
    const outcome = classifyHyperliquidExchangeResponse(200, {
      status: "err",
      response: "Order must have minimum value of $10.",
    });

    expect(outcome).toEqual({
      kind: "venue_rejected",
      venue: "hyperliquid",
      reason: "Order must have minimum value of $10.",
    });
  });

  it("maps per-leg Hyperliquid status errors to venue_rejected", () => {
    const outcome = classifyHyperliquidExchangeResponse(
      200,
      {
        status: "ok",
        response: {
          type: "order",
          data: {
            statuses: [
              { resting: { oid: 111 } },
              { error: "Order must have minimum value of $10." },
            ],
          },
        },
      },
      1,
    );

    expect(outcome).toEqual({
      kind: "venue_rejected",
      venue: "hyperliquid",
      reason: "Order must have minimum value of $10.",
    });
  });

  it("uses the matching per-leg order id for batched accepted responses", () => {
    const outcome = classifyHyperliquidExchangeResponse(
      200,
      {
        status: "ok",
        response: {
          type: "order",
          data: {
            statuses: [{ resting: { oid: 111 } }, { filled: { oid: 222 } }],
          },
        },
      },
      1,
    );

    expect(outcome).toEqual({
      kind: "venue_accepted",
      venue: "hyperliquid",
      venue_order_id: "222",
    });
  });

  it("builds an unattributed execution-report message when no wallet id is known", () => {
    const report = buildHyperliquidExecutionReport(payload, {
      httpStatus: 200,
      responseJson: {
        status: "ok",
        response: { data: { statuses: [{ filled: { oid: 42 } }] } },
      },
    });

    expect(report.type).toBe("execution-report");
    expect(report.wallet_id).toBeUndefined();
    expect(report.outcome).toMatchObject({
      kind: "venue_accepted",
      venue: "hyperliquid",
      venue_order_id: "42",
      client_order_id: "0x00000000000000000000000000000001",
    });
    expect(report.metadata).toMatchObject({
      source: "hyperliquid-fetch-hook",
      endpoint: "https://api.hyperliquid.xyz/exchange",
      symbol: "SPCX",
    });
  });
});
