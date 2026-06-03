/**
 * "Make Parameter" — convert a value block (lit/litEntity/set) into an
 * `expr_hole` parameter slot. Wired into Blockly's per-block context menu so
 * authors right-click a literal → 파라미터로 만들기 → fill metadata → done.
 *
 * Uses cedar/blocks' `makeHole()` for validation (only lit / litEntity / set
 * are parameterisable). On success, the original block is detached and the
 * hole replaces it in the parent's connection.
 *
 * Metadata prompts use native `prompt()` for Phase E. Phase E.next or F can
 * upgrade to a proper React modal once the UX needs more constraints fields.
 */

import * as Blockly from "blockly";
import type { Expr, HoleNode } from "../../cedar/blocks";
import { makeHole } from "../../cedar/blocks";
import { BLOCK_TYPES } from "../mapping/block-types";
import { readExprFromBlock } from "./read-expr";

/** Block types that can be turned into a parameter (per cedar/blocks rule). */
const PARAMETERISABLE_TYPES = new Set<string>([
  BLOCK_TYPES.expr_lit_bool,
  BLOCK_TYPES.expr_lit_long,
  BLOCK_TYPES.expr_lit_string,
  BLOCK_TYPES.expr_lit_entity,
  BLOCK_TYPES.expr_set,
]);

/** Snake-case-ish identifier; matches the cedar/blocks param-name conventions
 *  loosely. Empty / duplicate / invalid name → rejected. */
const NAME_RE = /^[a-zA-Z][a-zA-Z0-9_]*$/;

interface PromptParamsResult {
  name: string;
  label?: string;
  type?: string;
  optional: boolean;
}

/** Native prompt-driven metadata collection. Returns null on user cancel. */
function promptForMetadata(
  defaultName: string,
  existingNames: Set<string>,
): PromptParamsResult | null {
  let name = defaultName;
  while (true) {
    const raw = window.prompt(
      "파라미터 이름 (영문 식별자):",
      name,
    );
    if (raw === null) return null;
    name = raw.trim();
    if (!NAME_RE.test(name)) {
      window.alert("이름은 영문/숫자/_ 조합만 가능합니다 (첫 글자는 영문)");
      continue;
    }
    if (existingNames.has(name)) {
      window.alert(`"${name}" 이름이 이미 사용 중입니다`);
      continue;
    }
    break;
  }
  const label = window.prompt("라벨 (UI 표시명, 선택):", name) ?? "";
  const type = window.prompt("타입 힌트 (예: address / amount / token, 선택):", "") ?? "";
  const optional = window.confirm(
    "이 파라미터를 선택적으로 만들까요?\n(채우지 않아도 기본값으로 동작)",
  );
  return { name, label: label.trim() || undefined, type: type.trim() || undefined, optional };
}

/** Collect every hole name currently in the workspace. Used to enforce
 *  uniqueness when the author adds a new parameter. */
function collectHoleNames(ws: Blockly.WorkspaceSvg): Set<string> {
  const names = new Set<string>();
  for (const b of ws.getAllBlocks(false)) {
    if (b.type === BLOCK_TYPES.expr_hole) {
      const n = (b.getFieldValue("NAME") ?? "").trim();
      if (n) names.add(n);
    }
  }
  return names;
}

/** Replace a value block with an expr_hole block in the same parent slot.
 *  Caller has already validated `value` and produced the HoleNode. */
function swapBlockToHole(
  ws: Blockly.WorkspaceSvg,
  oldBlock: Blockly.BlockSvg,
  hole: HoleNode,
): void {
  const parent = oldBlock.outputConnection?.targetBlock();
  const parentInputName = parent
    ? findInputNameTargeting(parent, oldBlock)
    : null;

  oldBlock.unplug(false);

  const holeBlock = ws.newBlock(BLOCK_TYPES.expr_hole) as Blockly.BlockSvg;
  holeBlock.setFieldValue(hole.name, "NAME");
  holeBlock.setFieldValue(hole.label ?? "", "LABEL");
  holeBlock.setFieldValue(hole.type ?? "", "TYPE");
  const payload = {
    expected: hole.expected,
    default: hole.default,
    ...(hole.optional ? { optional: true } : {}),
    ...(hole.constraints ? { constraints: hole.constraints } : {}),
  };
  (holeBlock as unknown as { data: string }).data = JSON.stringify(payload);
  holeBlock.initSvg();
  holeBlock.render();

  if (parent && parentInputName) {
    parent.getInput(parentInputName)?.connection?.connect(holeBlock.outputConnection);
  } else {
    holeBlock.moveTo(oldBlock.getRelativeToSurfaceXY());
  }
  oldBlock.dispose(false);
}

function findInputNameTargeting(parent: Blockly.Block, child: Blockly.Block): string | null {
  for (const input of parent.inputList) {
    if (input.connection?.targetBlock() === child) return input.name;
  }
  return null;
}

/** Register a "파라미터로 만들기" item on the per-block context menu. Idempotent
 *  via a module-level latch. */
let registered = false;

export function registerMakeParamContextMenu(): void {
  if (registered) return;
  Blockly.ContextMenuRegistry.registry.register({
    id: "scopeball_make_param",
    weight: 0,
    scopeType: Blockly.ContextMenuRegistry.ScopeType.BLOCK,
    displayText: () => "파라미터로 만들기…",
    preconditionFn: ({ block }) =>
      PARAMETERISABLE_TYPES.has(block.type) ? "enabled" : "hidden",
    callback: ({ block }) => {
      const ws = block.workspace as Blockly.WorkspaceSvg;
      const errors: string[] = [];
      const expr: Expr = readExprFromBlock(block, errors);
      if (errors.length > 0) {
        window.alert(`파라미터화 실패: ${errors[0]}`);
        return;
      }
      const existing = collectHoleNames(ws);
      const meta = promptForMetadata(suggestNameFor(expr), existing);
      if (!meta) return;
      let hole: HoleNode;
      try {
        hole = makeHole(expr, {
          name: meta.name,
          label: meta.label,
          type: meta.type,
          optional: meta.optional,
        });
      } catch (e) {
        window.alert(`makeHole 실패: ${e instanceof Error ? e.message : String(e)}`);
        return;
      }
      swapBlockToHole(ws, block as Blockly.BlockSvg, hole);
    },
  });
  registered = true;
}

function suggestNameFor(expr: Expr): string {
  switch (expr.kind) {
    case "lit":
      return expr.litType === "long"
        ? "n"
        : expr.litType === "string"
          ? "s"
          : "flag";
    case "litEntity":
      return "entity";
    case "set":
      return "items";
    default:
      return "param";
  }
}
