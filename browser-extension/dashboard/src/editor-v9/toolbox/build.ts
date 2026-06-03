/**
 * Blockly toolbox builder — categories of draggable blocks.
 *
 * Returns the JSON shape Blockly expects (`Blockly.utils.toolbox.ToolboxDefinition`).
 * Locale-aware (ko/en) for category labels.
 *
 * Layout philosophy:
 *   - Top: 정책 + 범위 + 조건 (structural — every policy needs these).
 *   - Middle: domain-aware "필드" categories generated from gloss/paths.ts
 *     (주소 / 토큰·베뉴 / 금액·수량 / 방향·주문 / 인증·포지션 / 파생값). One block
 *     per gloss entry; user sees `입력 토큰 주소` instead of building
 *     attr(attr(attr(var(context), ...))) by hand. Plus `필드 (전체)` — a
 *     single smart-picker block whose dropdown lists every gloss entry.
 *   - Lower: 식 (raw expressions for the long tail) / 집합·레코드 / 연산 /
 *     확장·Raw / 파라미터. These stay for power users and unusual policies.
 */

import { BLOCK_TYPES } from "../mapping/block-types";
import {
  blockTypeForPath,
  glossByRole,
  ROLE_LABEL_KO,
  ROLE_LABEL_EN,
  ROLE_COLOUR,
  type GlossEntry,
  type Role,
} from "../gloss";

const STRINGS = {
  ko: {
    policy: "정책",
    scope: "범위",
    cond: "조건",
    fieldPicker: "필드 (전체)",
    expr: "식 (직접 만들기)",
    collection: "집합/레코드",
    ops: "연산",
    ext: "확장 / Raw",
    params: "파라미터",
  },
  en: {
    policy: "Policy",
    scope: "Scope",
    cond: "Condition",
    fieldPicker: "Field (all)",
    expr: "Expression (raw)",
    collection: "Set / Record",
    ops: "Ops",
    ext: "Ext / Raw",
    params: "Parameters",
  },
} as const;

function fieldCategory(role: Role, entries: GlossEntry[], locale: "ko" | "en"): object {
  return {
    kind: "category",
    name: locale === "ko" ? ROLE_LABEL_KO[role] : ROLE_LABEL_EN[role],
    colour: String(ROLE_COLOUR[role]),
    contents: entries.map((e) => ({
      kind: "block",
      type: blockTypeForPath(e.path),
    })),
  };
}

export function buildToolbox(locale: "ko" | "en" = "ko"): object {
  const s = STRINGS[locale];
  const byRole = glossByRole();

  return {
    kind: "categoryToolbox",
    contents: [
      // ── structural (top of the toolbox — every policy needs these) ──
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

      // ── domain-aware field categories (gloss-driven) ──
      fieldCategory("address", byRole.address, locale),
      fieldCategory("ref", byRole.ref, locale),
      fieldCategory("numeric", byRole.numeric, locale),
      fieldCategory("enum", byRole.enum, locale),
      fieldCategory("auth", byRole.auth, locale),
      fieldCategory("derived", byRole.derived, locale),
      {
        kind: "category",
        name: s.fieldPicker,
        colour: "220",
        contents: [{ kind: "block", type: BLOCK_TYPES.expr_field }],
      },

      // ── operations (lifted high — used everywhere) ──
      {
        kind: "category",
        name: s.ops,
        colour: "260",
        contents: [
          { kind: "block", type: BLOCK_TYPES.expr_binary },
          { kind: "block", type: BLOCK_TYPES.expr_unary },
        ],
      },

      // ── raw expression builders (long tail / power users) ──
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
