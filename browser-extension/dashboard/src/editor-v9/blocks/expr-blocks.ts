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
 *   - Exported as factories so tooltips resolve through i18n at registration
 *     time, not at module import.
 */

import { i18n } from "../../i18n";
import { BINARY_OPS, UNARY_OPS } from "../mapping/block-types";

// ── literals ────────────────────────────────────────────────────────────

export const EXPR_LIT_BOOL_BLOCK_JSON = () =>
  ({
    type: "expr_lit_bool",
    message0: "%1",
    args0: [{ type: "field_dropdown", name: "VALUE", options: [["true", "true"], ["false", "false"]] }],
    output: "Expr",
    colour: 160,
    tooltip: i18n.t("blocks:block.expr_lit_bool.tooltip"),
  }) as const;

export const EXPR_LIT_LONG_BLOCK_JSON = () =>
  ({
    type: "expr_lit_long",
    message0: "%1",
    args0: [{ type: "field_number", name: "VALUE", value: 0, precision: 1 }],
    output: "Expr",
    colour: 160,
    tooltip: i18n.t("blocks:block.expr_lit_long.tooltip"),
  }) as const;

export const EXPR_LIT_STRING_BLOCK_JSON = () =>
  ({
    type: "expr_lit_string",
    message0: '"%1"',
    args0: [{ type: "field_input", name: "VALUE", text: "" }],
    output: "Expr",
    colour: 160,
    tooltip: i18n.t("blocks:block.expr_lit_string.tooltip"),
  }) as const;

export const EXPR_LIT_ENTITY_BLOCK_JSON = () =>
  ({
    type: "expr_lit_entity",
    message0: '%1 :: "%2"',
    args0: [
      { type: "field_input", name: "TYPE", text: "User" },
      { type: "field_input", name: "ID", text: "alice" },
    ],
    output: "Expr",
    colour: 160,
    tooltip: i18n.t("blocks:block.expr_lit_entity.tooltip"),
  }) as const;

// ── request variables ──────────────────────────────────────────────────

export const EXPR_VAR_BLOCK_JSON = () =>
  ({
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
    tooltip: i18n.t("blocks:block.expr_var.tooltip"),
  }) as const;

// ── attribute access / presence ────────────────────────────────────────

export const EXPR_ATTR_BLOCK_JSON = () =>
  ({
    type: "expr_attr",
    message0: "%1 . %2",
    args0: [
      { type: "input_value", name: "OF", check: "Expr" },
      { type: "field_input", name: "FIELD", text: "field" },
    ],
    output: "Expr",
    inputsInline: true,
    colour: 180,
    tooltip: i18n.t("blocks:block.expr_attr.tooltip"),
  }) as const;

export const EXPR_HAS_BLOCK_JSON = () =>
  ({
    type: "expr_has",
    message0: "%1 has %2",
    args0: [
      { type: "input_value", name: "OF", check: "Expr" },
      { type: "field_input", name: "FIELD", text: "field" },
    ],
    output: "Expr",
    inputsInline: true,
    colour: 180,
    tooltip: i18n.t("blocks:block.expr_has.tooltip"),
  }) as const;

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

export const EXPR_BINARY_BLOCK_JSON = () =>
  ({
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
    tooltip: i18n.t("blocks:block.expr_binary.tooltip"),
  }) as const;

const UNARY_OP_LABELS: Record<string, string> = {
  "!": "not",
  neg: "−",
  isEmpty: "isEmpty",
};

export const EXPR_UNARY_BLOCK_JSON = () =>
  ({
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
    tooltip: i18n.t("blocks:block.expr_unary.tooltip"),
  }) as const;

// ── collections (set / record) ─────────────────────────────────────────

export const EXPR_SET_BLOCK_JSON = () =>
  ({
    type: "expr_set",
    message0: "[ %1 ]",
    args0: [{ type: "input_statement", name: "ITEMS", check: "SetItem" }],
    output: "Expr",
    colour: 140,
    tooltip: i18n.t("blocks:block.expr_set.tooltip"),
  }) as const;

export const EXPR_SET_ITEM_BLOCK_JSON = () =>
  ({
    type: "expr_set_item",
    message0: "• %1",
    args0: [{ type: "input_value", name: "ITEM", check: "Expr" }],
    previousStatement: "SetItem",
    nextStatement: "SetItem",
    colour: 140,
    tooltip: i18n.t("blocks:block.expr_set_item.tooltip"),
  }) as const;

export const EXPR_RECORD_BLOCK_JSON = () =>
  ({
    type: "expr_record",
    message0: "{ %1 }",
    args0: [{ type: "input_statement", name: "PAIRS", check: "RecordPair" }],
    output: "Expr",
    colour: 140,
    tooltip: i18n.t("blocks:block.expr_record.tooltip"),
  }) as const;

export const EXPR_RECORD_PAIR_BLOCK_JSON = () =>
  ({
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
    tooltip: i18n.t("blocks:block.expr_record_pair.tooltip"),
  }) as const;

// ── string / type / control ────────────────────────────────────────────

export const EXPR_LIKE_BLOCK_JSON = () =>
  ({
    type: "expr_like",
    message0: '%1 like "%2"',
    args0: [
      { type: "input_value", name: "OF", check: "Expr" },
      { type: "field_input", name: "PATTERN", text: "abc*" },
    ],
    output: "Expr",
    inputsInline: true,
    colour: 180,
    tooltip: i18n.t("blocks:block.expr_like.tooltip"),
  }) as const;

export const EXPR_IS_BLOCK_JSON = () =>
  ({
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
    tooltip: i18n.t("blocks:block.expr_is.tooltip"),
  }) as const;

export const EXPR_IF_BLOCK_JSON = () =>
  ({
    type: "expr_if",
    message0: "if %1",
    args0: [{ type: "input_value", name: "COND", check: "Expr" }],
    message1: "  then %1",
    args1: [{ type: "input_value", name: "THEN", check: "Expr" }],
    message2: "  else %1",
    args2: [{ type: "input_value", name: "ELSE", check: "Expr" }],
    output: "Expr",
    colour: 290,
    tooltip: i18n.t("blocks:block.expr_if.tooltip"),
  }) as const;

// ── extension function (variable-arity args) ───────────────────────────

export const EXPR_EXT_BLOCK_JSON = () =>
  ({
    type: "expr_ext",
    message0: "%1 ( %2 )",
    args0: [
      { type: "field_input", name: "FN", text: "fn" },
      { type: "input_statement", name: "ARGS", check: "ExtArg" },
    ],
    output: "Expr",
    colour: 50,
    tooltip: i18n.t("blocks:block.expr_ext.tooltip"),
  }) as const;

export const EXPR_EXT_ARG_BLOCK_JSON = () =>
  ({
    type: "expr_ext_arg",
    message0: "arg %1",
    args0: [{ type: "input_value", name: "ARG", check: "Expr" }],
    previousStatement: "ExtArg",
    nextStatement: "ExtArg",
    colour: 50,
    tooltip: i18n.t("blocks:block.expr_ext_arg.tooltip"),
  }) as const;

// ── escape hatch ───────────────────────────────────────────────────────

/** Raw block — the IR-side `raw` escape hatch. Read-only display of a short
 *  excerpt; the full EST JSON lives in `block.data` (Blockly persists this
 *  alongside the block). workspaceToIR pulls JSON.parse(block.data) into
 *  `{kind:"raw", est:...}`. */
export const EXPR_RAW_BLOCK_JSON = () =>
  ({
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
    tooltip: i18n.t("blocks:block.expr_raw.tooltip"),
  }) as const;
