/**
 * Blockly toolbox builder — categories of draggable blocks.
 *
 * Returns the JSON shape Blockly expects (`Blockly.utils.toolbox.ToolboxDefinition`).
 * Locale-aware (ko/en) for category labels.
 *
 * Phase A categories: 정책 / 범위 / 조건 / 식. Phase B fleshes out 조건 (unless),
 * 식 (var, lit, attr, has, binary, unary). Phase C+ adds 집합/레코드 sub-cats
 * and the 파라미터 category for hole blocks.
 */

import { BLOCK_TYPES } from "../mapping/block-types";

const STRINGS = {
  ko: { policy: "정책", scope: "범위", cond: "조건", expr: "식", ops: "연산" },
  en: { policy: "Policy", scope: "Scope", cond: "Condition", expr: "Expression", ops: "Ops" },
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
          { kind: "block", type: BLOCK_TYPES.action_scope_all },
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
          { kind: "block", type: BLOCK_TYPES.expr_attr },
          { kind: "block", type: BLOCK_TYPES.expr_has },
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
    ],
  };
}
