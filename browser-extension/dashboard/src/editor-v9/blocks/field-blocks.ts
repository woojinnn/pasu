/**
 * Field blocks — the UX layer over raw attr chains.
 *
 * Two surfaces, same IR target (attr chain rooted at a Cedar var):
 *
 *   1. Preset block per gloss path (`field_context_tokenIn`, ...).
 *      The label is the Korean display name; no editable fields. Users
 *      drop it as a single block, and workspaceToIR expands it to the
 *      canonical attr chain that cedar/blocks expects.
 *
 *   2. `expr_field` smart picker — single block with one dropdown listing
 *      every gloss entry. Lets the user search/scroll through all fields
 *      from one block instead of scanning category trees.
 *
 * Generated, not hand-written: the JSON list is a `.map` over GLOSS_ENTRIES,
 * so adding a path in `gloss/paths.ts` automatically adds a block (no JSON
 * churn here).
 */

import {
  allGloss,
  blockTypeForPath,
  ROLE_COLOUR,
  type GlossEntry,
} from "../gloss";

/** Build the JSON for one preset field block from a gloss entry. */
function presetFieldBlockJson(entry: GlossEntry): object {
  const unitSuffix = entry.unit ? ` (${entry.unit.ko})` : "";
  return {
    type: blockTypeForPath(entry.path),
    message0: entry.ko + unitSuffix,
    output: "Expr",
    colour: ROLE_COLOUR[entry.role],
    tooltip: `${entry.desc.ko}\n경로: ${entry.path}`,
  };
}

/** Every preset field block, in gloss order. Consumed by register.ts. */
export const FIELD_BLOCK_JSON_LIST: readonly object[] = allGloss().map(
  presetFieldBlockJson,
);

/** Smart picker — single dropdown over every gloss path. The dropdown value
 *  is the canonical path; the label is `<ko> — <path>` so the user sees
 *  both the human name and the structural hint. */
export const EXPR_FIELD_BLOCK_JSON = {
  type: "expr_field",
  message0: "필드 %1",
  args0: [
    {
      type: "field_dropdown",
      name: "PATH",
      options: allGloss().map((e) => [`${e.ko} — ${e.path}`, e.path] as const),
    },
  ],
  output: "Expr",
  colour: 220,
  tooltip:
    "주요 필드 드롭다운 — 한 블록으로 어떤 정의된 경로든 선택. " +
    "도메인 블록(왼쪽 색깔별 카테고리)이 더 보기 좋다면 그쪽을 쓰세요.",
} as const;
