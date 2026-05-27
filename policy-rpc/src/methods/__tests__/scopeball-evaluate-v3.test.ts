// Phase 5D — `scopeball.evaluate_v3` mock-method tests.
//
// Pins the wire shape the SW client (`policy-rpc.ts::evaluateV3`)
// expects: `policyRequest.actions` echoes the caller-supplied
// envelopes; `state_before` / `deltas` / `state_after` are empty;
// `diagnostics` is `[]`. Phase 6 swaps the body for the real reducer +
// state-sync — these tests will tighten then.

import { describe, expect, it } from "vitest";

import { createMethodRegistry } from "../registry.js";
import { createScopeballEvaluateV3Method, scopeballEvaluateV3Catalog } from "../scopeball-evaluate-v3.js";

describe("scopeball.evaluate_v3 (Phase 5D echo mock)", () => {
  it("is registered in the bundled catalog with the documented shape", () => {
    const registry = createMethodRegistry();
    expect(registry.listMethods()).toContain("scopeball.evaluate_v3");
    const entry = registry.catalog().methods["scopeball.evaluate_v3"];
    expect(entry).toBe(scopeballEvaluateV3Catalog);
    expect(entry.origin).toBe("bundled");
    expect(Object.keys(entry.params)).toEqual([
      "wallet_id",
      "envelopes",
      "eval_context",
    ]);
  });

  it("echoes envelopes into policyRequest.actions and returns empty state fields", async () => {
    const fn = createScopeballEvaluateV3Method();
    const envelopes = [
      { meta: { submitter: "0xaaaa" }, body: { domain: "token", action: "erc20_approve" } },
      { meta: { submitter: "0xaaaa" }, body: { domain: "amm", action: "swap" } },
    ];
    const evalContext = {
      chain: "eip155:1",
      clock: 1_700_000_000,
      request_kind: "Transaction",
    };
    const walletId = { address: "0xaaaa", chains: ["eip155:1"] };

    const result = await fn({
      wallet_id: walletId,
      envelopes,
      eval_context: evalContext,
    });

    expect(result).toEqual({
      policyRequest: {
        actions: envelopes,
        state_before: {},
        deltas: [],
        state_after: {},
        wallet_id: walletId,
        eval_context: evalContext,
      },
      diagnostics: [],
    });
  });

  it("rejects params that are not an object", async () => {
    const fn = createScopeballEvaluateV3Method();
    await expect(fn("not an object")).rejects.toThrow(/params must be an object/);
    await expect(fn([])).rejects.toThrow(/params must be an object/);
  });

  it("rejects missing required fields", async () => {
    const fn = createScopeballEvaluateV3Method();
    await expect(
      fn({ envelopes: [], eval_context: {} }),
    ).rejects.toThrow(/wallet_id is required/);
    await expect(
      fn({ wallet_id: {}, eval_context: {} }),
    ).rejects.toThrow(/envelopes must be an array/);
    await expect(
      fn({ wallet_id: {}, envelopes: [], eval_context: undefined }),
    ).rejects.toThrow(/eval_context is required/);
  });

  it("rejects non-array envelopes", async () => {
    const fn = createScopeballEvaluateV3Method();
    await expect(
      fn({ wallet_id: {}, envelopes: {}, eval_context: {} }),
    ).rejects.toThrow(/envelopes must be an array/);
  });

  it("registry.execute round-trips through the echo method", async () => {
    const registry = createMethodRegistry();
    const result = await registry.execute({
      id: "rt-1",
      method: "scopeball.evaluate_v3",
      params: {
        wallet_id: { address: "0xbeef", chains: ["eip155:8453"] },
        envelopes: [],
        eval_context: { chain: "eip155:8453", clock: 0, request_kind: "Transaction" },
      },
    });
    expect(result).toEqual({
      id: "rt-1",
      ok: true,
      result: {
        policyRequest: {
          actions: [],
          state_before: {},
          deltas: [],
          state_after: {},
          wallet_id: { address: "0xbeef", chains: ["eip155:8453"] },
          eval_context: { chain: "eip155:8453", clock: 0, request_kind: "Transaction" },
        },
        diagnostics: [],
      },
    });
  });
});
