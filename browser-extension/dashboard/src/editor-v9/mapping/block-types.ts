/**
 * Single source of truth for Blockly block type ↔ PolicyIR node mapping.
 *
 * Two registries are derived from this file:
 *   1. `blocks/register.ts` iterates the JSON exports to register Blockly blocks.
 *   2. `mapping/workspaceToIR.ts` and `mapping/irToWorkspace.ts` switch on the
 *      same ids to convert between Blockly Workspace and PolicyIR.
 *
 * Phase A added: policy_hat, scope_all, action_scope_all, cond_when, expr_lit_bool.
 * Phase B adds: cond_unless, expr_var, expr_lit_long, expr_lit_string, expr_attr,
 *   expr_has, expr_binary, expr_unary.
 *
 * Adding a new Expr.kind: append here, add a block JSON in blocks/, add round-
 * trip arms in workspaceToIR.ts / irToWorkspace.ts. coverage.test.ts fails if
 * any of the three is missing.
 */

import type { BinaryOp, Expr, UnaryOp } from "../../cedar/blocks";

/** Blockly value-input connector check kinds. Used to gate which blocks plug
 *  into which slots. */
export type ConnectorCheck = "Expr" | "Scope" | "ActionScope" | "Cond";

export const BLOCK_TYPES = {
  // ── policy / scope / condition (structural) ──
  policy_hat: "policy_hat",
  scope_all: "scope_all",
  action_scope_all: "action_scope_all",
  cond_when: "cond_when",
  cond_unless: "cond_unless",
  // ── expressions ──
  expr_var: "expr_var",
  expr_lit_bool: "expr_lit_bool",
  expr_lit_long: "expr_lit_long",
  expr_lit_string: "expr_lit_string",
  expr_attr: "expr_attr",
  expr_has: "expr_has",
  expr_binary: "expr_binary",
  expr_unary: "expr_unary",
} as const;

export type BlockTypeId = (typeof BLOCK_TYPES)[keyof typeof BLOCK_TYPES];

/** Block types that carry `output: "Expr"`. Used by the coverage test to assert
 *  every value-producing block type round-trips through PolicyIR. */
export const EXPR_BLOCK_TYPES: readonly BlockTypeId[] = [
  BLOCK_TYPES.expr_var,
  BLOCK_TYPES.expr_lit_bool,
  BLOCK_TYPES.expr_lit_long,
  BLOCK_TYPES.expr_lit_string,
  BLOCK_TYPES.expr_attr,
  BLOCK_TYPES.expr_has,
  BLOCK_TYPES.expr_binary,
  BLOCK_TYPES.expr_unary,
] as const;

/** Expr.kind values that Phase A+B handles end-to-end. Phase C adds the rest
 *  (set, record, litEntity, like, is, if, ext, raw, hole). coverage.test.ts
 *  uses this to skip not-yet-implemented kinds without going red. */
export const PHASE_AB_EXPR_KINDS: readonly Expr["kind"][] = [
  "var",
  "lit",
  "attr",
  "has",
  "binary",
  "unary",
] as const;

/** All BinaryOp values surfaced as a dropdown in `expr_binary`. Kept in the
 *  same order Cedar's spec lists them so the UI is predictable. */
export const BINARY_OPS: readonly BinaryOp[] = [
  "==",
  "!=",
  "<",
  "<=",
  ">",
  ">=",
  "&&",
  "||",
  "+",
  "-",
  "*",
  "in",
  "contains",
  "containsAll",
  "containsAny",
  "getTag",
  "hasTag",
] as const;

/** Unary-op dropdown. `neg` is rendered as `-` in Cedar source. */
export const UNARY_OPS: readonly UnaryOp[] = ["!", "neg", "isEmpty"] as const;

/** Reverse map for irToWorkspace: pick the block type for a given Expr. Returns
 *  null when no Phase-A/B block matches (Phase C / Phase E will broaden this). */
export function blockTypeForExpr(e: Expr): BlockTypeId | null {
  switch (e.kind) {
    case "var":
      return BLOCK_TYPES.expr_var;
    case "lit":
      switch (e.litType) {
        case "bool":
          return BLOCK_TYPES.expr_lit_bool;
        case "long":
          return BLOCK_TYPES.expr_lit_long;
        case "string":
          return BLOCK_TYPES.expr_lit_string;
      }
      return null;
    case "attr":
      return BLOCK_TYPES.expr_attr;
    case "has":
      return BLOCK_TYPES.expr_has;
    case "binary":
      return BLOCK_TYPES.expr_binary;
    case "unary":
      return BLOCK_TYPES.expr_unary;
    default:
      return null; // litEntity / set / record / like / is / if / ext / raw / hole → Phase C+
  }
}
