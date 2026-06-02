import { describe, it, expect } from "vitest";
import { estToBlocks } from "../estToBlocks";
import { type SchemaDescriptor, descriptorFromCustomTypes } from "../schema";

const desc: SchemaDescriptor = {
  Swap: [
    { path: "context.custom.totalInputUsd", type: "UsdValuation", fieldKind: "primitive", source: "custom" },
    { path: "context.amount", type: "String", fieldKind: "primitive", source: "base" },
  ],
};

// permit(principal, action == Action::"Swap", resource) when {
//   context.custom.totalInputUsd > 10000 && context.amount == "0x1" && context.other > 1 };
const ctxAttr = (attr: string): any => ({ ".": { left: { Var: "context" }, attr } });
const customAttrEst = {
  ".": { left: { ".": { left: { Var: "context" }, attr: "custom" } }, attr: "totalInputUsd" },
};
const est: any = {
  effect: "permit",
  principal: { op: "All" },
  action: { op: "==", entity: { type: "Action", id: "Swap" } },
  resource: { op: "All" },
  conditions: [
    {
      kind: "when",
      body: {
        "&&": {
          left: {
            "&&": {
              left: { ">": { left: customAttrEst, right: { Value: 10000 } } },
              right: { "==": { left: ctxAttr("amount"), right: { Value: "0x1" } } },
            },
          },
          right: { ">": { left: ctxAttr("other"), right: { Value: 1 } } },
        },
      },
    },
  ],
};

describe("schema-aware annotation", () => {
  it("classifies custom, base, and unknown attrs with types", () => {
    const ir: any = estToBlocks(est, desc);
    const top = ir.conditions[0].body; // &&
    const customAttr = top.left.left.left; // context.custom.totalInputUsd
    const baseAttr = top.left.right.left; // context.amount
    const unknownAttr = top.right.left; // context.other
    expect(customAttr.source).toBe("custom");
    expect(customAttr.type).toBe("UsdValuation");
    expect(baseAttr.source).toBe("base");
    expect(baseAttr.type).toBe("String");
    expect(unknownAttr.source).toBe("unknown");
    expect(unknownAttr.type).toBeUndefined();
  });

  it("leaves attrs unannotated when no schema is supplied", () => {
    const ir: any = estToBlocks(est, null);
    expect(ir.conditions[0].body.left.left.left.source).toBeUndefined();
  });

  it("builds a descriptor from enriched-schema preview custom types", () => {
    const d = descriptorFromCustomTypes([
      {
        name: "Swap",
        fields: [
          { field: "totalInputUsd", cedar_type: "UsdValuation" },
          { field: "tags", cedar_type: "Set<String>" },
        ],
      },
    ]);
    expect(d.Swap[0]).toEqual({
      path: "context.custom.totalInputUsd",
      type: "UsdValuation",
      fieldKind: "primitive",
      source: "custom",
    });
    expect(d.Swap[1].fieldKind).toBe("collection");
    // end-to-end: the generated descriptor annotates a matching attr.
    const ir: any = estToBlocks(est, d);
    expect(ir.conditions[0].body.left.left.left.type).toBe("UsdValuation");
  });
});
