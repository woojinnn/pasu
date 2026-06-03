/**
 * PolicyIR → Blockly Workspace.
 *
 * Used by:
 *   - irToWorkspace(ws, [policy]) on initial mount when a serialized workspace
 *     is unavailable (e.g. legacy policies seeded only from Cedar text);
 *   - the textToBlocks path in Phase D (paste Cedar → blocks).
 *
 * Clears the workspace first, then materializes each PolicyIR. Block positions
 * are left to Blockly's auto-layout; callers can centerOnBlock afterwards.
 *
 * Coverage: Phase A/B Expr.kinds (var, lit:{bool,long,string}, attr, has,
 * binary, unary) render as real blocks. Other kinds fall back to a placeholder
 * `expr_lit_bool(true)` block — that's a lossy render and will trip the
 * round-trip test once Phase C wires the remaining renderers.
 */

import * as Blockly from "blockly";
import type {
  ActionScope,
  Condition,
  Expr,
  PolicyIR,
  Scope,
} from "../../cedar/blocks";
import { BLOCK_TYPES, blockTypeForExpr } from "./block-types";

export function irToWorkspace(ws: Blockly.WorkspaceSvg, policies: PolicyIR[]): void {
  ws.clear();
  let yCursor = 30;
  for (const policy of policies) {
    const hat = createPolicyHat(ws, policy);
    hat.moveBy(50, yCursor);
    yCursor += 400;
  }
}

function createPolicyHat(ws: Blockly.WorkspaceSvg, policy: PolicyIR): Blockly.BlockSvg {
  const hat = ws.newBlock(BLOCK_TYPES.policy_hat) as Blockly.BlockSvg;
  hat.setFieldValue(policy.effect, "EFFECT");
  attachScope(ws, hat, "PRINCIPAL", policy.scope.principal);
  attachActionScope(ws, hat, "ACTION", policy.scope.action);
  attachScope(ws, hat, "RESOURCE", policy.scope.resource);
  attachConditions(ws, hat, "CONDITIONS", policy.conditions);
  hat.initSvg();
  hat.render();
  return hat;
}

function attachScope(
  ws: Blockly.WorkspaceSvg,
  parent: Blockly.BlockSvg,
  inputName: string,
  scope: Scope,
): void {
  // Phase A/B: only scopeAll is renderable. Other Scope variants
  // (scopeEq/scopeIn/scopeIs/slot) fall back to scope_all — Phase C adds them.
  const child = ws.newBlock(BLOCK_TYPES.scope_all) as Blockly.BlockSvg;
  child.initSvg();
  child.render();
  void scope;
  parent.getInput(inputName)?.connection?.connect(child.outputConnection);
}

function attachActionScope(
  ws: Blockly.WorkspaceSvg,
  parent: Blockly.BlockSvg,
  inputName: string,
  scope: ActionScope,
): void {
  const child = ws.newBlock(BLOCK_TYPES.action_scope_all) as Blockly.BlockSvg;
  child.initSvg();
  child.render();
  void scope;
  parent.getInput(inputName)?.connection?.connect(child.outputConnection);
}

function attachConditions(
  ws: Blockly.WorkspaceSvg,
  parent: Blockly.BlockSvg,
  inputName: string,
  conditions: Condition[],
): void {
  let prev: Blockly.BlockSvg | null = null;
  for (const cond of conditions) {
    const blockType =
      cond.kind === "when" ? BLOCK_TYPES.cond_when : BLOCK_TYPES.cond_unless;
    const block = ws.newBlock(blockType) as Blockly.BlockSvg;
    attachExpr(ws, block, "BODY", cond.body);
    block.initSvg();
    block.render();
    if (prev === null) {
      parent.getInput(inputName)?.connection?.connect(block.previousConnection);
    } else {
      prev.nextConnection?.connect(block.previousConnection);
    }
    prev = block;
  }
}

function attachExpr(
  ws: Blockly.WorkspaceSvg,
  parent: Blockly.BlockSvg,
  inputName: string,
  expr: Expr,
): void {
  const child = createExprBlock(ws, expr);
  if (!child) return;
  child.initSvg();
  child.render();
  parent.getInput(inputName)?.connection?.connect(child.outputConnection);
}

function createExprBlock(ws: Blockly.WorkspaceSvg, expr: Expr): Blockly.BlockSvg | null {
  const blockType = blockTypeForExpr(expr);
  if (!blockType) return placeholderBool(ws);

  const block = ws.newBlock(blockType) as Blockly.BlockSvg;
  switch (expr.kind) {
    case "var":
      block.setFieldValue(expr.name, "NAME");
      break;
    case "lit":
      if (expr.litType === "bool") {
        block.setFieldValue(expr.value ? "true" : "false", "VALUE");
      } else if (expr.litType === "long") {
        block.setFieldValue(String(expr.value), "VALUE");
      } else {
        block.setFieldValue(String(expr.value), "VALUE");
      }
      break;
    case "attr":
      attachExpr(ws, block, "OF", expr.of);
      block.setFieldValue(expr.attr, "FIELD");
      break;
    case "has":
      attachExpr(ws, block, "OF", expr.of);
      block.setFieldValue(expr.attr, "FIELD");
      break;
    case "binary":
      attachExpr(ws, block, "LEFT", expr.left);
      attachExpr(ws, block, "RIGHT", expr.right);
      block.setFieldValue(expr.op, "OP");
      break;
    case "unary":
      attachExpr(ws, block, "OPERAND", expr.operand);
      block.setFieldValue(expr.op, "OP");
      break;
    default:
      // Should not reach — blockTypeForExpr returned non-null only for the
      // kinds enumerated above. Falls through to placeholder below.
      block.dispose(false);
      return placeholderBool(ws);
  }
  return block;
}

function placeholderBool(ws: Blockly.WorkspaceSvg): Blockly.BlockSvg {
  const b = ws.newBlock(BLOCK_TYPES.expr_lit_bool) as Blockly.BlockSvg;
  b.setFieldValue("true", "VALUE");
  return b;
}
