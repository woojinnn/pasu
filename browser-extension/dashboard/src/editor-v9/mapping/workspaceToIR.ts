/**
 * Blockly Workspace → PolicyIR.
 *
 * Walks all top-level `policy_hat` blocks; one PolicyIR per hat. Empty
 * workspace → empty array. Structural problems push onto `errors[]` instead
 * of throwing, so callers can still receive a partial IR and surface the
 * error list in the UI.
 *
 * Coverage (Phases A/B/C):
 *   Scope: scope_all / scope_eq / scope_in / scope_is / scope_slot
 *   ActionScope: action_scope_all / action_scope_eq / action_scope_in
 *   Cond: cond_when / cond_unless
 *   Expr: var, lit{bool,long,string}, litEntity, set, record, attr, has,
 *         binary, unary, like, is, if, ext, raw
 *
 * Things to revisit alongside Phase E:
 *   - `hole` blocks (parameterization) — currently fall into the unknown-block
 *     arm.
 *   - schema-aware annotations (`attr.source/type`) — left null here; the
 *     read path (textToBlocks in Phase D) will hydrate them via descriptor.
 */

import * as Blockly from "blockly";
import type {
  ActionScope,
  BinaryOp,
  Condition,
  Effect,
  EntityRef,
  Expr,
  LikePattern,
  PolicyIR,
  Scope,
  Slot,
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

// ── scope ───────────────────────────────────────────────────────────────

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
    case BLOCK_TYPES.scope_eq:
      return { kind: "scopeEq", entity: readEntityRef(child, errors) };
    case BLOCK_TYPES.scope_in:
      return { kind: "scopeIn", entity: readEntityRef(child, errors) };
    case BLOCK_TYPES.scope_is: {
      const entityType = (child.getFieldValue("TYPE") ?? "").trim();
      if (!entityType) {
        errors.push({
          kind: "structural",
          message: "is 블록의 Type이 비어있습니다",
          blockId: child.id,
        });
      }
      const inType = (child.getFieldValue("IN_TYPE") ?? "").trim();
      const inId = (child.getFieldValue("IN_ID") ?? "").trim();
      // Both fields blank → no qualifier; both filled → qualifier. Half-filled
      // is a likely typo (better surfaced loudly than swallowed silently).
      if (inType && inId) {
        return { kind: "scopeIs", entityType, in: { type: inType, id: inId } };
      }
      if (inType || inId) {
        errors.push({
          kind: "structural",
          message: "is 블록의 in 절: TYPE / ID 를 모두 채우거나 모두 비우세요",
          blockId: child.id,
        });
      }
      return { kind: "scopeIs", entityType };
    }
    case BLOCK_TYPES.scope_slot: {
      const slotRaw = (child.getFieldValue("SLOT") ?? "?principal") as Slot;
      const slot: Slot = slotRaw === "?resource" ? "?resource" : "?principal";
      return { kind: "slot", slot };
    }
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
    case BLOCK_TYPES.action_scope_eq: {
      const id = (child.getFieldValue("ID") ?? "").trim();
      if (!id) {
        errors.push({
          kind: "structural",
          message: "action == 의 Action id 가 비어있습니다",
          blockId: child.id,
        });
      }
      return { kind: "scopeEq", entity: { type: "Action", id } };
    }
    case BLOCK_TYPES.action_scope_in: {
      const entities: EntityRef[] = [];
      let cur = child.getInputTargetBlock("ITEMS");
      while (cur) {
        if (cur.type === BLOCK_TYPES.action_scope_in_item) {
          entities.push(readEntityRef(cur, errors));
        } else {
          errors.push({
            kind: "structural",
            message: `action_scope_in 목록에 예상치 못한 블록 "${cur.type}"`,
            blockId: cur.id,
          });
        }
        cur = cur.getNextBlock();
      }
      return { kind: "scopeIn", entities };
    }
    default:
      errors.push({
        kind: "structural",
        message: `${inputName} 슬롯에 알 수 없는 블록 "${child.type}"`,
        blockId: child.id,
      });
      return { kind: "scopeAll" };
  }
}

function readEntityRef(block: Blockly.Block, errors: EditorError[]): EntityRef {
  const type = (block.getFieldValue("TYPE") ?? "").trim();
  const id = (block.getFieldValue("ID") ?? "").trim();
  if (!type) {
    errors.push({ kind: "structural", message: "엔티티 Type이 비어있습니다", blockId: block.id });
  }
  if (!id) {
    errors.push({ kind: "structural", message: "엔티티 id 가 비어있습니다", blockId: block.id });
  }
  return { type, id };
}

// ── conditions ──────────────────────────────────────────────────────────

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

// ── expressions ─────────────────────────────────────────────────────────

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

function readOptionalExpr(parent: Blockly.Block, inputName: string, errors: EditorError[]): Expr | null {
  const child = parent.getInputTargetBlock(inputName);
  if (!child) return null;
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
    case BLOCK_TYPES.expr_lit_entity: {
      return { kind: "litEntity", entity: readEntityRef(block, errors) };
    }
    case BLOCK_TYPES.expr_attr: {
      const of = readExpr(block, "OF", errors);
      const attr = (block.getFieldValue("FIELD") ?? "").trim();
      if (!attr) {
        errors.push({ kind: "structural", message: "필드 이름이 비어있습니다", blockId: block.id });
      }
      return { kind: "attr", of, attr };
    }
    case BLOCK_TYPES.expr_has: {
      const of = readExpr(block, "OF", errors);
      const attr = (block.getFieldValue("FIELD") ?? "").trim();
      if (!attr) {
        errors.push({ kind: "structural", message: "has 의 필드 이름이 비어있습니다", blockId: block.id });
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
    case BLOCK_TYPES.expr_set: {
      const elements: Expr[] = [];
      let cur = block.getInputTargetBlock("ITEMS");
      while (cur) {
        if (cur.type === BLOCK_TYPES.expr_set_item) {
          elements.push(readExpr(cur, "ITEM", errors));
        } else {
          errors.push({
            kind: "structural",
            message: `set 원소 슬롯에 예상치 못한 블록 "${cur.type}"`,
            blockId: cur.id,
          });
        }
        cur = cur.getNextBlock();
      }
      return { kind: "set", elements };
    }
    case BLOCK_TYPES.expr_record: {
      const pairs: { key: string; value: Expr }[] = [];
      let cur = block.getInputTargetBlock("PAIRS");
      while (cur) {
        if (cur.type === BLOCK_TYPES.expr_record_pair) {
          const key = (cur.getFieldValue("KEY") ?? "").trim();
          if (!key) {
            errors.push({
              kind: "structural",
              message: "레코드 키가 비어있습니다",
              blockId: cur.id,
            });
          }
          pairs.push({ key, value: readExpr(cur, "VALUE", errors) });
        } else {
          errors.push({
            kind: "structural",
            message: `record 쌍 슬롯에 예상치 못한 블록 "${cur.type}"`,
            blockId: cur.id,
          });
        }
        cur = cur.getNextBlock();
      }
      return { kind: "record", pairs };
    }
    case BLOCK_TYPES.expr_like: {
      const of = readExpr(block, "OF", errors);
      const raw = String(block.getFieldValue("PATTERN") ?? "");
      const pattern: LikePattern = parseLikePattern(raw);
      return { kind: "like", of, pattern };
    }
    case BLOCK_TYPES.expr_is: {
      const of = readExpr(block, "OF", errors);
      const entityType = (block.getFieldValue("TYPE") ?? "").trim();
      if (!entityType) {
        errors.push({
          kind: "structural",
          message: "is 의 Type 이 비어있습니다",
          blockId: block.id,
        });
      }
      const inExpr = readOptionalExpr(block, "IN", errors);
      return inExpr === null
        ? { kind: "is", of, entityType }
        : { kind: "is", of, entityType, in: inExpr };
    }
    case BLOCK_TYPES.expr_if: {
      return {
        kind: "if",
        cond: readExpr(block, "COND", errors),
        then: readExpr(block, "THEN", errors),
        else: readExpr(block, "ELSE", errors),
      };
    }
    case BLOCK_TYPES.expr_ext: {
      const fn = (block.getFieldValue("FN") ?? "").trim();
      if (!fn) {
        errors.push({
          kind: "structural",
          message: "확장 함수 이름이 비어있습니다",
          blockId: block.id,
        });
      }
      const args: Expr[] = [];
      let cur = block.getInputTargetBlock("ARGS");
      while (cur) {
        if (cur.type === BLOCK_TYPES.expr_ext_arg) {
          args.push(readExpr(cur, "ARG", errors));
        } else {
          errors.push({
            kind: "structural",
            message: `ext 인자 슬롯에 예상치 못한 블록 "${cur.type}"`,
            blockId: cur.id,
          });
        }
        cur = cur.getNextBlock();
      }
      return { kind: "ext", fn, args };
    }
    case BLOCK_TYPES.expr_raw: {
      // Payload sits in block.data (set by irToWorkspace.ts when seeding).
      // If the user dragged a fresh raw block from the toolbox, data is empty
      // — treat as `{ raw: null }` and let blocksToEst flag it.
      const raw = (block as unknown as { data?: string }).data ?? null;
      if (!raw) return { kind: "raw", est: null };
      try {
        return { kind: "raw", est: JSON.parse(raw) };
      } catch {
        errors.push({
          kind: "structural",
          message: "raw 블록의 EST JSON 이 손상되었습니다",
          blockId: block.id,
        });
        return { kind: "raw", est: null };
      }
    }
    case BLOCK_TYPES.expr_hole: {
      // Hidden payload (expected / default / optional / constraints) is
      // serialised on block.data. Required.
      const name = (block.getFieldValue("NAME") ?? "").trim();
      if (!name) {
        errors.push({
          kind: "structural",
          message: "파라미터 이름이 비어있습니다",
          blockId: block.id,
        });
      }
      const data = (block as unknown as { data?: string }).data ?? "";
      let payload: {
        expected?: string;
        default?: Expr;
        optional?: boolean;
        constraints?: { min?: number; max?: number; enum?: (string | number)[] };
      } = {};
      if (data) {
        try {
          payload = JSON.parse(data);
        } catch {
          errors.push({
            kind: "structural",
            message: `파라미터 "${name}" 의 메타데이터가 손상되었습니다`,
            blockId: block.id,
          });
        }
      } else {
        errors.push({
          kind: "structural",
          message: `파라미터 "${name}" 의 expected/default 메타데이터가 없습니다`,
          blockId: block.id,
        });
      }
      const label = (block.getFieldValue("LABEL") ?? "").trim();
      const type = (block.getFieldValue("TYPE") ?? "").trim();
      return {
        kind: "hole",
        name,
        expected: (payload.expected ?? "lit:string") as
          | "lit:long"
          | "lit:string"
          | "lit:bool"
          | "litEntity"
          | "set",
        default: payload.default ?? { kind: "lit", litType: "string", value: "" },
        ...(payload.optional ? { optional: true } : {}),
        ...(label ? { label } : {}),
        ...(type ? { type } : {}),
        ...(payload.constraints ? { constraints: payload.constraints } : {}),
      };
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

/** "abc*def*g" → [{Literal:"abc"}, "Wildcard", {Literal:"def"}, "Wildcard", {Literal:"g"}].
 *  Empty literal runs are skipped. */
function parseLikePattern(s: string): LikePattern {
  const out: LikePattern = [];
  let buf = "";
  for (const ch of s) {
    if (ch === "*") {
      if (buf) {
        out.push({ Literal: buf });
        buf = "";
      }
      out.push("Wildcard");
    } else {
      buf += ch;
    }
  }
  if (buf) out.push({ Literal: buf });
  return out;
}
