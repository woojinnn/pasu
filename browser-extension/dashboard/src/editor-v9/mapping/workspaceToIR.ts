/**
 * Blockly Workspace → PolicyIR.
 *
 * Walks all top-level `policy_hat` blocks and produces one PolicyIR each.
 * Empty workspace → empty array.
 *
 * Error policy: structural problems (missing required inputs, unmapped block
 * types) DO NOT throw — they push onto the supplied `errors` array. Callers
 * inspect `errors.length` to decide whether to allow save. blocksToEst will
 * separately throw on semantic violations (unfilled holes) once Phase E lands.
 *
 * Phase A/B coverage: policy_hat / scope_all / action_scope_all /
 * cond_when / cond_unless / expr_var / expr_lit_bool / expr_lit_long /
 * expr_lit_string / expr_attr / expr_has / expr_binary / expr_unary.
 * Unhandled expression blocks fall back to `{kind:"raw", est:null}` (a
 * placeholder; Phase C wires the real raw escape hatch).
 */

import * as Blockly from "blockly";
import type {
  ActionScope,
  BinaryOp,
  Condition,
  Effect,
  Expr,
  PolicyIR,
  Scope,
  UnaryOp,
  VarName,
} from "../../cedar/blocks";
import { BLOCK_TYPES, BINARY_OPS, UNARY_OPS } from "./block-types";
import type { EditorError } from "../errors";

export function workspaceToIR(
  ws: Blockly.Workspace,
  errors: EditorError[],
): PolicyIR[] {
  const policies: PolicyIR[] = [];
  for (const block of ws.getTopBlocks(true)) {
    if (block.type !== BLOCK_TYPES.policy_hat) {
      errors.push({
        kind: "structural",
        message: `최상위에 정책(policy_hat) 블록이 아닌 "${block.type}"이 있습니다`,
        blockId: block.id,
      });
      continue;
    }
    const ir = policyHatToIR(block, errors);
    if (ir) policies.push(ir);
  }
  return policies;
}

function policyHatToIR(
  block: Blockly.Block,
  errors: EditorError[],
): PolicyIR | null {
  const effect = (block.getFieldValue("EFFECT") ?? "permit") as Effect;
  const principal = readScope(block, "PRINCIPAL", errors);
  const action = readActionScope(block, "ACTION", errors);
  const resource = readScope(block, "RESOURCE", errors);
  const conditions = readConditionStatements(block, "CONDITIONS", errors);

  return {
    kind: "policy",
    effect,
    annotations: [],
    scope: { principal, action, resource },
    conditions,
  };
}

function readScope(parent: Blockly.Block, inputName: string, errors: EditorError[]): Scope {
  const child = parent.getInputTargetBlock(inputName);
  if (!child) {
    errors.push({
      kind: "structural",
      message: `${inputName} 슬롯이 비어있습니다`,
      blockId: parent.id,
    });
    return { kind: "scopeAll" };
  }
  switch (child.type) {
    case BLOCK_TYPES.scope_all:
      return { kind: "scopeAll" };
    default:
      errors.push({
        kind: "structural",
        message: `${inputName} 슬롯에 알 수 없는 블록 "${child.type}"`,
        blockId: child.id,
      });
      return { kind: "scopeAll" };
  }
}

function readActionScope(parent: Blockly.Block, inputName: string, errors: EditorError[]): ActionScope {
  const child = parent.getInputTargetBlock(inputName);
  if (!child) {
    errors.push({
      kind: "structural",
      message: `${inputName} 슬롯이 비어있습니다`,
      blockId: parent.id,
    });
    return { kind: "scopeAll" };
  }
  switch (child.type) {
    case BLOCK_TYPES.action_scope_all:
      return { kind: "scopeAll" };
    default:
      errors.push({
        kind: "structural",
        message: `${inputName} 슬롯에 알 수 없는 블록 "${child.type}"`,
        blockId: child.id,
      });
      return { kind: "scopeAll" };
  }
}

function readConditionStatements(
  parent: Blockly.Block,
  inputName: string,
  errors: EditorError[],
): Condition[] {
  const out: Condition[] = [];
  let cur = parent.getInputTargetBlock(inputName);
  while (cur) {
    if (cur.type === BLOCK_TYPES.cond_when || cur.type === BLOCK_TYPES.cond_unless) {
      const body = readExpr(cur, "BODY", errors);
      out.push({
        kind: cur.type === BLOCK_TYPES.cond_when ? "when" : "unless",
        body,
      });
    } else {
      errors.push({
        kind: "structural",
        message: `조건 슬롯에 예상치 못한 블록 "${cur.type}"`,
        blockId: cur.id,
      });
    }
    cur = cur.getNextBlock();
  }
  return out;
}

function readExpr(parent: Blockly.Block, inputName: string, errors: EditorError[]): Expr {
  const child = parent.getInputTargetBlock(inputName);
  if (!child) {
    errors.push({
      kind: "structural",
      message: `식 슬롯 ${inputName} 가 비어있습니다`,
      blockId: parent.id,
    });
    return { kind: "raw", est: null };
  }
  return blockToExpr(child, errors);
}

function blockToExpr(block: Blockly.Block, errors: EditorError[]): Expr {
  switch (block.type) {
    case BLOCK_TYPES.expr_var: {
      const name = (block.getFieldValue("NAME") ?? "principal") as VarName;
      return { kind: "var", name };
    }
    case BLOCK_TYPES.expr_lit_bool: {
      const raw = block.getFieldValue("VALUE") ?? "true";
      return { kind: "lit", litType: "bool", value: raw === "true" };
    }
    case BLOCK_TYPES.expr_lit_long: {
      const raw = block.getFieldValue("VALUE") ?? 0;
      const n = Number(raw);
      if (!Number.isFinite(n) || !Number.isInteger(n)) {
        errors.push({
          kind: "structural",
          message: `정수 리터럴 값이 올바르지 않습니다 (입력: ${raw})`,
          blockId: block.id,
        });
        return { kind: "lit", litType: "long", value: 0 };
      }
      return { kind: "lit", litType: "long", value: n };
    }
    case BLOCK_TYPES.expr_lit_string: {
      const raw = block.getFieldValue("VALUE") ?? "";
      return { kind: "lit", litType: "string", value: String(raw) };
    }
    case BLOCK_TYPES.expr_attr: {
      const of = readExpr(block, "OF", errors);
      const attr = (block.getFieldValue("FIELD") ?? "").trim();
      if (!attr) {
        errors.push({
          kind: "structural",
          message: "필드 이름이 비어있습니다",
          blockId: block.id,
        });
      }
      return { kind: "attr", of, attr };
    }
    case BLOCK_TYPES.expr_has: {
      const of = readExpr(block, "OF", errors);
      const attr = (block.getFieldValue("FIELD") ?? "").trim();
      if (!attr) {
        errors.push({
          kind: "structural",
          message: "has 의 필드 이름이 비어있습니다",
          blockId: block.id,
        });
      }
      return { kind: "has", of, attr };
    }
    case BLOCK_TYPES.expr_binary: {
      const left = readExpr(block, "LEFT", errors);
      const right = readExpr(block, "RIGHT", errors);
      const opRaw = (block.getFieldValue("OP") ?? "==") as BinaryOp;
      const op = (BINARY_OPS as readonly string[]).includes(opRaw) ? opRaw : "==";
      return { kind: "binary", op, left, right };
    }
    case BLOCK_TYPES.expr_unary: {
      const operand = readExpr(block, "OPERAND", errors);
      const opRaw = (block.getFieldValue("OP") ?? "!") as UnaryOp;
      const op = (UNARY_OPS as readonly string[]).includes(opRaw) ? opRaw : "!";
      return { kind: "unary", op, operand };
    }
    default:
      errors.push({
        kind: "structural",
        message: `식 슬롯에 알 수 없는 블록 "${block.type}"`,
        blockId: block.id,
      });
      return { kind: "raw", est: null };
  }
}
