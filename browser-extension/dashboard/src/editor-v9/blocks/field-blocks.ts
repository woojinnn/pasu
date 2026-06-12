/**
 * Field blocks — the UX layer over raw attr chains.
 *
 * Two surfaces, same IR target (attr chain rooted at a Cedar var):
 *
 *   1. Preset block per gloss path (`field_context_tokenIn`, ...).
 *      The label is the locale display name; no editable fields. Users
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
 *
 * Exported as factories so labels/tooltips resolve through i18n (current
 * `i18n.language`) at registration time, not at module import.
 */

import { i18n } from "../../i18n";
import {
  allGloss,
  blockTypeForPath,
  glossDesc,
  glossLabel,
  glossUnit,
  ROLE_COLOUR,
  type GlossEntry,
} from "../gloss";

/** Build the JSON for one preset field block from a gloss entry. */
function presetFieldBlockJson(entry: GlossEntry): object {
  const unit = glossUnit(entry);
  const unitSuffix = unit ? ` (${unit})` : "";
  return {
    type: blockTypeForPath(entry.path),
    message0: glossLabel(entry) + unitSuffix,
    output: "Expr",
    colour: ROLE_COLOUR[entry.role],
    tooltip: i18n.t("blocks:block.field_preset.tooltip", {
      desc: glossDesc(entry),
      path: entry.path,
    }),
  };
}

/** Every preset field block, in gloss order. Consumed by register.ts. */
export const FIELD_BLOCK_JSON_LIST = (): readonly object[] =>
  allGloss().map(presetFieldBlockJson);

/** Smart picker — single dropdown over every gloss path. The dropdown value
 *  is the canonical path; the label is `<label> — <path>` so the user sees
 *  both the human name and the structural hint. */
export const EXPR_FIELD_BLOCK_JSON = () =>
  ({
    type: "expr_field",
    message0: i18n.t("blocks:block.expr_field.label"),
    args0: [
      {
        type: "field_dropdown",
        name: "PATH",
        options: allGloss().map((e) => [`${glossLabel(e)} — ${e.path}`, e.path] as const),
      },
    ],
    output: "Expr",
    colour: 220,
    tooltip: i18n.t("blocks:block.expr_field.tooltip"),
  }) as const;
