/**
 * `expr_hole` — a named parameter slot. The IR equivalent is HoleNode in
 * cedar/blocks/params.ts. A template = a PolicyIR containing one or more
 * holes; adopters call fillParams(template, values) to materialize a concrete
 * policy.
 *
 * Visible fields:
 *   NAME   — parameter id (`maxUsd`), uniquely identifies the hole within the
 *            policy and is the key adopters use to fill it. Author-set.
 *   LABEL  — human-readable name shown in the adopter form. Author-set.
 *   TYPE   — display hint (e.g. "address", "amount"). Optional.
 *
 * Hidden payload — `block.data` carries a JSON blob with everything that
 * doesn't round-trip well through Blockly fields:
 *   { expected: "lit:long"|...,
 *     default:  Expr,
 *     optional: boolean,
 *     constraints?: { min, max, enum } }
 *
 * The visible label changes to `[name : type]` if TYPE is non-empty, else
 * `[name]`. Adopter view (Phase F) replaces this with the supplied value
 * before evaluation.
 *
 * Exported as a factory so the tooltip resolves through i18n at registration
 * time, not at module import.
 */

import { i18n } from "../../i18n";

export const EXPR_HOLE_BLOCK_JSON = () =>
  ({
    type: "expr_hole",
    message0: "param %1",
    args0: [{ type: "field_input", name: "NAME", text: "param" }],
    message1: "label %1",
    args1: [{ type: "field_input", name: "LABEL", text: "" }],
    message2: "type %1",
    args2: [{ type: "field_input", name: "TYPE", text: "" }],
    output: "Expr",
    inputsInline: false,
    colour: 320,
    tooltip: i18n.t("blocks:block.expr_hole.tooltip"),
  }) as const;
