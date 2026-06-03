/**
 * Expression blocks. Phase A added bool literal; Phase B added the core
 * exprs (var, lit:long/string, attr, has, binary, unary); Phase C completes
 * the Expr surface (litEntity, set, record, like, is, if, ext, raw).
 *
 * Block JSON design notes:
 *   - Variable-arity nodes (set elements, record pairs, ext args) use the
 *     statement-list wrapper pattern (mirrors Cond) instead of Blockly mutators.
 *     This trades a slightly noisier serialised tree for a much simpler
 *     authoring + maintenance story.
 *   - Like patterns are entered as a single string; `*` becomes a Wildcard
 *     token at IR build time (workspaceToIR.ts handles split / join).
 *   - Raw blocks persist their EST payload via Blockly's `data` field
 *     (`block.data = JSON.stringify(est)`). The visible label shows a short
 *     ellipsised excerpt so the user knows what's there.
 */

import { BINARY_OPS, UNARY_OPS } from "../mapping/block-types";

// ── literals ────────────────────────────────────────────────────────────

export const EXPR_LIT_BOOL_BLOCK_JSON = {
  type: "expr_lit_bool",
  message0: "%1",
  args0: [{ type: "field_dropdown", name: "VALUE", options: [["true", "true"], ["false", "false"]] }],
  output: "Expr",
  colour: 160,
  tooltip: "true / false 리터럴",
} as const;

export const EXPR_LIT_LONG_BLOCK_JSON = {
  type: "expr_lit_long",
  message0: "%1",
  args0: [{ type: "field_number", name: "VALUE", value: 0, precision: 1 }],
  output: "Expr",
  colour: 160,
  tooltip: "정수 리터럴 (Cedar `long`)",
} as const;

export const EXPR_LIT_STRING_BLOCK_JSON = {
  type: "expr_lit_string",
  message0: '"%1"',
  args0: [{ type: "field_input", name: "VALUE", text: "" }],
  output: "Expr",
  colour: 160,
  tooltip: "문자열 리터럴",
} as const;

export const EXPR_LIT_ENTITY_BLOCK_JSON = {
  type: "expr_lit_entity",
  message0: '%1 :: "%2"',
  args0: [
    { type: "field_input", name: "TYPE", text: "User" },
    { type: "field_input", name: "ID", text: "alice" },
  ],
  output: "Expr",
  colour: 160,
  tooltip: "엔티티 리터럴 (값으로 사용; 예: User::\"alice\")",
} as const;

// ── request variables ──────────────────────────────────────────────────

export const EXPR_VAR_BLOCK_JSON = {
  type: "expr_var",
  message0: "%1",
  args0: [
    {
      type: "field_dropdown",
      name: "NAME",
      options: [
        ["principal", "principal"],
        ["action", "action"],
        ["resource", "resource"],
        ["context", "context"],
      ],
    },
  ],
  output: "Expr",
  colour: 230,
  tooltip: "요청 변수 (principal / action / resource / context)",
} as const;

// ── attribute access / presence ────────────────────────────────────────

export const EXPR_ATTR_BLOCK_JSON = {
  type: "expr_attr",
  message0: "%1 . %2",
  args0: [
    { type: "input_value", name: "OF", check: "Expr" },
    { type: "field_input", name: "FIELD", text: "field" },
  ],
  output: "Expr",
  inputsInline: true,
  colour: 180,
  tooltip: "필드 접근 (예: principal.role)",
} as const;

export const EXPR_HAS_BLOCK_JSON = {
  type: "expr_has",
  message0: "%1 has %2",
  args0: [
    { type: "input_value", name: "OF", check: "Expr" },
    { type: "field_input", name: "FIELD", text: "field" },
  ],
  output: "Expr",
  inputsInline: true,
  colour: 180,
  tooltip: "필드 존재 여부 (예: context has amount)",
} as const;

// ── binary / unary ──────────────────────────────────────────────────────

const BINARY_OP_LABELS: Record<string, string> = {
  "==": "=",
  "!=": "≠",
  "<": "<",
  "<=": "≤",
  ">": ">",
  ">=": "≥",
  "&&": "and",
  "||": "or",
  "+": "+",
  "-": "−",
  "*": "×",
  in: "in",
  contains: "contains",
  containsAll: "containsAll",
  containsAny: "containsAny",
  getTag: ".getTag",
  hasTag: ".hasTag",
};

export const EXPR_BINARY_BLOCK_JSON = {
  type: "expr_binary",
  message0: "%1 %2 %3",
  args0: [
    { type: "input_value", name: "LEFT", check: "Expr" },
    {
      type: "field_dropdown",
      name: "OP",
      options: BINARY_OPS.map((op) => [BINARY_OP_LABELS[op] ?? op, op]),
    },
    { type: "input_value", name: "RIGHT", check: "Expr" },
  ],
  output: "Expr",
  inputsInline: true,
  colour: 260,
  tooltip: "두 식의 이항 연산 (비교 / 논리 / 산술 / 집합)",
} as const;

const UNARY_OP_LABELS: Record<string, string> = {
  "!": "not",
  neg: "−",
  isEmpty: "isEmpty",
};

export const EXPR_UNARY_BLOCK_JSON = {
  type: "expr_unary",
  message0: "%1 %2",
  args0: [
    {
      type: "field_dropdown",
      name: "OP",
      options: UNARY_OPS.map((op) => [UNARY_OP_LABELS[op] ?? op, op]),
    },
    { type: "input_value", name: "OPERAND", check: "Expr" },
  ],
  output: "Expr",
  inputsInline: true,
  colour: 260,
  tooltip: "단항 연산 (! / − / isEmpty)",
} as const;

// ── collections (set / record) ─────────────────────────────────────────

export const EXPR_SET_BLOCK_JSON = {
  type: "expr_set",
  message0: "[ %1 ]",
  args0: [{ type: "input_statement", name: "ITEMS", check: "SetItem" }],
  output: "Expr",
  colour: 140,
  tooltip: "집합 리터럴 — 원소 블록(•)을 stack 안에 채워 넣으세요",
} as const;

export const EXPR_SET_ITEM_BLOCK_JSON = {
  type: "expr_set_item",
  message0: "• %1",
  args0: [{ type: "input_value", name: "ITEM", check: "Expr" }],
  previousStatement: "SetItem",
  nextStatement: "SetItem",
  colour: 140,
  tooltip: "집합 원소",
} as const;

export const EXPR_RECORD_BLOCK_JSON = {
  type: "expr_record",
  message0: "{ %1 }",
  args0: [{ type: "input_statement", name: "PAIRS", check: "RecordPair" }],
  output: "Expr",
  colour: 140,
  tooltip: "레코드 리터럴 — 키:값 블록을 stack 안에 채워 넣으세요",
} as const;

export const EXPR_RECORD_PAIR_BLOCK_JSON = {
  type: "expr_record_pair",
  message0: "%1 : %2",
  args0: [
    { type: "field_input", name: "KEY", text: "key" },
    { type: "input_value", name: "VALUE", check: "Expr" },
  ],
  previousStatement: "RecordPair",
  nextStatement: "RecordPair",
  inputsInline: true,
  colour: 140,
  tooltip: "레코드의 키:값 쌍",
} as const;

// ── string / type / control ────────────────────────────────────────────

export const EXPR_LIKE_BLOCK_JSON = {
  type: "expr_like",
  message0: '%1 like "%2"',
  args0: [
    { type: "input_value", name: "OF", check: "Expr" },
    { type: "field_input", name: "PATTERN", text: "abc*" },
  ],
  output: "Expr",
  inputsInline: true,
  colour: 180,
  tooltip: "문자열 패턴 매치 (`*`는 와일드카드)",
} as const;

export const EXPR_IS_BLOCK_JSON = {
  type: "expr_is",
  message0: "%1 is %2",
  args0: [
    { type: "input_value", name: "OF", check: "Expr" },
    { type: "field_input", name: "TYPE", text: "User" },
  ],
  message1: "  in %1",
  args1: [{ type: "input_value", name: "IN", check: "Expr" }],
  output: "Expr",
  colour: 180,
  tooltip: "엔티티 타입 확인 (in 슬롯은 옵션)",
} as const;

export const EXPR_IF_BLOCK_JSON = {
  type: "expr_if",
  message0: "if %1",
  args0: [{ type: "input_value", name: "COND", check: "Expr" }],
  message1: "  then %1",
  args1: [{ type: "input_value", name: "THEN", check: "Expr" }],
  message2: "  else %1",
  args2: [{ type: "input_value", name: "ELSE", check: "Expr" }],
  output: "Expr",
  colour: 290,
  tooltip: "조건식 (Cedar `if c then a else b`)",
} as const;

// ── extension function (variable-arity args) ───────────────────────────

export const EXPR_EXT_BLOCK_JSON = {
  type: "expr_ext",
  message0: "%1 ( %2 )",
  args0: [
    { type: "field_input", name: "FN", text: "fn" },
    { type: "input_statement", name: "ARGS", check: "ExtArg" },
  ],
  output: "Expr",
  colour: 50,
  tooltip: "확장 함수 호출 (예: decimal, ip, ...). 인자 블록을 stack 안에 추가",
} as const;

export const EXPR_EXT_ARG_BLOCK_JSON = {
  type: "expr_ext_arg",
  message0: "arg %1",
  args0: [{ type: "input_value", name: "ARG", check: "Expr" }],
  previousStatement: "ExtArg",
  nextStatement: "ExtArg",
  colour: 50,
  tooltip: "확장 함수 인자",
} as const;

// ── escape hatch ───────────────────────────────────────────────────────

/** Raw block — the IR-side `raw` escape hatch. Read-only display of a short
 *  excerpt; the full EST JSON lives in `block.data` (Blockly persists this
 *  alongside the block). workspaceToIR pulls JSON.parse(block.data) into
 *  `{kind:"raw", est:...}`. */
export const EXPR_RAW_BLOCK_JSON = {
  type: "expr_raw",
  message0: "⟨raw⟩ %1",
  args0: [
    {
      // Visible label only; the real payload sits in block.data so users can't
      // accidentally corrupt it via the editor.
      type: "field_label_serializable",
      name: "PREVIEW",
      text: "(empty)",
    },
  ],
  output: "Expr",
  colour: 0,
  tooltip: "Cedar 식이지만 블록으로 매핑 안 됨 — 원본 EST 보존 (편집 불가)",
} as const;
