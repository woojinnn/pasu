/**
 * Form-editor model — the small, constrained shape the "폼으로 만들기" UI edits.
 *
 * A policy is a `forbid` over an action-eq trigger with two flat condition lists
 * (`when` and `unless`). Each condition is a single comparison with its own
 * `not` and a `joiner` (AND/OR) to the previous one. AND binds tighter than OR,
 * so the list reads as an OR of AND-runs (e.g. `A 그리고 B 또는 C` = `(A∧B)∨C`)
 * — a flat, query-builder UX that still covers most real policies. Anything
 * deeper (nested OR-groups, if/then/else, …) hands off to the Block tab.
 *
 * The form NEVER assembles Cedar text. It builds this model, {@link formToIr}
 * turns it into a `PolicyIR`, and the existing pipeline renders Cedar.
 * {@link irToForm} is the reverse and returns `null` for anything outside the
 * subset.
 */

/** Comparison operators the form offers. `contains`/`in` are membership over a
 *  set field / a literal set respectively. */
export type FormOp = "==" | "!=" | "<" | "<=" | ">" | ">=" | "contains" | "in";

/** A typed leaf value. The field's type (from the gloss/enrichment catalog)
 *  picks which variant the value widget produces. */
export type FormValue =
  | { kind: "bool"; value: boolean }
  | { kind: "long"; value: number }
  /** A decimal extension value — kept as its source string (e.g. "0.05"). */
  | { kind: "decimal"; value: string }
  | { kind: "string"; value: string }
  /** A literal set of strings, for the `in` operator (`x in ["a","b"]`). */
  | { kind: "set"; values: string[] }
  /** Another field (compare field-vs-field, e.g. `recipient != principal.address`). */
  | { kind: "field"; path: string };

/** A single comparison: `<fieldPath> <op> <value>`, e.g.
 *  `context.custom.inputUsd >= 0.05`. */
export interface FormLeaf {
  /** Dotted attribute path rooted at a request var, e.g. `context.flagged`. */
  fieldPath: string;
  op: FormOp;
  value: FormValue;
}

/** AND/OR connector between conditions. */
export type GroupOp = "and" | "or";

/** One row of the condition list: a comparison, optionally negated, joined to
 *  the previous row by `joiner` (ignored for the first row). */
export interface FormCondition extends FormLeaf {
  /** Wrap this single condition in `!(…)`. */
  not?: boolean;
  /** Connector to the PREVIOUS condition. The first row's value is ignored. */
  joiner: GroupOp;
}

/** An explicit parenthesized group — `(…)` — of conditions, joined to its
 *  siblings by `joiner`. One level deep (its `conds` are plain leaves), which is
 *  enough for CNF like `(A | B) & (C | D)`; deeper nesting hands off to blocks. */
export interface FormGroupNode {
  kind: "group";
  joiner: GroupOp;
  not?: boolean;
  conds: FormCondition[];
}

/** A node in a clause's list: either a bare condition or a `(…)` group box.
 *  A bare condition has no `kind`; a group is `{ kind: "group", … }`. */
export type FormNode = FormCondition | FormGroupNode;

/** Type guard: is this node a `(…)` group box? */
export function isGroupNode(n: FormNode): n is FormGroupNode {
  return "kind" in n && n.kind === "group";
}

/** What the policy applies to (검사 대상). v1 supports action-scope equality
 *  (`action == Type::"id"`) and "any action". */
export type FormTrigger =
  | { kind: "actionEq"; entityType: string; id: string }
  | { kind: "any" };

/** Severity drives the `@severity` annotation; the effect is always `forbid`. */
export type FormSeverity = "warn" | "deny" | "info";

/** The whole form: trigger + when/unless condition lists + notify metadata. */
export interface FormModel {
  trigger: FormTrigger;
  /** `when` nodes (conditions and/or `(…)` groups). Empty = no `when`. */
  when: FormNode[];
  /** `unless` nodes — exceptions ("단, ~인 경우는 제외"). */
  unless: FormNode[];
  id: string;
  severity: FormSeverity;
  reason: string;
}

/** An empty starter model for "새 정책 → 폼으로 만들기". */
export function emptyFormModel(id = "untitled-policy"): FormModel {
  return { trigger: { kind: "any" }, when: [], unless: [], id, severity: "warn", reason: "" };
}
