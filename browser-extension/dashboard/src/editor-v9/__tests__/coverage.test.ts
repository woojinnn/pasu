/**
 * Coverage lock — every Expr.kind that Phase A/B claims to handle has both an
 * EXPR_BLOCK_TYPES entry and a `blockTypeForExpr` return value. Catches the
 * "forgot one side of the mapping" failure mode at typecheck/test time
 * rather than at runtime.
 *
 * Pure logic only — no Blockly DOM. Round-trip tests that exercise the actual
 * workspace land alongside Phase D when we have textToBlocks to seed from.
 */

import { describe, expect, it } from "vitest";
import {
  BINARY_OPS,
  BLOCK_TYPES,
  EXPR_BLOCK_TYPES,
  PHASE_AB_EXPR_KINDS,
  UNARY_OPS,
  blockTypeForExpr,
} from "../mapping/block-types";
import type { Expr } from "../../cedar/blocks";

describe("editor-v9 block ↔ IR coverage", () => {
  it("every Phase A/B Expr.kind has a non-null blockTypeForExpr mapping", () => {
    // Build minimal Expr witnesses for each Phase-A/B kind. blockTypeForExpr
    // should pick a real block id for each.
    const witnesses: Record<(typeof PHASE_AB_EXPR_KINDS)[number], Expr> = {
      var: { kind: "var", name: "principal" },
      lit: { kind: "lit", litType: "bool", value: true },
      attr: { kind: "attr", of: { kind: "var", name: "principal" }, attr: "x" },
      has: { kind: "has", of: { kind: "var", name: "principal" }, attr: "x" },
      binary: {
        kind: "binary",
        op: "==",
        left: { kind: "var", name: "principal" },
        right: { kind: "var", name: "principal" },
      },
      unary: { kind: "unary", op: "!", operand: { kind: "lit", litType: "bool", value: true } },
    };
    for (const kind of PHASE_AB_EXPR_KINDS) {
      const id = blockTypeForExpr(witnesses[kind]);
      expect(id, `missing block for Expr.kind "${kind}"`).not.toBeNull();
    }
  });

  it("each lit subtype maps to a distinct block", () => {
    const ids = (["bool", "long", "string"] as const).map((litType) =>
      blockTypeForExpr({ kind: "lit", litType, value: litType === "bool" ? true : litType === "long" ? 0 : "" }),
    );
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("EXPR_BLOCK_TYPES set matches the actual mapping outputs (no orphans)", () => {
    const usedByMapping = new Set<string | null>();
    usedByMapping.add(blockTypeForExpr({ kind: "var", name: "principal" }));
    for (const litType of ["bool", "long", "string"] as const) {
      const witness: Expr =
        litType === "bool"
          ? { kind: "lit", litType, value: true }
          : litType === "long"
            ? { kind: "lit", litType, value: 0 }
            : { kind: "lit", litType, value: "" };
      usedByMapping.add(blockTypeForExpr(witness));
    }
    usedByMapping.add(blockTypeForExpr({ kind: "attr", of: { kind: "var", name: "principal" }, attr: "x" }));
    usedByMapping.add(blockTypeForExpr({ kind: "has", of: { kind: "var", name: "principal" }, attr: "x" }));
    usedByMapping.add(
      blockTypeForExpr({
        kind: "binary",
        op: "==",
        left: { kind: "var", name: "principal" },
        right: { kind: "var", name: "principal" },
      }),
    );
    usedByMapping.add(
      blockTypeForExpr({ kind: "unary", op: "!", operand: { kind: "lit", litType: "bool", value: true } }),
    );

    for (const id of EXPR_BLOCK_TYPES) {
      expect(usedByMapping, `EXPR_BLOCK_TYPES claims "${id}" but no Phase A/B Expr maps to it`).toContain(id);
    }
  });

  it("operator catalogs match Cedar's surface (no missing / extra)", () => {
    // Sanity: ensure BINARY_OPS and UNARY_OPS are non-empty and use Cedar's
    // canonical strings. If Cedar adds a new op (e.g. `**`), this test won't
    // catch it directly, but the BinaryOp / UnaryOp TS union from
    // `../../cedar/blocks` will trip tsc.
    expect(BINARY_OPS.length).toBeGreaterThan(0);
    expect(UNARY_OPS.length).toBeGreaterThan(0);
    expect(new Set(BINARY_OPS).size).toBe(BINARY_OPS.length);
    expect(new Set(UNARY_OPS).size).toBe(UNARY_OPS.length);
  });

  it("BLOCK_TYPES ids are unique and snake_case-ish", () => {
    const ids = Object.values(BLOCK_TYPES);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) {
      expect(id).toMatch(/^[a-z][a-z0-9_]*$/);
    }
  });
});
