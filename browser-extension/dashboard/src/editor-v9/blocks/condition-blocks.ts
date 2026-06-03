/**
 * Condition wrappers for `policy_hat`'s CONDITIONS statement list.
 *
 * `cond_when` and `cond_unless` share the same input shape; only the label and
 * tone differ. Multiple cond_* blocks stack vertically; Cedar semantics ANDs
 * them, with `unless` negating its body.
 */

export const COND_WHEN_BLOCK_JSON = {
  type: "cond_when",
  message0: "when %1",
  args0: [{ type: "input_value", name: "BODY", check: "Expr" }],
  previousStatement: "Cond",
  nextStatement: "Cond",
  colour: 290,
  tooltip: "조건 (when) — 안의 식이 true일 때 정책 적용",
} as const;

export const COND_UNLESS_BLOCK_JSON = {
  type: "cond_unless",
  message0: "unless %1",
  args0: [{ type: "input_value", name: "BODY", check: "Expr" }],
  previousStatement: "Cond",
  nextStatement: "Cond",
  colour: 0,
  tooltip: "조건 (unless) — 안의 식이 true일 때 정책 차단",
} as const;
