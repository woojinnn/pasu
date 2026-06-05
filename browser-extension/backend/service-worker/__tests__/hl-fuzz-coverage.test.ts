/**
 * Real-data fuzz coverage for the Hyperliquid `/exchange` verdict boundary.
 *
 * Seeds the generators with a committed snapshot of the HL public info API
 * (`fixtures/hl-info-snapshot.json` — valid asset_index↔symbol + spot tokens)
 * and runs many randomized-but-realistic bodies through the REAL in-page parser
 * (`parseHyperliquidExchangeOrders`) and SW converter (`hlOrderToAction`).
 *
 * It proves a coverage MAP over the whole documented `/exchange` surface:
 *   - modeled actions     → a specific `hl_*` ActionBody,
 *   - benign actions      → `null` (pass through unevaluated, by design),
 *   - everything else     → the `hl_unknown` catch-all.
 *
 * The security invariant (I1): NO fund-movement / permission action ever lands
 * in the passed-through bucket — i.e. a novel such action can never silently
 * reach the venue. This is the regression guard for the original silent-allow
 * gap (see memory `project_hl_order_audit`).
 *
 * Determinism: a seeded LCG, not Math.random, so a failure reproduces exactly.
 */
import { describe, it, expect } from "vitest";

import { parseHyperliquidExchangeOrders } from "../../injected/hl-exchange-parse";
import { hlOrderToAction } from "../hl-order-to-action";
import snapshot from "./fixtures/hl-info-snapshot.json";

const URL = "https://api-ui.hyperliquid.xyz/exchange";
const HOST = "app.hyperliquid.xyz";
const ITERATIONS = 60; // per action type

// ── seeded PRNG (reproducible) ───────────────────────────────────────────────
function makeRng(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (Math.imul(s, 1103515245) + 12345) >>> 0;
    return s / 0xffffffff;
  };
}

// ── generators drawing from real snapshot data ──────────────────────────────
const PERPS = snapshot.perps;
const SPOT_TOKENS = snapshot.spotTokens;

function makeGen(rnd: () => number) {
  const pick = <T>(a: readonly T[]): T => a[Math.floor(rnd() * a.length)];
  const hex = (n: number) =>
    "0x" +
    Array.from({ length: n }, () => Math.floor(rnd() * 16).toString(16)).join("");
  const addr = () => hex(40);
  const dec = () => (rnd() * 1e6).toFixed(Math.floor(rnd() * 6)); // decimal string
  const wei = () => String(Math.floor(rnd() * 1e12)); // integer wei string
  const bool = () => rnd() > 0.5;
  const perp = () => pick(PERPS);
  const token = () => {
    const t = pick(SPOT_TOKENS);
    return `${t.name}:${t.tokenId}`;
  };
  return { pick, addr, dec, wei, bool, perp, token, rnd };
}

type Gen = ReturnType<typeof makeGen>;
type Category = "modeled" | "benign" | "catch_all";

interface Spec {
  type: string;
  category: Category;
  expectTag?: string; // for modeled
  /** Build the `action` object (the parser wraps it in a full request body). */
  action: (g: Gen) => Record<string, unknown>;
}

// The authoritative `/exchange` action catalog (HL docs + python-sdk exchange.py).
const SPECS: Spec[] = [
  // ── modeled: trading ──
  {
    type: "order",
    category: "modeled",
    expectTag: "hl_order",
    action: (g) => {
      const legs = 1 + Math.floor(g.rnd() * 3);
      return {
        type: "order",
        orders: Array.from({ length: legs }, () => ({
          a: g.perp().assetIndex,
          b: g.bool(),
          p: g.dec(),
          s: g.dec(),
          r: g.bool(),
          t: { limit: { tif: g.pick(["Gtc", "Ioc", "Alo"]) } },
        })),
        grouping: "na",
      };
    },
  },
  {
    type: "twapOrder",
    category: "modeled",
    expectTag: "hl_twap_order",
    action: (g) => ({
      type: "twapOrder",
      twap: {
        a: g.perp().assetIndex,
        b: g.bool(),
        s: g.dec(),
        r: g.bool(),
        m: 1 + Math.floor(g.rnd() * 1440),
        t: g.bool(),
      },
    }),
  },
  {
    type: "updateLeverage",
    category: "modeled",
    expectTag: "hl_update_leverage",
    action: (g) => ({
      type: "updateLeverage",
      asset: g.perp().assetIndex,
      isCross: g.bool(),
      leverage: 1 + Math.floor(g.rnd() * 40),
    }),
  },
  {
    type: "updateIsolatedMargin",
    category: "modeled",
    expectTag: "hl_update_isolated_margin",
    action: (g) => ({
      type: "updateIsolatedMargin",
      asset: g.perp().assetIndex,
      isBuy: g.bool(),
      ntli: Math.floor((g.rnd() - 0.5) * 1e8), // signed
    }),
  },
  // ── modeled: fund movement ──
  {
    type: "withdraw3",
    category: "modeled",
    expectTag: "hl_withdraw",
    action: (g) => ({ type: "withdraw3", destination: g.addr(), amount: g.dec(), time: 1 }),
  },
  {
    type: "usdSend",
    category: "modeled",
    expectTag: "hl_usd_send",
    action: (g) => ({ type: "usdSend", destination: g.addr(), amount: g.dec(), time: 1 }),
  },
  {
    type: "spotSend",
    category: "modeled",
    expectTag: "hl_spot_send",
    action: (g) => ({
      type: "spotSend",
      destination: g.addr(),
      token: g.token(),
      amount: g.dec(),
      time: 1,
    }),
  },
  {
    type: "usdClassTransfer",
    category: "modeled",
    expectTag: "hl_usd_class_transfer",
    action: (g) => ({ type: "usdClassTransfer", amount: g.dec(), toPerp: g.bool(), nonce: 1 }),
  },
  {
    type: "sendAsset",
    category: "modeled",
    expectTag: "hl_send_asset",
    action: (g) => ({
      type: "sendAsset",
      destination: g.addr(),
      sourceDex: g.pick(["", "perp"]),
      destinationDex: g.pick(["", "perp"]),
      token: g.token(),
      amount: g.dec(),
    }),
  },
  {
    type: "sendToEvmWithData",
    category: "modeled",
    expectTag: "hl_send_to_evm_with_data",
    action: (g) => ({
      type: "sendToEvmWithData",
      token: g.token(),
      amount: g.dec(),
      sourceDex: g.pick(["", "perp"]),
      destinationRecipient: g.addr(),
      data: "0x" + g.addr().slice(2),
    }),
  },
  {
    type: "cDeposit",
    category: "modeled",
    expectTag: "hl_c_deposit",
    action: (g) => ({ type: "cDeposit", wei: Number(g.wei()) }),
  },
  {
    type: "cWithdraw",
    category: "modeled",
    expectTag: "hl_c_withdraw",
    action: (g) => ({ type: "cWithdraw", wei: Number(g.wei()) }),
  },
  {
    type: "vaultTransfer",
    category: "modeled",
    expectTag: "hl_vault_transfer",
    action: (g) => ({
      type: "vaultTransfer",
      vaultAddress: g.addr(),
      isDeposit: g.bool(),
      usd: Math.floor(g.rnd() * 1e8),
    }),
  },
  {
    type: "subAccountTransfer",
    category: "modeled",
    expectTag: "hl_sub_account_transfer",
    action: (g) => ({
      type: "subAccountTransfer",
      subAccountUser: g.addr(),
      isDeposit: g.bool(),
      usd: Math.floor(g.rnd() * 1e8),
    }),
  },
  // ── modeled: permission ──
  {
    type: "approveAgent",
    category: "modeled",
    expectTag: "hl_approve_agent",
    action: (g) => ({ type: "approveAgent", agentAddress: g.addr(), nonce: 1 }),
  },
  {
    type: "approveBuilderFee",
    category: "modeled",
    expectTag: "hl_approve_builder_fee",
    action: (g) => ({
      type: "approveBuilderFee",
      maxFeeRate: `${(g.rnd() * 0.1).toFixed(4)}%`,
      builder: g.addr(),
      nonce: 1,
    }),
  },
  {
    type: "tokenDelegate",
    category: "modeled",
    expectTag: "hl_token_delegate",
    action: (g) => ({
      type: "tokenDelegate",
      validator: g.addr(),
      wei: Number(g.wei()),
      isUndelegate: g.bool(),
      nonce: 1,
    }),
  },
  // ── benign: high-frequency, fund-/permission-neutral → null ──
  { type: "cancel", category: "benign", action: (g) => ({ type: "cancel", cancels: [{ a: g.perp().assetIndex, o: 1 }] }) },
  { type: "cancelByCloid", category: "benign", action: (g) => ({ type: "cancelByCloid", cancels: [{ asset: g.perp().assetIndex, cloid: "0x1" }] }) },
  { type: "modify", category: "benign", action: () => ({ type: "modify", oid: 1, order: {} }) },
  { type: "batchModify", category: "benign", action: () => ({ type: "batchModify", modifies: [] }) },
  { type: "twapCancel", category: "benign", action: (g) => ({ type: "twapCancel", a: g.perp().assetIndex, t: 1 }) },
  { type: "scheduleCancel", category: "benign", action: () => ({ type: "scheduleCancel", time: 0 }) },
  { type: "noop", category: "benign", action: () => ({ type: "noop" }) },
  { type: "reserveRequestWeight", category: "benign", action: () => ({ type: "reserveRequestWeight", weight: 1 }) },
  { type: "setReferrer", category: "benign", action: () => ({ type: "setReferrer", code: "REF" }) },
  { type: "createSubAccount", category: "benign", action: () => ({ type: "createSubAccount", name: "sub" }) },
  { type: "subAccountModify", category: "benign", action: () => ({ type: "subAccountModify", subAccountUser: "0x1", name: "x" }) },
  { type: "vaultModify", category: "benign", action: () => ({ type: "vaultModify", vaultAddress: "0x1" }) },
  { type: "spotUser", category: "benign", action: () => ({ type: "spotUser", toggleSpotDusting: { optOut: true } }) },
  { type: "evmUserModify", category: "benign", action: () => ({ type: "evmUserModify", usingBigBlocks: true }) },
  // ── catch-all: unmodeled, non-benign → hl_unknown ──
  { type: "convertToMultiSigUser", category: "catch_all", action: () => ({ type: "convertToMultiSigUser", signers: {}, nonce: 1 }) },
  { type: "perpDeploy", category: "catch_all", action: () => ({ type: "perpDeploy", registerAsset: {} }) },
  { type: "CDeposit", category: "catch_all", action: (g) => ({ type: "CDeposit", wei: Number(g.wei()) }) }, // wrong-case ≠ modeled cDeposit
  { type: "futureUnknownAction", category: "catch_all", action: () => ({ type: "futureUnknownAction", whatever: 1 }) },
];

const FUND_OR_PERMISSION = new Set([
  "withdraw3", "usdSend", "spotSend", "usdClassTransfer", "sendAsset",
  "sendToEvmWithData", "cDeposit", "cWithdraw", "vaultTransfer",
  "subAccountTransfer", "approveAgent", "approveBuilderFee", "tokenDelegate",
]);

interface Bucket {
  handled: Set<string>;
  catch_all: Set<string>;
  passed_through: Set<string>;
}

describe("HL /exchange fuzz coverage (real info-API-seeded)", () => {
  const rnd = makeRng(0x5c0beba1);
  const g = makeGen(rnd);
  const bucket: Bucket = { handled: new Set(), catch_all: new Set(), passed_through: new Set() };

  it("classifies every documented /exchange action correctly across fuzzed inputs", () => {
    for (const spec of SPECS) {
      for (let i = 0; i < ITERATIONS; i++) {
        const body = {
          action: spec.action(g),
          nonce: 1_700_000_000_000 + i,
          signature: { r: "0x1", s: "0x2", v: 27 },
        };
        const payloads = parseHyperliquidExchangeOrders("hyperliquid", URL, HOST, body);

        if (spec.category === "benign") {
          bucket.passed_through.add(spec.type);
          expect(payloads, `${spec.type} must pass through (null)`).toBeNull();
          continue;
        }

        // modeled + catch_all both produce payload(s) and reach the engine.
        expect(payloads, `${spec.type} must produce a payload`).not.toBeNull();
        for (const p of payloads!) {
          const { action } = hlOrderToAction(p);
          expect(action.domain).toBe("hyperliquid_core");
          if (spec.category === "modeled") {
            bucket.handled.add(spec.type);
            expect(action.action, `${spec.type} → ${spec.expectTag}`).toBe(spec.expectTag);
          } else {
            bucket.catch_all.add(spec.type);
            expect(action.action, `${spec.type} → hl_unknown`).toBe("hl_unknown");
            expect(action.action_type).toBe(spec.type);
          }
        }
      }
    }

    // ── I1: no fund/permission action is ever passed through unevaluated ──
    for (const t of bucket.passed_through) {
      expect(FUND_OR_PERMISSION.has(t), `INVARIANT I1 violated: ${t} passed through`).toBe(false);
    }

    // ── coverage map (quantitative report) ──
    const map = {
      handled: [...bucket.handled].sort(),
      catch_all: [...bucket.catch_all].sort(),
      passed_through: [...bucket.passed_through].sort(),
    };
    // eslint-disable-next-line no-console
    console.log("HL /exchange coverage map:", JSON.stringify(map, null, 2));

    const modeled = SPECS.filter((s) => s.category === "modeled").map((s) => s.type);
    const benign = SPECS.filter((s) => s.category === "benign").map((s) => s.type);
    const catchAll = SPECS.filter((s) => s.category === "catch_all").map((s) => s.type);

    expect(map.handled).toEqual([...modeled].sort()); // all 17 modeled handled
    expect(map.passed_through).toEqual([...benign].sort());
    expect(map.catch_all).toEqual([...catchAll].sort());
  });
});
