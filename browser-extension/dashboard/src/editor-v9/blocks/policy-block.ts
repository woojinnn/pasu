/**
 * `policy_hat` — the top-level Cedar policy header block.
 *
 * Fields:
 *   - EFFECT: dropdown `permit` / `forbid`.
 *
 * Inputs (in canvas order):
 *   - PRINCIPAL: value, accepts a Scope block.
 *   - ACTION:    value, accepts an ActionScope block.
 *   - RESOURCE:  value, accepts a Scope block.
 *   - CONDITIONS: statement list of Cond blocks (when/unless).
 *
 * One workspace ≡ one policy in Phase A. (Multi-policy workspaces arrive when
 * textToBlocks lands in Phase D — we just append more `policy_hat`s.)
 */

export const POLICY_BLOCK_JSON = {
  type: "policy_hat",
  message0: "%1",
  args0: [
    { type: "field_dropdown", name: "EFFECT", options: [["permit", "permit"], ["forbid", "forbid"]] },
  ],
  message1: "  principal %1",
  args1: [{ type: "input_value", name: "PRINCIPAL", check: "Scope" }],
  message2: "  action %1",
  args2: [{ type: "input_value", name: "ACTION", check: "ActionScope" }],
  message3: "  resource %1",
  args3: [{ type: "input_value", name: "RESOURCE", check: "Scope" }],
  message4: "  conditions %1",
  args4: [{ type: "input_statement", name: "CONDITIONS", check: "Cond" }],
  // No previous/next statement — only one policy per top-level chain.
  colour: 230,
  tooltip: "Cedar 정책 — effect(permit/forbid) + scope + conditions",
  helpUrl: "",
} as const;
