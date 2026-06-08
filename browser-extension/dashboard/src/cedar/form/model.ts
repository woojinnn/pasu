/**
 * Form-editor model — the small, constrained shape the "폼으로 만들기" UI edits.
 *
 * It is a deliberately tiny SUBSET of {@link PolicyIR}: a single `forbid` whose
 * `when` body is an AND of OR-groups of simple comparisons, plus an action-scope
 * trigger and the `@id`/`@severity`/`@reason` annotations. Everything the form
 * cannot express round-trips through the Cedar/Block tabs instead.
 *
 * The form NEVER assembles Cedar text. It builds this model, {@link formToIr}
 * turns it into a `PolicyIR`, and the existing pipeline (`blocksToText`) renders
 * Cedar — so manifest generation, address-casing normalization, and the diagram
 * all keep working. {@link irToForm} is the reverse and returns `null` for any
 * policy outside this subset (so the form honestly refuses to open it).
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

/** One condition row: `<fieldPath> <op> <value>`, e.g.
 *  `context.custom.inputUsd >= 0.05`. */
export interface FormLeaf {
  /** Dotted attribute path rooted at a request var, e.g. `context.flagged`. */
  fieldPath: string;
  op: FormOp;
  value: FormValue;
}

/** A group of leaves OR-ed together (the row + its `+ 또는(OR)` siblings).
 *  `negated` wraps the whole group in `!(…)` ("다음이 아닐 때"). */
export interface FormGroup {
  leaves: FormLeaf[];
  negated?: boolean;
}

/** What the policy applies to (검사 대상). v1 supports action-scope equality
 *  (`action == Type::"id"`) and "any action". */
export type FormTrigger =
  | { kind: "actionEq"; entityType: string; id: string }
  | { kind: "any" };

/** Severity drives the `@severity` annotation; the effect is always `forbid`. */
export type FormSeverity = "warn" | "deny" | "info";

/** The whole form: trigger + AND-of-OR condition groups + notify metadata. */
export interface FormModel {
  trigger: FormTrigger;
  /** `when` body — groups are AND-ed; leaves within a group are OR-ed. Empty =
   *  no `when` (the action is forbidden unconditionally). */
  groups: FormGroup[];
  /** `unless` body — exceptions ("단, ~인 경우는 제외"). Same AND-of-OR shape. */
  unlessGroups: FormGroup[];
  id: string;
  severity: FormSeverity;
  reason: string;
}

/** An empty starter model for "새 정책 → 폼으로 만들기". */
export function emptyFormModel(id = "untitled-policy"): FormModel {
  return { trigger: { kind: "any" }, groups: [], unlessGroups: [], id, severity: "warn", reason: "" };
}
