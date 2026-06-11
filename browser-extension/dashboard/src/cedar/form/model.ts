/**
 * Form-editor model — the small, constrained shape the "폼으로 만들기" UI edits.
 *
 * A policy is a `forbid` over an action-eq trigger with two flat condition lists
 * (`when` and `unless`). Each condition is a single comparison with a `joiner`
 * (AND/OR) to the previous one. AND binds tighter than OR,
 * so the list reads as an OR of AND-runs ("위험 상황" cards in the UI). Inside a
 * run, {@link FormGroupNode} containers nest recursively with alternating
 * AND/OR parity, covering arbitrary boolean structure. What the form still
 * can't hold (if/then/else, like/is, …) stays Cedar-text-only.
 *
 * The form NEVER assembles Cedar text. It builds this model, {@link formToIr}
 * turns it into a `PolicyIR`, and the existing pipeline renders Cedar.
 * {@link irToForm} is the reverse and returns `null` for anything outside the
 * subset.
 */

/** Comparison operators the form offers. `contains`/`in` are membership over a
 *  set field / a literal set; `notContains`/`notIn` are their complements.
 *  EVERY op has a complement, so the form needs no separate NOT toggle — a
 *  hand-written `!(…)` canonicalizes into complement ops (De Morgan) on open. */
export type FormOp =
  | "=="
  | "!="
  | "<"
  | "<="
  | ">"
  | ">="
  | "contains"
  | "notContains"
  | "in"
  | "notIn";

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
  /** "지갑별 설정" 승격 — 켜지면 이 값이 정의의 파라미터(홀)가 되고, `value`는
   *  기본값이 된다. 지갑 트리에서 바인딩별로 다른 값을 줄 수 있다. */
  param?: { name: string; label: string } | undefined;
}

/** AND/OR connector between conditions. */
export type GroupOp = "and" | "or";

/** One row of the condition list, joined to the previous row by `joiner`
 *  (ignored for the first row). Negation lives in the operator (≠, 포함 안 함,
 *  …), never as a separate flag. */
export interface FormCondition extends FormLeaf {
  /** Connector to the PREVIOUS condition. The first row's value is ignored. */
  joiner: GroupOp;
}

/** An explicit parenthesized group — `(…)` — of conditions and/or deeper
 *  groups. A group's MEANING comes from nesting parity, not a stored op: a
 *  group sitting in an AND context (a situation card, or an AND-subgroup) is an
 *  OR of its children ("다음 중 하나라도"); a group sitting in an OR group is an
 *  AND of its children ("다음에 모두 해당"). Alternating containers express any
 *  boolean formula (NOT stays a per-node toggle). Children's `joiner` carries
 *  no meaning inside a group and is normalized to head "and" / rest "or".
 *  A hand-written `!(group)` De-Morgans into the opposite container with
 *  complemented leaf operators on open, so groups carry no NOT either. */
export interface FormGroupNode {
  kind: "group";
  joiner: GroupOp;
  conds: FormNode[];
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
