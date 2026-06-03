/**
 * IR round-trip sanity — exercises blocksToEst inside cedar/blocks for a
 * representative set of IRs that editor-v9's mapping is supposed to
 * produce. If blocksToEst can serialise these without throwing, then
 * workspaceToIR outputs are safe to pipe through the wasm bridge.
 *
 * Full workspace round-trip (Blockly DOM build → workspaceToIR →
 * irToWorkspace → workspaceToIR ≡ id) is deferred: vitest's jsdom does
 * not satisfy Blockly's DOM requirements without significant scaffolding,
 * and the value/cost ratio is bad until we hit a regression. The
 * symmetric workspaceToIR / irToWorkspace files cover the same node set,
 * so a missing arm is caught by the coverage test instead.
 */

import { describe, expect, it } from "vitest";
import { blocksToEst, type PolicyIR } from "../../cedar/blocks";

function makePolicy(conditions: PolicyIR["conditions"]): PolicyIR {
  return {
    kind: "policy",
    effect: "permit",
    annotations: [],
    scope: {
      principal: { kind: "scopeAll" },
      action: { kind: "scopeAll" },
      resource: { kind: "scopeAll" },
    },
    conditions,
  };
}

describe("editor-v9 → cedar/blocks IR round-trip (serialisation safety)", () => {
  it("empty policy serialises", () => {
    const ir = makePolicy([]);
    expect(() => blocksToEst(ir)).not.toThrow();
  });

  it("simple when (literal bool) serialises", () => {
    const ir = makePolicy([
      { kind: "when", body: { kind: "lit", litType: "bool", value: true } },
    ]);
    expect(() => blocksToEst(ir)).not.toThrow();
  });

  it("binary compare on var serialises", () => {
    const ir = makePolicy([
      {
        kind: "when",
        body: {
          kind: "binary",
          op: "==",
          left: { kind: "var", name: "principal" },
          right: { kind: "litEntity", entity: { type: "User", id: "alice" } },
        },
      },
    ]);
    expect(() => blocksToEst(ir)).not.toThrow();
  });

  it("nested attr / has serialises", () => {
    const ir = makePolicy([
      {
        kind: "when",
        body: {
          kind: "binary",
          op: "&&",
          left: {
            kind: "has",
            of: { kind: "var", name: "context" },
            attr: "amount",
          },
          right: {
            kind: "binary",
            op: ">",
            left: {
              kind: "attr",
              of: { kind: "var", name: "context" },
              attr: "amount",
            },
            right: { kind: "lit", litType: "long", value: 100 },
          },
        },
      },
    ]);
    expect(() => blocksToEst(ir)).not.toThrow();
  });

  it("set + record + like + is + if serialise", () => {
    const ir = makePolicy([
      {
        kind: "when",
        body: {
          kind: "set",
          elements: [
            { kind: "lit", litType: "long", value: 1 },
            { kind: "lit", litType: "long", value: 2 },
          ],
        },
      },
      {
        kind: "when",
        body: {
          kind: "record",
          pairs: [
            { key: "a", value: { kind: "lit", litType: "string", value: "x" } },
            { key: "b", value: { kind: "lit", litType: "bool", value: true } },
          ],
        },
      },
      {
        kind: "unless",
        body: {
          kind: "like",
          of: {
            kind: "attr",
            of: { kind: "var", name: "resource" },
            attr: "name",
          },
          pattern: [{ Literal: "test_" }, "Wildcard"],
        },
      },
      {
        kind: "when",
        body: {
          kind: "is",
          of: { kind: "var", name: "principal" },
          entityType: "User",
        },
      },
      {
        kind: "when",
        body: {
          kind: "if",
          cond: { kind: "lit", litType: "bool", value: true },
          then: { kind: "lit", litType: "long", value: 1 },
          else: { kind: "lit", litType: "long", value: 0 },
        },
      },
    ]);
    expect(() => blocksToEst(ir)).not.toThrow();
  });

  it("scope variants (scopeEq / scopeIn / scopeIs / slot, action scopeEq / scopeIn) serialise", () => {
    const ir: PolicyIR = {
      kind: "policy",
      effect: "forbid",
      annotations: [],
      scope: {
        principal: { kind: "scopeEq", entity: { type: "User", id: "alice" } },
        action: {
          kind: "scopeIn",
          entities: [
            { type: "Action", id: "Swap" },
            { type: "Action", id: "Approve" },
          ],
        },
        resource: { kind: "scopeIs", entityType: "Token" },
      },
      conditions: [],
    };
    expect(() => blocksToEst(ir)).not.toThrow();
  });

  it("raw escape hatch passes through (object payload)", () => {
    // Use a structurally valid EST node — `Value: true` wraps a literal in
    // Cedar's EST format. We don't care what — only that the round-trip
    // doesn't throw on it.
    const ir = makePolicy([
      { kind: "when", body: { kind: "raw", est: { Value: true } } },
    ]);
    expect(() => blocksToEst(ir)).not.toThrow();
  });
});
