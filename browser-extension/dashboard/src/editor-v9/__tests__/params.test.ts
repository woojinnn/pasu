/**
 * Parameterization round-trip (cedar/blocks side) — exercises makeHole +
 * extractParams + fillParams over the same shapes editor-v9 emits, so a
 * change to either side trips a test before it ships.
 *
 * UI-level tests (clicking the context menu, filling the form) are deferred
 * — they need a Blockly DOM + React Testing Library setup that we don't have
 * yet, and the value-add over the IR-side test is small for now.
 */

import { describe, expect, it } from "vitest";
import {
  extractParams,
  fillParams,
  makeHole,
  type PolicyIR,
} from "../../cedar/blocks";

function basePolicy(): PolicyIR {
  return {
    kind: "policy",
    effect: "forbid",
    annotations: [],
    scope: {
      principal: { kind: "scopeAll" },
      action: { kind: "scopeEq", entity: { type: "Action", id: "Swap" } },
      resource: { kind: "scopeAll" },
    },
    conditions: [],
  };
}

describe("editor-v9 parameterisation (template → adopter)", () => {
  it("makeHole + fillParams round-trips a long literal", () => {
    const tmpl = basePolicy();
    const lit = { kind: "lit" as const, litType: "long" as const, value: 100 };
    const hole = makeHole(lit, { name: "maxUsd", label: "Max swap (USD)" });
    tmpl.conditions.push({
      kind: "when",
      body: {
        kind: "binary",
        op: ">",
        left: { kind: "attr", of: { kind: "var", name: "context" }, attr: "amount" },
        right: hole,
      },
    });

    const specs = extractParams(tmpl);
    expect(specs).toHaveLength(1);
    expect(specs[0].name).toBe("maxUsd");
    expect(specs[0].expected).toBe("lit:long");

    const filled = fillParams(tmpl, { maxUsd: 5000 });
    expect(filled.ok).toBe(true);
    if (filled.ok) {
      // The hole is gone, replaced by lit:long with the new value.
      const cond = filled.policy.conditions[0];
      const body = cond.body as Extract<typeof cond.body, { kind: "binary" }>;
      expect(body.right).toEqual({ kind: "lit", litType: "long", value: 5000 });
    }
  });

  it("required hole missing → fillParams returns missing error", () => {
    const tmpl = basePolicy();
    const lit = { kind: "lit" as const, litType: "string" as const, value: "USDT" };
    const hole = makeHole(lit, { name: "token" });
    tmpl.conditions.push({ kind: "when", body: hole });

    const result = fillParams(tmpl, {});
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errors[0].reason).toBe("missing");
      expect(result.errors[0].name).toBe("token");
    }
  });

  it("optional hole falls back to default when unsupplied", () => {
    const tmpl = basePolicy();
    const lit = { kind: "lit" as const, litType: "long" as const, value: 42 };
    const hole = makeHole(lit, { name: "n", optional: true });
    tmpl.conditions.push({ kind: "when", body: hole });

    const result = fillParams(tmpl, {});
    expect(result.ok).toBe(true);
    if (result.ok) {
      const body = result.policy.conditions[0].body as { kind: "lit"; value: number };
      expect(body.value).toBe(42);
    }
  });

  it("constraint enum is enforced", () => {
    const tmpl = basePolicy();
    const lit = { kind: "lit" as const, litType: "string" as const, value: "low" };
    const hole = makeHole(lit, {
      name: "tier",
      constraints: { enum: ["low", "med", "high"] },
    });
    tmpl.conditions.push({ kind: "when", body: hole });

    const bad = fillParams(tmpl, { tier: "ultra" });
    expect(bad.ok).toBe(false);
    if (!bad.ok) expect(bad.errors[0].reason).toBe("enum");

    const good = fillParams(tmpl, { tier: "med" });
    expect(good.ok).toBe(true);
  });

  it("makeHole rejects non-value blocks (e.g. var)", () => {
    expect(() =>
      makeHole({ kind: "var", name: "principal" }, { name: "p" }),
    ).toThrow();
  });

  it("duplicate hole names trigger extractParams to throw", () => {
    const tmpl = basePolicy();
    const litA = { kind: "lit" as const, litType: "long" as const, value: 1 };
    const litB = { kind: "lit" as const, litType: "long" as const, value: 2 };
    tmpl.conditions.push(
      { kind: "when", body: makeHole(litA, { name: "x" }) },
      { kind: "when", body: makeHole(litB, { name: "x" }) },
    );
    expect(() => extractParams(tmpl)).toThrow();
  });
});
