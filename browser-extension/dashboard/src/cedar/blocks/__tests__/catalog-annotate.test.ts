import { describe, it, expect } from "vitest";
import { estToBlocks, childExprs, attrPath } from "../../blocks";
import type { Expr, PolicyIR, SchemaDescriptor, SchemaField } from "../../blocks";
import real from "./fixtures/real-policies-est.json";

// Collect every attr node in an IR's conditions.
function attrNodes(ir: PolicyIR): Extract<Expr, { kind: "attr" }>[] {
  const acc: Extract<Expr, { kind: "attr" }>[] = [];
  const visit = (e: Expr): void => {
    if (e.kind === "attr") acc.push(e);
    childExprs(e).forEach(visit);
  };
  ir.conditions.forEach((c) => visit(c.body));
  return acc;
}

describe("field-catalog annotation (end-to-end join)", () => {
  it("custom attr nodes get source+type when the catalog is keyed by the policy action id", () => {
    let proven = false;
    for (const { est } of real as { est: unknown }[]) {
      const bare = estToBlocks(est as Parameters<typeof estToBlocks>[0], null);
      if (bare.scope.action.kind !== "scopeEq") continue;
      const actionId = (bare.scope.action as { kind: "scopeEq"; entity: { id: string } }).entity.id;

      // Derive the custom paths this policy actually uses, from its own IR.
      const customPaths = attrNodes(bare)
        .map((a) => attrPath(a.of, a.attr))
        .filter((p): p is string => !!p && p.startsWith("context.custom."));
      if (customPaths.length === 0) continue;

      // Catalog keyed by the EXACT Pascal action id (as the wasm export emits).
      const descriptor: SchemaDescriptor = {
        [actionId]: customPaths.map(
          (path): SchemaField => ({ path, type: "Long", fieldKind: "primitive", source: "custom" }),
        ),
      };

      const annotated = estToBlocks(est as Parameters<typeof estToBlocks>[0], descriptor);
      const typedCustom = attrNodes(annotated).filter(
        (a) => (a as { source?: string }).source === "custom" && (a as { type?: string }).type,
      );
      expect(typedCustom.length, `action ${actionId} should yield ≥1 typed custom attr`).toBeGreaterThan(0);
      proven = true;
      break;
    }
    expect(proven, "expected a concrete-action policy touching context.custom").toBe(true);
  });
});
