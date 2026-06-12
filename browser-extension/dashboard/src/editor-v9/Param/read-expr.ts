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
import { i18n } from "../../i18n";
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
        errors.push(i18n.t("blocks:param.longInvalid"));
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
      if (!type || !id) errors.push(i18n.t("blocks:param.entityEmpty"));
      return { kind: "litEntity", entity: { type, id } };
    }
    case BLOCK_TYPES.expr_set: {
      const elements: Expr[] = [];
      let cur = block.getInputTargetBlock("ITEMS");
      while (cur) {
        if (cur.type === BLOCK_TYPES.expr_set_item) {
          const inner = cur.getInputTargetBlock("ITEM");
          if (inner) elements.push(readExprFromBlock(inner, errors));
          else errors.push(i18n.t("blocks:param.setItemEmpty"));
        }
        cur = cur.getNextBlock();
      }
      return { kind: "set", elements };
    }
    default:
      errors.push(i18n.t("blocks:param.unsupportedBlock", { type: block.type }));
      // Fallback that makeHole will reject.
      return { kind: "var", name: "principal" };
  }
}
