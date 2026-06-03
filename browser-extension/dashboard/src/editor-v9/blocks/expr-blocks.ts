/**
 * Expression blocks (Phase B: core + bool from Phase A).
 *
 * Each `expr_*` block carries `output: "Expr"` so it plugs into any value slot
 * with `check: "Expr"`. Phase C adds the remaining Expr.kind variants
 * (set / record / litEntity / like / is / if / ext / raw / hole).
 *
 * Block JSON design notes:
 *   - Dropdowns use Blockly's `field_dropdown` with [[label, value], ...] pairs.
 *     For operators, label = symbol, value = the canonical Expr op string.
 *   - Field-name inputs (FIELD on attr/has) use `field_input` with a sensible
 *     default so the block isn't dangling; workspaceToIR pushes a structural
 *     error if the user clears it.
 *   - Colours track category: structural blocks use the policy palette,
 *     expressions use the green/teal family (160-180). Binary/unary share a
 *     distinct hue so the operator chain is visually scannable.
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

// ── request variables (principal / action / resource / context) ─────────

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

/** Operator symbols shown in the dropdown UI; values are the canonical op
 *  strings consumed by PolicyIR. `getTag`/`hasTag` are method-like in Cedar
 *  source but shaped as binary here (matches PolicyIR). */
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
