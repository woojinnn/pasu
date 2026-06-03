/**
 * Coverage lock — every Expr.kind from cedar/blocks (except `hole`, deferred to
 * Phase E) has both an EXPR_BLOCK_TYPES entry and a `blockTypeForExpr` return
 * value. Catches the "forgot one side of the mapping" failure mode at
 * typecheck/test time rather than at runtime.
 *
 * Pure logic only — no Blockly DOM. Workspace-level round-trip tests land
 * alongside Phase D when textToBlocks gives us a way to seed real workspaces.
 */

import { describe, expect, it } from "vitest";
import {
  ALL_EXPR_KINDS,
  BINARY_OPS,
  BLOCK_TYPES,
  EXPR_BLOCK_TYPES,
  UNARY_OPS,
  blockTypeForExpr,
} from "../mapping/block-types";
import type { Expr } from "../../cedar/blocks";

const V_PRINCIPAL: Expr = { kind: "var", name: "principal" };
const V_BOOL: Expr = { kind: "lit", litType: "bool", value: true };

const WITNESSES: Record<(typeof ALL_EXPR_KINDS)[number], Expr> = {
  var: V_PRINCIPAL,
  lit: V_BOOL,
  litEntity: { kind: "litEntity", entity: { type: "User", id: "alice" } },
  set: { kind: "set", elements: [V_BOOL] },
  record: { kind: "record", pairs: [{ key: "k", value: V_BOOL }] },
  attr: { kind: "attr", of: V_PRINCIPAL, attr: "x" },
  has: { kind: "has", of: V_PRINCIPAL, attr: "x" },
  binary: { kind: "binary", op: "==", left: V_PRINCIPAL, right: V_PRINCIPAL },
  unary: { kind: "unary", op: "!", operand: V_BOOL },
  like: { kind: "like", of: V_PRINCIPAL, pattern: [{ Literal: "x" }, "Wildcard"] },
  is: { kind: "is", of: V_PRINCIPAL, entityType: "User" },
  if: { kind: "if", cond: V_BOOL, then: V_BOOL, else: V_BOOL },
  ext: { kind: "ext", fn: "decimal", args: [{ kind: "lit", litType: "string", value: "1.0" }] },
  raw: { kind: "raw", est: { fake: true } },
  hole: { kind: "hole", name: "x", expected: "lit:bool", default: V_BOOL },
};

describe("editor-v9 block ↔ IR coverage", () => {
  it("every Expr.kind in ALL_EXPR_KINDS has a non-null blockTypeForExpr mapping", () => {
    for (const kind of ALL_EXPR_KINDS) {
      const id = blockTypeForExpr(WITNESSES[kind]);
      expect(id, `missing block for Expr.kind "${kind}"`).not.toBeNull();
    }
  });

  it("each lit subtype maps to a distinct block", () => {
    const ids = (["bool", "long", "string"] as const).map((litType) =>
      blockTypeForExpr({
        kind: "lit",
        litType,
        value: litType === "bool" ? true : litType === "long" ? 0 : "",
      }),
    );
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("hole maps to expr_hole (Phase E)", () => {
    const hole: Expr = {
      kind: "hole",
      name: "x",
      expected: "lit:bool",
      default: V_BOOL,
    };
    expect(blockTypeForExpr(hole)).toBe(BLOCK_TYPES.expr_hole);
  });

  it("EXPR_BLOCK_TYPES has no orphans (every entry is reachable from some Expr.kind)", () => {
    const reached = new Set<string | null>();
    for (const kind of ALL_EXPR_KINDS) {
      reached.add(blockTypeForExpr(WITNESSES[kind]));
    }
    // Lit subtypes branch — add the rest of the lit block ids.
    for (const litType of ["long", "string"] as const) {
      const witness: Expr =
        litType === "long"
          ? { kind: "lit", litType, value: 0 }
          : { kind: "lit", litType, value: "" };
      reached.add(blockTypeForExpr(witness));
    }
    for (const id of EXPR_BLOCK_TYPES) {
      expect(reached, `EXPR_BLOCK_TYPES claims "${id}" but no Expr.kind maps to it`).toContain(id);
    }
  });

  it("operator catalogs are non-empty and unique", () => {
    expect(BINARY_OPS.length).toBeGreaterThan(0);
    expect(UNARY_OPS.length).toBeGreaterThan(0);
    expect(new Set(BINARY_OPS).size).toBe(BINARY_OPS.length);
    expect(new Set(UNARY_OPS).size).toBe(UNARY_OPS.length);
  });

  it("BLOCK_TYPES ids are unique and snake_case", () => {
    const ids = Object.values(BLOCK_TYPES);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) {
      expect(id).toMatch(/^[a-z][a-z0-9_]*$/);
    }
  });

  it("wrapper blocks are not in EXPR_BLOCK_TYPES (they're statement children)", () => {
    const wrappers = [
      BLOCK_TYPES.expr_set_item,
      BLOCK_TYPES.expr_record_pair,
      BLOCK_TYPES.expr_ext_arg,
      BLOCK_TYPES.action_scope_in_item,
    ];
    for (const id of wrappers) {
      expect(EXPR_BLOCK_TYPES, `wrapper "${id}" should not be in EXPR_BLOCK_TYPES`).not.toContain(id);
    }
  });
});
