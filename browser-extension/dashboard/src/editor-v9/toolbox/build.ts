/**
 * Blockly toolbox builder — categories of draggable blocks.
 *
 * Returns the JSON shape Blockly expects (`Blockly.utils.toolbox.ToolboxDefinition`).
 * Locale-aware (ko/en) for category labels.
 *
 * Wrapper blocks (set_item, record_pair, action_scope_in_item, ext_arg) ARE
 * surfaced in the toolbox alongside their parents so users can drag them
 * directly — Blockly's connection-check prevents misuse (only the right
 * parent slot will accept them).
 */

import { BLOCK_TYPES } from "../mapping/block-types";

const STRINGS = {
  ko: {
    policy: "정책",
    scope: "범위",
    cond: "조건",
    expr: "식",
    collection: "집합/레코드",
    ops: "연산",
    ext: "확장 / Raw",
    params: "파라미터",
  },
  en: {
    policy: "Policy",
    scope: "Scope",
    cond: "Condition",
    expr: "Expression",
    collection: "Set / Record",
    ops: "Ops",
    ext: "Ext / Raw",
    params: "Parameters",
  },
} as const;

export function buildToolbox(locale: "ko" | "en" = "ko"): object {
  const s = STRINGS[locale];
  return {
    kind: "categoryToolbox",
    contents: [
      {
        kind: "category",
        name: s.policy,
        colour: "230",
        contents: [{ kind: "block", type: BLOCK_TYPES.policy_hat }],
      },
      {
        kind: "category",
        name: s.scope,
        colour: "200",
        contents: [
          { kind: "block", type: BLOCK_TYPES.scope_all },
          { kind: "block", type: BLOCK_TYPES.scope_eq },
          { kind: "block", type: BLOCK_TYPES.scope_in },
          { kind: "block", type: BLOCK_TYPES.scope_is },
          { kind: "block", type: BLOCK_TYPES.scope_slot },
          { kind: "block", type: BLOCK_TYPES.action_scope_all },
          { kind: "block", type: BLOCK_TYPES.action_scope_eq },
          { kind: "block", type: BLOCK_TYPES.action_scope_in },
          { kind: "block", type: BLOCK_TYPES.action_scope_in_item },
        ],
      },
      {
        kind: "category",
        name: s.cond,
        colour: "290",
        contents: [
          { kind: "block", type: BLOCK_TYPES.cond_when },
          { kind: "block", type: BLOCK_TYPES.cond_unless },
        ],
      },
      {
        kind: "category",
        name: s.expr,
        colour: "160",
        contents: [
          { kind: "block", type: BLOCK_TYPES.expr_var },
          { kind: "block", type: BLOCK_TYPES.expr_lit_bool },
          { kind: "block", type: BLOCK_TYPES.expr_lit_long },
          { kind: "block", type: BLOCK_TYPES.expr_lit_string },
          { kind: "block", type: BLOCK_TYPES.expr_lit_entity },
          { kind: "block", type: BLOCK_TYPES.expr_attr },
          { kind: "block", type: BLOCK_TYPES.expr_has },
          { kind: "block", type: BLOCK_TYPES.expr_like },
          { kind: "block", type: BLOCK_TYPES.expr_is },
          { kind: "block", type: BLOCK_TYPES.expr_if },
        ],
      },
      {
        kind: "category",
        name: s.collection,
        colour: "140",
        contents: [
          { kind: "block", type: BLOCK_TYPES.expr_set },
          { kind: "block", type: BLOCK_TYPES.expr_set_item },
          { kind: "block", type: BLOCK_TYPES.expr_record },
          { kind: "block", type: BLOCK_TYPES.expr_record_pair },
        ],
      },
      {
        kind: "category",
        name: s.ops,
        colour: "260",
        contents: [
          { kind: "block", type: BLOCK_TYPES.expr_binary },
          { kind: "block", type: BLOCK_TYPES.expr_unary },
        ],
      },
      {
        kind: "category",
        name: s.ext,
        colour: "50",
        contents: [
          { kind: "block", type: BLOCK_TYPES.expr_ext },
          { kind: "block", type: BLOCK_TYPES.expr_ext_arg },
          { kind: "block", type: BLOCK_TYPES.expr_raw },
        ],
      },
      {
        kind: "category",
        name: s.params,
        colour: "320",
        contents: [{ kind: "block", type: BLOCK_TYPES.expr_hole }],
      },
    ],
  };
}
