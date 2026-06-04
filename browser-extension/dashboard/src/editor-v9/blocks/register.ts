/**
 * Register all custom Blockly blocks for editor-v9.
 *
 * Idempotent: safe to call multiple times (Workspace re-mount triggers it).
 * Block JSON lives next to this file (one file per category).
 */

import * as Blockly from "blockly";
import { POLICY_BLOCK_JSON } from "./policy-block";
import {
  SCOPE_BLOCK_JSON,
  SCOPE_EQ_BLOCK_JSON,
  SCOPE_IN_BLOCK_JSON,
  SCOPE_IS_BLOCK_JSON,
  SCOPE_SLOT_BLOCK_JSON,
  ACTION_SCOPE_BLOCK_JSON,
  ACTION_SCOPE_EQ_BLOCK_JSON,
  ACTION_SCOPE_IN_BLOCK_JSON,
  ACTION_SCOPE_IN_ITEM_BLOCK_JSON,
} from "./scope-blocks";
import { COND_WHEN_BLOCK_JSON, COND_UNLESS_BLOCK_JSON } from "./condition-blocks";
import { EXPR_HOLE_BLOCK_JSON } from "./hole-block";
import { EXPR_FIELD_BLOCK_JSON, FIELD_BLOCK_JSON_LIST } from "./field-blocks";
import {
  EXPR_LIT_BOOL_BLOCK_JSON,
  EXPR_LIT_LONG_BLOCK_JSON,
  EXPR_LIT_STRING_BLOCK_JSON,
  EXPR_LIT_ENTITY_BLOCK_JSON,
  EXPR_VAR_BLOCK_JSON,
  EXPR_ATTR_BLOCK_JSON,
  EXPR_HAS_BLOCK_JSON,
  EXPR_BINARY_BLOCK_JSON,
  EXPR_UNARY_BLOCK_JSON,
  EXPR_SET_BLOCK_JSON,
  EXPR_SET_ITEM_BLOCK_JSON,
  EXPR_RECORD_BLOCK_JSON,
  EXPR_RECORD_PAIR_BLOCK_JSON,
  EXPR_LIKE_BLOCK_JSON,
  EXPR_IS_BLOCK_JSON,
  EXPR_IF_BLOCK_JSON,
  EXPR_EXT_BLOCK_JSON,
  EXPR_EXT_ARG_BLOCK_JSON,
  EXPR_RAW_BLOCK_JSON,
} from "./expr-blocks";

const ALL_BLOCK_JSON = [
  POLICY_BLOCK_JSON,
  // scope
  SCOPE_BLOCK_JSON,
  SCOPE_EQ_BLOCK_JSON,
  SCOPE_IN_BLOCK_JSON,
  SCOPE_IS_BLOCK_JSON,
  SCOPE_SLOT_BLOCK_JSON,
  ACTION_SCOPE_BLOCK_JSON,
  ACTION_SCOPE_EQ_BLOCK_JSON,
  ACTION_SCOPE_IN_BLOCK_JSON,
  ACTION_SCOPE_IN_ITEM_BLOCK_JSON,
  // conditions
  COND_WHEN_BLOCK_JSON,
  COND_UNLESS_BLOCK_JSON,
  // expressions
  EXPR_VAR_BLOCK_JSON,
  EXPR_LIT_BOOL_BLOCK_JSON,
  EXPR_LIT_LONG_BLOCK_JSON,
  EXPR_LIT_STRING_BLOCK_JSON,
  EXPR_LIT_ENTITY_BLOCK_JSON,
  EXPR_ATTR_BLOCK_JSON,
  EXPR_HAS_BLOCK_JSON,
  EXPR_BINARY_BLOCK_JSON,
  EXPR_UNARY_BLOCK_JSON,
  EXPR_SET_BLOCK_JSON,
  EXPR_SET_ITEM_BLOCK_JSON,
  EXPR_RECORD_BLOCK_JSON,
  EXPR_RECORD_PAIR_BLOCK_JSON,
  EXPR_LIKE_BLOCK_JSON,
  EXPR_IS_BLOCK_JSON,
  EXPR_IF_BLOCK_JSON,
  EXPR_EXT_BLOCK_JSON,
  EXPR_EXT_ARG_BLOCK_JSON,
  EXPR_RAW_BLOCK_JSON,
  EXPR_HOLE_BLOCK_JSON,
  EXPR_FIELD_BLOCK_JSON,
  // ...generated preset field blocks (40 entries) — see field-blocks.ts.
  ...FIELD_BLOCK_JSON_LIST,
] as const;

let registered = false;

export function registerBlocks(): void {
  if (registered) return;
  Blockly.defineBlocksWithJsonArray([...ALL_BLOCK_JSON]);
  registered = true;
}

/** Test helper — drop the latch so vitest cases register fresh. */
export function __resetBlockRegistrationForTest(): void {
  registered = false;
}
