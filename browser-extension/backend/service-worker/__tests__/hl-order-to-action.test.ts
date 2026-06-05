/**
 * Contract test: `hlOrderToAction` reproduces the EXACT `{ action, meta }` JSON
 * the Rust v2 entry point (`evaluate_action_v2_json`) deserializes into an
 * `ActionBody::HyperliquidCore(...)` — pinned against the same canonical shapes
 * the Rust e2e test builds (`crates/policy-engine-wasm/tests/hl_core_deny_e2e.rs`).
 *
 * If the converter drifts from the Rust serde shape, this fails loudly rather
 * than silently fail-closing at runtime.
 */
import { describe, it, expect } from "vitest";
import { hlOrderToAction } from "../hl-order-to-action";
import type { VenueActionWire, VenueOrderPayload } from "@lib/types";
import { RequestType } from "@lib/types";

function payload(hlAction: VenueActionWire, symbol?: string): VenueOrderPayload {
  const p: VenueOrderPayload = {
    type: RequestType.VENUE_ORDER,
    chainId: 0,
    hostname: "app.hyperliquid.xyz",
    venue: "hyperliquid",
    endpoint: "https://api-ui.hyperliquid.xyz/exchange",
    hlAction,
  };
  if (symbol !== undefined) p.symbol = symbol;
  return p;
}

const shortBtc = (): VenueOrderPayload =>
  payload({
    kind: "order",
    order: { a: 0, b: false, p: "60000", s: "0.1", r: false, t: { limit: { tif: "Gtc" } } },
  });

describe("hlOrderToAction", () => {
  it("converts a Hyperliquid short order to the canonical HyperliquidCore ActionBody", () => {
    const { action, meta } = hlOrderToAction(shortBtc());
    expect(action).toEqual({
      domain: "hyperliquid_core",
      action: "hl_order",
      asset_index: 0,
      is_buy: false,
      price: "60000",
      size: "0.1",
      reduce_only: false,
      tif: "gtc",
    });
    // Fractional size is preserved verbatim (no truncation to "0").
    expect(action.size).toBe("0.1");
    expect(meta).toMatchObject({
      submitter: expect.any(String),
      nature: { kind: "offchain_sig" },
    });
  });

  it("maps buy=true to is_buy and carries reduce_only + symbol when resolved", () => {
    const { action } = hlOrderToAction(
      payload(
        { kind: "order", order: { a: 0, b: true, p: "60000", s: "0.5", r: true } },
        "BTC-USD",
      ),
    );
    expect(action.is_buy).toBe(true);
    expect(action.reduce_only).toBe(true);
    expect(action.symbol).toBe("BTC-USD");
  });

  it("omits symbol when unresolved (Rust lowering falls back to ASSET-<index>)", () => {
    const { action } = hlOrderToAction(shortBtc());
    expect(action.symbol).toBeUndefined();
  });

  it("normalizes Alo → post_only and Ioc → ioc", () => {
    const alo = hlOrderToAction(
      payload({ kind: "order", order: { a: 0, b: true, p: "1", s: "1", t: { limit: { tif: "Alo" } } } }),
    ).action;
    expect(alo.tif).toBe("post_only");
    const ioc = hlOrderToAction(
      payload({ kind: "order", order: { a: 0, b: true, p: "1", s: "1", t: { limit: { tif: "Ioc" } } } }),
    ).action;
    expect(ioc.tif).toBe("ioc");
  });

  it("converts updateLeverage", () => {
    const { action } = hlOrderToAction(
      payload({ kind: "update_leverage", assetIndex: 1, isCross: false, leverage: 25 }, "ETH-USD"),
    );
    expect(action).toEqual({
      domain: "hyperliquid_core",
      action: "hl_update_leverage",
      asset_index: 1,
      is_cross: false,
      leverage: 25,
      symbol: "ETH-USD",
    });
  });

  it("converts withdraw3 (fund movement)", () => {
    const { action } = hlOrderToAction(
      payload({ kind: "withdraw", destination: "0x000000000000000000000000000000000000dEaD", amount: "1000.5" }),
    );
    expect(action).toEqual({
      domain: "hyperliquid_core",
      action: "hl_withdraw",
      destination: "0x000000000000000000000000000000000000dEaD",
      amount: "1000.5",
    });
  });

  it("converts usdSend (fund movement)", () => {
    const { action } = hlOrderToAction(
      payload({ kind: "usd_send", destination: "0x000000000000000000000000000000000000bEEF", amount: "250" }),
    );
    expect(action).toMatchObject({
      domain: "hyperliquid_core",
      action: "hl_usd_send",
      destination: "0x000000000000000000000000000000000000bEEF",
      amount: "250",
    });
  });

  it("converts approveAgent (delegation), carrying agent_name when present", () => {
    const withName = hlOrderToAction(
      payload({ kind: "approve_agent", agentAddress: "0x00000000000000000000000000000000000a6e47", agentName: "bot" }),
    ).action;
    expect(withName).toEqual({
      domain: "hyperliquid_core",
      action: "hl_approve_agent",
      agent_address: "0x00000000000000000000000000000000000a6e47",
      agent_name: "bot",
    });
    const noName = hlOrderToAction(
      payload({ kind: "approve_agent", agentAddress: "0x00000000000000000000000000000000000a6e47" }),
    ).action;
    expect(noName.agent_name).toBeUndefined();
  });

  it("converts spotSend (spot token fund movement)", () => {
    const { action } = hlOrderToAction(
      payload({
        kind: "spot_send",
        destination: "0x000000000000000000000000000000000000bEEF",
        token: "USDC:0xc1fb593aeffbeb02f85e0308e9956a90",
        amount: "500.25",
      }),
    );
    expect(action).toEqual({
      domain: "hyperliquid_core",
      action: "hl_spot_send",
      destination: "0x000000000000000000000000000000000000bEEF",
      token: "USDC:0xc1fb593aeffbeb02f85e0308e9956a90",
      amount: "500.25",
    });
  });

  it("converts sendToEvmWithData (preserves recipient + raw data)", () => {
    const { action } = hlOrderToAction(
      payload({
        kind: "send_to_evm_with_data",
        token: "USDC",
        amount: "1000",
        sourceDex: "",
        destinationRecipient: "0x000000000000000000000000000000000000dEaD",
        data: "0xdeadbeef",
      }),
    );
    expect(action).toEqual({
      domain: "hyperliquid_core",
      action: "hl_send_to_evm_with_data",
      token: "USDC",
      amount: "1000",
      source_dex: "",
      destination_recipient: "0x000000000000000000000000000000000000dEaD",
      data: "0xdeadbeef",
    });
  });

  it("converts vaultTransfer (carries isDeposit + usd)", () => {
    const { action } = hlOrderToAction(
      payload({
        kind: "vault_transfer",
        vaultAddress: "0x000000000000000000000000000000000000dEaD",
        isDeposit: true,
        usd: "250",
      }),
    );
    expect(action).toEqual({
      domain: "hyperliquid_core",
      action: "hl_vault_transfer",
      vault_address: "0x000000000000000000000000000000000000dEaD",
      is_deposit: true,
      usd: "250",
    });
  });

  it("converts tokenDelegate (delegation), carrying isUndelegate", () => {
    const { action } = hlOrderToAction(
      payload({
        kind: "token_delegate",
        validator: "0x000000000000000000000000000000000000bEEF",
        isUndelegate: false,
        wei: "1000000000",
      }),
    );
    expect(action).toEqual({
      domain: "hyperliquid_core",
      action: "hl_token_delegate",
      validator: "0x000000000000000000000000000000000000bEEF",
      is_undelegate: false,
      wei: "1000000000",
    });
  });

  it("converts twapOrder (side + minutes + randomize), with symbol when resolved", () => {
    const { action } = hlOrderToAction(
      payload(
        {
          kind: "twap_order",
          assetIndex: 0,
          isBuy: true,
          size: "10",
          reduceOnly: false,
          minutes: 30,
          randomize: true,
        },
        "BTC-USD",
      ),
    );
    expect(action).toEqual({
      domain: "hyperliquid_core",
      action: "hl_twap_order",
      asset_index: 0,
      is_buy: true,
      size: "10",
      reduce_only: false,
      minutes: 30,
      randomize: true,
      symbol: "BTC-USD",
    });
  });

  it("threads the request nonce (ms) into meta.submitted_at (seconds) + deadline", () => {
    const p = shortBtc();
    p.nonce = 1_700_000_000_000; // ms
    const { meta } = hlOrderToAction(p);
    expect(meta.submitted_at).toBe(1_700_000_000); // ÷1000 → seconds
    expect((meta.nature as { deadline: number }).deadline).toBe(1_700_000_600);
  });

  it("falls back to the sentinel submitted_at when no nonce is present", () => {
    const { meta } = hlOrderToAction(shortBtc());
    expect(meta.submitted_at).toBe(1_738_000_000);
  });

  it("converts the hl_unknown catch-all (raw wire type only)", () => {
    const { action } = hlOrderToAction(
      payload({ kind: "unknown", actionType: "convertToMultiSigUser" }),
    );
    expect(action).toEqual({
      domain: "hyperliquid_core",
      action: "hl_unknown",
      action_type: "convertToMultiSigUser",
    });
  });
});
