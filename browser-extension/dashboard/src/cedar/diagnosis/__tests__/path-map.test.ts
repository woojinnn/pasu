import { describe, it, expect } from "vitest";
import type { PolicyIR, Expr } from "../../blocks/ir";
import { pathToBlockId, enumeratePaths } from "../path";

const leaf: Expr = { kind: "attr", of: { kind: "var", name: "context" }, attr: "slippageBp" };
const lit: Expr = { kind: "lit", litType: "long", value: 100 };
const body: Expr = { kind: "binary", op: ">", left: leaf, right: lit };
const policy: PolicyIR = {
  kind: "policy", effect: "forbid", annotations: [],
  scope: { principal: { kind: "scopeAll" }, action: { kind: "scopeAll" }, resource: { kind: "scopeAll" } },
  conditions: [{ kind: "when", body }],
};

describe("pathToBlockId combiner", () => {
  it("derives path->blockId from an Expr->blockId identity map", () => {
    // Simulate what irToWorkspace records: each Expr node → a fake block id.
    const blockIdByNode = new Map<Expr, string>();
    for (const { node } of enumeratePaths(policy)) blockIdByNode.set(node, `blk_${blockIdByNode.size}`);
    const map = pathToBlockId(policy, blockIdByNode);
    expect(map.get("c0.body")).toBe(blockIdByNode.get(body));
    expect(map.get("c0.body.left")).toBe(blockIdByNode.get(leaf));
    expect(map.get("c0.body.right")).toBe(blockIdByNode.get(lit));
  });
});
