/**
 * Read a single block into its `Expr` IR. Narrow: only supports the 5 value
 * kinds that `makeHole` accepts (`lit:{bool,long,string}`, `litEntity`, `set`).
 * Other inputs come back as a fallback that `makeHole` will reject so the
 * caller surfaces a clear error.
 *
 * Used by make-param.ts so it doesn't have to import the larger
 * workspaceToIR.blockToExpr (which depends on the full error-collection
 * machinery and the workspace's containment shape).
 */

import type * as Blockly from "blockly";
import type { Expr } from "../../cedar/blocks";
import { BLOCK_TYPES } from "../mapping/block-types";

export function readExprFromBlock(block: Blockly.Block, errors: string[]): Expr {
  switch (block.type) {
    case BLOCK_TYPES.expr_lit_bool: {
      const raw = block.getFieldValue("VALUE") ?? "true";
      return { kind: "lit", litType: "bool", value: raw === "true" };
    }
    case BLOCK_TYPES.expr_lit_long: {
      const raw = Number(block.getFieldValue("VALUE") ?? 0);
      if (!Number.isFinite(raw) || !Number.isInteger(raw)) {
        errors.push("정수 리터럴 값이 올바르지 않습니다");
        return { kind: "lit", litType: "long", value: 0 };
      }
      return { kind: "lit", litType: "long", value: raw };
    }
    case BLOCK_TYPES.expr_lit_string: {
      const raw = String(block.getFieldValue("VALUE") ?? "");
      return { kind: "lit", litType: "string", value: raw };
    }
    case BLOCK_TYPES.expr_lit_entity: {
      const type = (block.getFieldValue("TYPE") ?? "").trim();
      const id = (block.getFieldValue("ID") ?? "").trim();
      if (!type || !id) errors.push("엔티티 type/id 가 비어있습니다");
      return { kind: "litEntity", entity: { type, id } };
    }
    case BLOCK_TYPES.expr_set: {
      const elements: Expr[] = [];
      let cur = block.getInputTargetBlock("ITEMS");
      while (cur) {
        if (cur.type === BLOCK_TYPES.expr_set_item) {
          const inner = cur.getInputTargetBlock("ITEM");
          if (inner) elements.push(readExprFromBlock(inner, errors));
          else errors.push("빈 set 원소 슬롯");
        }
        cur = cur.getNextBlock();
      }
      return { kind: "set", elements };
    }
    default:
      errors.push(`파라미터화 불가 — 블록 "${block.type}"는 lit/litEntity/set 만 지원`);
      // Fallback that makeHole will reject.
      return { kind: "var", name: "principal" };
  }
}
