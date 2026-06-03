/**
 * Register all custom Blockly blocks for editor-v9.
 *
 * Idempotent: safe to call multiple times (Workspace re-mount triggers it).
 * Block JSON lives next to this file (one file per category).
 */

import * as Blockly from "blockly";
import { POLICY_BLOCK_JSON } from "./policy-block";
import { SCOPE_BLOCK_JSON, ACTION_SCOPE_BLOCK_JSON } from "./scope-blocks";
import { COND_WHEN_BLOCK_JSON, COND_UNLESS_BLOCK_JSON } from "./condition-blocks";
import {
  EXPR_LIT_BOOL_BLOCK_JSON,
  EXPR_LIT_LONG_BLOCK_JSON,
  EXPR_LIT_STRING_BLOCK_JSON,
  EXPR_VAR_BLOCK_JSON,
  EXPR_ATTR_BLOCK_JSON,
  EXPR_HAS_BLOCK_JSON,
  EXPR_BINARY_BLOCK_JSON,
  EXPR_UNARY_BLOCK_JSON,
} from "./expr-blocks";

const ALL_BLOCK_JSON = [
  POLICY_BLOCK_JSON,
  SCOPE_BLOCK_JSON,
  ACTION_SCOPE_BLOCK_JSON,
  COND_WHEN_BLOCK_JSON,
  COND_UNLESS_BLOCK_JSON,
  EXPR_VAR_BLOCK_JSON,
  EXPR_LIT_BOOL_BLOCK_JSON,
  EXPR_LIT_LONG_BLOCK_JSON,
  EXPR_LIT_STRING_BLOCK_JSON,
  EXPR_ATTR_BLOCK_JSON,
  EXPR_HAS_BLOCK_JSON,
  EXPR_BINARY_BLOCK_JSON,
  EXPR_UNARY_BLOCK_JSON,
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
