/**
 * Single source of truth for Blockly block type ↔ PolicyIR node mapping.
 *
 * Two registries are derived from this file:
 *   1. `blocks/register.ts` iterates the JSON exports to register Blockly blocks.
 *   2. `mapping/workspaceToIR.ts` and `mapping/irToWorkspace.ts` switch on the
 *      same ids to convert between Blockly Workspace and PolicyIR.
 *
 * Phase coverage:
 *   A — policy_hat, scope_all, action_scope_all, cond_when, expr_lit_bool.
 *   B — cond_unless, expr_var, expr_lit_long, expr_lit_string, expr_attr,
 *       expr_has, expr_binary, expr_unary.
 *   C — scope variants (scope_eq/in/is/slot, action_scope_eq/in), remaining
 *       Expr.kinds (litEntity, set, record, like, is, if, ext, raw), and the
 *       child-wrapper blocks for variable-arity nodes (set_item, record_pair,
 *       action_scope_in_item, ext_arg).
 *   E — expr_hole (parameterization).
 *
 * Adding a new Expr.kind: append here, add a block JSON in blocks/, add
 * round-trip arms in workspaceToIR.ts / irToWorkspace.ts. coverage.test.ts
 * fails if any of the three is missing.
 */

import type { BinaryOp, Expr, UnaryOp } from "../../cedar/blocks";

/** Blockly value-input connector check kinds. */
export type ConnectorCheck =
  | "Expr"
  | "Scope"
  | "ActionScope"
  | "Cond"
  | "SetItem"
  | "RecordPair"
  | "ActionScopeInItem"
  | "ExtArg";

export const BLOCK_TYPES = {
  // ── policy / scope / condition (structural) ──
  policy_hat: "policy_hat",
  scope_all: "scope_all",
  scope_eq: "scope_eq",
  scope_in: "scope_in",
  scope_is: "scope_is",
  scope_slot: "scope_slot",
  action_scope_all: "action_scope_all",
  action_scope_eq: "action_scope_eq",
  action_scope_in: "action_scope_in",
  action_scope_in_item: "action_scope_in_item",
  cond_when: "cond_when",
  cond_unless: "cond_unless",
  // ── expressions ──
  expr_var: "expr_var",
  expr_lit_bool: "expr_lit_bool",
  expr_lit_long: "expr_lit_long",
  expr_lit_string: "expr_lit_string",
  expr_lit_entity: "expr_lit_entity",
  expr_attr: "expr_attr",
  expr_has: "expr_has",
  expr_binary: "expr_binary",
  expr_unary: "expr_unary",
  expr_set: "expr_set",
  expr_set_item: "expr_set_item",
  expr_record: "expr_record",
  expr_record_pair: "expr_record_pair",
  expr_like: "expr_like",
  expr_is: "expr_is",
  expr_if: "expr_if",
  expr_ext: "expr_ext",
  expr_ext_arg: "expr_ext_arg",
  expr_raw: "expr_raw",
  expr_hole: "expr_hole",
  /** Smart-picker dropdown — single block, dropdown over all gloss paths.
   *  Preset per-path field blocks live under their own `field_<path>` ids
   *  generated from gloss/paths.ts (not enumerated here). */
  expr_field: "expr_field",
} as const;

export type BlockTypeId = (typeof BLOCK_TYPES)[keyof typeof BLOCK_TYPES];

/** Blocks that carry `output: "Expr"` — i.e. plug into any Expr value slot.
 *  Used by the coverage test to assert every value-producing block id is
 *  reachable from some Expr.kind. Wrapper blocks (set_item, record_pair,
 *  action_scope_in_item, ext_arg) are NOT expressions; they're statement-list
 *  children of their parent and excluded here. */
export const EXPR_BLOCK_TYPES: readonly BlockTypeId[] = [
  BLOCK_TYPES.expr_var,
  BLOCK_TYPES.expr_lit_bool,
  BLOCK_TYPES.expr_lit_long,
  BLOCK_TYPES.expr_lit_string,
  BLOCK_TYPES.expr_lit_entity,
  BLOCK_TYPES.expr_attr,
  BLOCK_TYPES.expr_has,
  BLOCK_TYPES.expr_binary,
  BLOCK_TYPES.expr_unary,
  BLOCK_TYPES.expr_set,
  BLOCK_TYPES.expr_record,
  BLOCK_TYPES.expr_like,
  BLOCK_TYPES.expr_is,
  BLOCK_TYPES.expr_if,
  BLOCK_TYPES.expr_ext,
  BLOCK_TYPES.expr_raw,
  BLOCK_TYPES.expr_hole,
] as const;

/** Every Expr.kind that has a corresponding block. Updated as phases land. */
export const ALL_EXPR_KINDS: readonly Expr["kind"][] = [
  "var",
  "lit",
  "litEntity",
  "set",
  "record",
  "attr",
  "has",
  "binary",
  "unary",
  "like",
  "is",
  "if",
  "ext",
  "raw",
  "hole",
] as const;

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

export const UNARY_OPS: readonly UnaryOp[] = ["!", "neg", "isEmpty"] as const;

/** Pick the block type for a given Expr. Returns null for `hole` (Phase E). */
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
    case "litEntity":
      return BLOCK_TYPES.expr_lit_entity;
    case "set":
      return BLOCK_TYPES.expr_set;
    case "record":
      return BLOCK_TYPES.expr_record;
    case "attr":
      return BLOCK_TYPES.expr_attr;
    case "has":
      return BLOCK_TYPES.expr_has;
    case "binary":
      return BLOCK_TYPES.expr_binary;
    case "unary":
      return BLOCK_TYPES.expr_unary;
    case "like":
      return BLOCK_TYPES.expr_like;
    case "is":
      return BLOCK_TYPES.expr_is;
    case "if":
      return BLOCK_TYPES.expr_if;
    case "ext":
      return BLOCK_TYPES.expr_ext;
    case "raw":
      return BLOCK_TYPES.expr_raw;
    case "hole":
      return BLOCK_TYPES.expr_hole;
  }
}
