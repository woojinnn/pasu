/**
 * Manifest auto-generation tests. The golden case asserts the generator
 * reproduces the hand-authored shipped bundle
 * `crates/policy-engine/tests/fixtures/default_policies_v2/swap-input-usd-cap-deny/manifest.json`
 * (validated Rust-side by `default_policies_v2.rs` + `swap_input_usd_cap_eval.rs`),
 * so the editor path and the proven fixture stay in lock-step.
 */

import { describe, expect, it } from "vitest";

import type { Expr, PolicyIR } from "../../../cedar/blocks";
import { collectCustomFields, generateManifest } from "../generate";
import type { EnrichmentRegistry } from "../registry";

const CTX_CUSTOM: Expr = {
  kind: "attr",
  of: { kind: "var", name: "context" },
  attr: "custom",
};

/** `context.custom.<field>` value access. */
const customField = (field: string): Expr => ({ kind: "attr", of: CTX_CUSTOM, attr: field });

/** Build a forbid policy reading `context.custom.<field>` with the given
 *  annotations + action, mirroring an /editor enrichment policy. */
function enrichmentPolicy(opts: {
  id?: string;
  severity?: string;
  field: string;
  actionId?: string;
  actionKind?: "scopeEq" | "scopeAll";
}): PolicyIR {
  const annotations = [
    ...(opts.id ? [{ name: "id", value: opts.id }] : []),
    ...(opts.severity ? [{ name: "severity", value: opts.severity }] : []),
  ];
  // `context has custom && context.custom has FIELD && context.custom.FIELD >= …`
  const body: Expr = {
    kind: "binary",
    op: "&&",
    left: {
      kind: "binary",
      op: "&&",
      left: { kind: "has", of: { kind: "var", name: "context" }, attr: "custom" },
      right: { kind: "has", of: CTX_CUSTOM, attr: opts.field },
    },
    right: {
      kind: "ext",
      fn: "greaterThanOrEqual",
      args: [
        customField(opts.field),
        { kind: "ext", fn: "decimal", args: [{ kind: "lit", litType: "string", value: "0.0500" }] },
      ],
    },
  };
  return {
    kind: "policy",
    effect: "forbid",
    annotations,
    scope: {
      principal: { kind: "scopeAll" },
      action:
        opts.actionKind === "scopeAll"
          ? { kind: "scopeAll" }
          : { kind: "scopeEq", entity: { type: "Amm::Action", id: opts.actionId ?? "Swap" } },
      resource: { kind: "scopeAll" },
    },
    conditions: [{ kind: "when", body }],
  };
}

describe("collectCustomFields", () => {
  it("finds context.custom.<field> but not `context has custom`", () => {
    const policy = enrichmentPolicy({ id: "p", severity: "deny", field: "inputUsd" });
    expect(collectCustomFields(policy)).toEqual(["inputUsd"]);
  });

  it("returns empty for a base-context policy (no context.custom)", () => {
    const policy: PolicyIR = {
      kind: "policy",
      effect: "forbid",
      annotations: [{ name: "id", value: "p" }],
      scope: {
        principal: { kind: "scopeAll" },
        action: { kind: "scopeEq", entity: { type: "Amm::Action", id: "Swap" } },
        resource: { kind: "scopeAll" },
      },
      conditions: [
        {
          kind: "when",
          body: {
            kind: "binary",
            op: ">",
            left: { kind: "attr", of: { kind: "var", name: "context" }, attr: "slippageBp" },
            right: { kind: "lit", litType: "long", value: 100 },
          },
        },
      ],
    };
    expect(collectCustomFields(policy)).toEqual([]);
  });
});

describe("generateManifest", () => {
  it("reproduces the shipped swap-input-usd-cap-deny manifest (golden)", () => {
    const policy = enrichmentPolicy({
      id: "swap-input-usd-cap-deny",
      severity: "deny",
      field: "inputUsd",
    });
    const { manifest, errors } = generateManifest(policy);

    expect(errors).toEqual([]);
    expect(manifest).toEqual({
      id: "swap-input-usd-cap-deny",
      schema_version: 2,
      trigger: { where: { "action.tag": { eq: "swap" } } },
      policy_rpc: [
        {
          id: "inputUsd",
          method: "oracle.usd_value",
          params: {
            chain_id: "$.root.chain_id",
            asset: "$.action.tokenIn.key.address",
            amount: "$.action.direction.amountIn",
          },
          outputs: [
            {
              kind: "context",
              field: "inputUsd",
              type: "Decimal",
              from: "$.result.usd",
              required: true,
            },
          ],
          optional: false,
        },
      ],
      custom_context: { fields: { inputUsd: "decimal" } },
    });
  });

  it("base-context policy → no manifest, no errors", () => {
    const policy = enrichmentPolicy({ id: "p", severity: "deny", field: "inputUsd" });
    policy.conditions = []; // strip the custom-field references
    const { manifest, errors } = generateManifest(policy);
    expect(manifest).toBeUndefined();
    expect(errors).toEqual([]);
  });

  it("warn severity → optional call (may fail open)", () => {
    const policy = enrichmentPolicy({ id: "p", severity: "warn", field: "inputUsd" });
    const { manifest } = generateManifest(policy);
    expect(manifest?.policy_rpc[0].optional).toBe(true);
    expect(manifest?.policy_rpc[0].outputs[0].required).toBe(false);
  });

  it("unbound custom field → error, no manifest", () => {
    const policy = enrichmentPolicy({ id: "p", severity: "deny", field: "madeUpField" });
    const { manifest, errors } = generateManifest(policy);
    expect(manifest).toBeUndefined();
    expect(errors).toHaveLength(1);
    expect(errors[0].field).toBe("madeUpField");
  });

  it("field used on an action it does not apply to → error", () => {
    // inputUsd appliesTo ["swap"]; use it on a Transfer action.
    const policy = enrichmentPolicy({
      id: "p",
      severity: "deny",
      field: "inputUsd",
      actionId: "Erc20Transfer",
    });
    const { manifest, errors } = generateManifest(policy);
    expect(manifest).toBeUndefined();
    expect(errors[0].message).toContain("erc20_transfer");
  });

  it("literal params pass through (normalize_to_nano decimals=6)", () => {
    const registry: EnrichmentRegistry = {
      inputAmountNano: {
        type: "Long",
        label: { ko: "", en: "" },
        appliesTo: ["swap"],
        method: "token.normalize_to_nano",
        projection: "$.result.nano",
        params: { amount: "$.action.direction.amountIn", decimals: { literal: 6 } },
      },
    };
    const policy = enrichmentPolicy({ id: "p", severity: "deny", field: "inputAmountNano" });
    const { manifest } = generateManifest(policy, registry);
    expect(manifest?.policy_rpc[0].params).toEqual({
      amount: "$.action.direction.amountIn",
      decimals: 6,
    });
    expect(manifest?.custom_context.fields).toEqual({ inputAmountNano: "Long" });
  });

  it("missing single action head → error", () => {
    const policy = enrichmentPolicy({
      id: "p",
      severity: "deny",
      field: "inputUsd",
      actionKind: "scopeAll",
    });
    const { manifest, errors } = generateManifest(policy);
    expect(manifest).toBeUndefined();
    expect(errors.some((e) => e.message.includes("action"))).toBe(true);
  });

  it("generates for a field lifted from the seeded defaults (pctOfHolding)", () => {
    // pctOfHolding is registered from phase1A-seed.json with appliesTo
    // ["erc20_transfer"] — the action tag is snake_cased, so it must match.
    const policy = enrichmentPolicy({
      id: "holding-pct-outflow-warn",
      severity: "warn",
      field: "pctOfHolding",
      actionId: "Erc20Transfer",
    });
    const { manifest, errors } = generateManifest(policy);
    expect(errors).toEqual([]);
    expect(manifest?.policy_rpc[0].method).toBe("token.outflow_pct_of_holding");
    expect(manifest?.trigger.where["action.tag"].eq).toBe("erc20_transfer");
    expect(manifest?.custom_context.fields).toEqual({ pctOfHolding: "decimal" });
  });
});
