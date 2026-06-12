/**
 * Condition wrappers for `policy_hat`'s CONDITIONS statement list.
 *
 * `cond_when` and `cond_unless` share the same input shape; only the label and
 * tone differ. Multiple cond_* blocks stack vertically; Cedar semantics ANDs
 * them, with `unless` negating its body.
 *
 * Exported as factories (not consts) so user-facing strings resolve through
 * i18n at registration time, not at module import.
 */

import { i18n } from "../../i18n";

export const COND_WHEN_BLOCK_JSON = () =>
  ({
    type: "cond_when",
    message0: "when %1",
    args0: [{ type: "input_value", name: "BODY", check: "Expr" }],
    previousStatement: "Cond",
    nextStatement: "Cond",
    colour: 290,
    tooltip: i18n.t("blocks:block.cond_when.tooltip"),
  }) as const;

export const COND_UNLESS_BLOCK_JSON = () =>
  ({
    type: "cond_unless",
    message0: "unless %1",
    args0: [{ type: "input_value", name: "BODY", check: "Expr" }],
    previousStatement: "Cond",
    nextStatement: "Cond",
    colour: 0,
    tooltip: i18n.t("blocks:block.cond_unless.tooltip"),
  }) as const;
