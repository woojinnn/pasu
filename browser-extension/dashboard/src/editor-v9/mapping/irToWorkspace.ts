/**
 * PolicyIR → Blockly Workspace.
 *
 * Phase C completes Expr coverage; every Expr.kind except `hole` (Phase E)
 * now renders as a real block instead of the bool placeholder. Scope variants
 * (scope_eq/in/is/slot, action_scope_eq/in) also materialise.
 *
 * Roundtrip contract (verified by tests in Phase D once textToBlocks lands):
 *   workspaceToIR(irToWorkspace(ws, ir)) ≡ ir
 *   (up to schema annotations on `attr`, which are display-only and dropped
 *   on the workspace side.)
 */

import * as Blockly from "blockly";
import type {
  ActionScope,
  Condition,
  EntityRef,
  Expr,
  LikePattern,
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

// ── scope ───────────────────────────────────────────────────────────────

function attachScope(
  ws: Blockly.WorkspaceSvg,
  parent: Blockly.BlockSvg,
  inputName: string,
  scope: Scope,
): void {
  const child = createScopeBlock(ws, scope);
  child.initSvg();
  child.render();
  parent.getInput(inputName)?.connection?.connect(child.outputConnection);
}

function createScopeBlock(ws: Blockly.WorkspaceSvg, scope: Scope): Blockly.BlockSvg {
  switch (scope.kind) {
    case "scopeAll":
      return ws.newBlock(BLOCK_TYPES.scope_all) as Blockly.BlockSvg;
    case "scopeEq": {
      const b = ws.newBlock(BLOCK_TYPES.scope_eq) as Blockly.BlockSvg;
      writeEntityFields(b, scope.entity);
      return b;
    }
    case "scopeIn": {
      const b = ws.newBlock(BLOCK_TYPES.scope_in) as Blockly.BlockSvg;
      writeEntityFields(b, scope.entity);
      return b;
    }
    case "scopeIs": {
      const b = ws.newBlock(BLOCK_TYPES.scope_is) as Blockly.BlockSvg;
      b.setFieldValue(scope.entityType, "TYPE");
      b.setFieldValue(scope.in?.type ?? "", "IN_TYPE");
      b.setFieldValue(scope.in?.id ?? "", "IN_ID");
      return b;
    }
    case "slot": {
      const b = ws.newBlock(BLOCK_TYPES.scope_slot) as Blockly.BlockSvg;
      b.setFieldValue(scope.slot, "SLOT");
      return b;
    }
  }
}

function attachActionScope(
  ws: Blockly.WorkspaceSvg,
  parent: Blockly.BlockSvg,
  inputName: string,
  scope: ActionScope,
): void {
  const child = createActionScopeBlock(ws, scope);
  child.initSvg();
  child.render();
  parent.getInput(inputName)?.connection?.connect(child.outputConnection);
}

function createActionScopeBlock(ws: Blockly.WorkspaceSvg, scope: ActionScope): Blockly.BlockSvg {
  switch (scope.kind) {
    case "scopeAll":
      return ws.newBlock(BLOCK_TYPES.action_scope_all) as Blockly.BlockSvg;
    case "scopeEq": {
      const b = ws.newBlock(BLOCK_TYPES.action_scope_eq) as Blockly.BlockSvg;
      b.setFieldValue(scope.entity.id, "ID");
      return b;
    }
    case "scopeIn": {
      const b = ws.newBlock(BLOCK_TYPES.action_scope_in) as Blockly.BlockSvg;
      let prev: Blockly.BlockSvg | null = null;
      for (const e of scope.entities) {
        const item = ws.newBlock(BLOCK_TYPES.action_scope_in_item) as Blockly.BlockSvg;
        writeEntityFields(item, e);
        item.initSvg();
        item.render();
        if (prev === null) {
          b.getInput("ITEMS")?.connection?.connect(item.previousConnection);
        } else {
          prev.nextConnection?.connect(item.previousConnection);
        }
        prev = item;
      }
      return b;
    }
  }
}

function writeEntityFields(block: Blockly.BlockSvg, e: EntityRef): void {
  block.setFieldValue(e.type, "TYPE");
  block.setFieldValue(e.id, "ID");
}

// ── conditions ──────────────────────────────────────────────────────────

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

// ── expressions ─────────────────────────────────────────────────────────

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
  if (!blockType) {
    // Only `hole` should land here (Phase E). Fall back to placeholder until
    // the hole block is registered.
    return placeholderBool(ws);
  }

  const block = ws.newBlock(blockType) as Blockly.BlockSvg;
  switch (expr.kind) {
    case "var":
      block.setFieldValue(expr.name, "NAME");
      break;
    case "lit":
      if (expr.litType === "bool") {
        block.setFieldValue(expr.value ? "true" : "false", "VALUE");
      } else {
        block.setFieldValue(String(expr.value), "VALUE");
      }
      break;
    case "litEntity":
      writeEntityFields(block, expr.entity);
      break;
    case "set": {
      let prev: Blockly.BlockSvg | null = null;
      for (const el of expr.elements) {
        const item = ws.newBlock(BLOCK_TYPES.expr_set_item) as Blockly.BlockSvg;
        attachExpr(ws, item, "ITEM", el);
        item.initSvg();
        item.render();
        if (prev === null) {
          block.getInput("ITEMS")?.connection?.connect(item.previousConnection);
        } else {
          prev.nextConnection?.connect(item.previousConnection);
        }
        prev = item;
      }
      break;
    }
    case "record": {
      let prev: Blockly.BlockSvg | null = null;
      for (const p of expr.pairs) {
        const item = ws.newBlock(BLOCK_TYPES.expr_record_pair) as Blockly.BlockSvg;
        item.setFieldValue(p.key, "KEY");
        attachExpr(ws, item, "VALUE", p.value);
        item.initSvg();
        item.render();
        if (prev === null) {
          block.getInput("PAIRS")?.connection?.connect(item.previousConnection);
        } else {
          prev.nextConnection?.connect(item.previousConnection);
        }
        prev = item;
      }
      break;
    }
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
    case "like":
      attachExpr(ws, block, "OF", expr.of);
      block.setFieldValue(serializeLikePattern(expr.pattern), "PATTERN");
      break;
    case "is":
      attachExpr(ws, block, "OF", expr.of);
      block.setFieldValue(expr.entityType, "TYPE");
      if (expr.in) attachExpr(ws, block, "IN", expr.in);
      break;
    case "if":
      attachExpr(ws, block, "COND", expr.cond);
      attachExpr(ws, block, "THEN", expr.then);
      attachExpr(ws, block, "ELSE", expr.else);
      break;
    case "ext": {
      block.setFieldValue(expr.fn, "FN");
      let prev: Blockly.BlockSvg | null = null;
      for (const a of expr.args) {
        const arg = ws.newBlock(BLOCK_TYPES.expr_ext_arg) as Blockly.BlockSvg;
        attachExpr(ws, arg, "ARG", a);
        arg.initSvg();
        arg.render();
        if (prev === null) {
          block.getInput("ARGS")?.connection?.connect(arg.previousConnection);
        } else {
          prev.nextConnection?.connect(arg.previousConnection);
        }
        prev = arg;
      }
      break;
    }
    case "raw": {
      // Stash the EST payload on block.data so workspaceToIR can recover it
      // verbatim. Visible label shows a short excerpt.
      const json = JSON.stringify(expr.est);
      (block as unknown as { data: string }).data = json;
      const excerpt =
        json.length > 32 ? `${json.slice(0, 30)}…` : json;
      block.setFieldValue(excerpt, "PREVIEW");
      break;
    }
    case "hole": {
      block.setFieldValue(expr.name, "NAME");
      block.setFieldValue(expr.label ?? "", "LABEL");
      block.setFieldValue(expr.type ?? "", "TYPE");
      // Stash expected / default / optional / constraints on block.data so the
      // round-trip preserves typing + defaults that can't fit in visible fields.
      const payload = {
        expected: expr.expected,
        default: expr.default,
        ...(expr.optional ? { optional: true } : {}),
        ...(expr.constraints ? { constraints: expr.constraints } : {}),
      };
      (block as unknown as { data: string }).data = JSON.stringify(payload);
      break;
    }
  }
  return block;
}

function placeholderBool(ws: Blockly.WorkspaceSvg): Blockly.BlockSvg {
  const b = ws.newBlock(BLOCK_TYPES.expr_lit_bool) as Blockly.BlockSvg;
  b.setFieldValue("true", "VALUE");
  return b;
}

/** Inverse of parseLikePattern in workspaceToIR. Token sequence → display
 *  string with `*` for each Wildcard. */
function serializeLikePattern(pattern: LikePattern): string {
  return pattern
    .map((tok) => (tok === "Wildcard" ? "*" : tok.Literal))
    .join("");
}
