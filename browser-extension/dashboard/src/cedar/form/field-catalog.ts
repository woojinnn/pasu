/**
 * Field catalog for the form's pickers — a thin adapter over the EXISTING gloss
 * the block editor already uses (`gloss/paths.ts`), so the form and block tabs
 * offer the same fields. `allGloss()` already merges base (calldata) fields with
 * the custom enrichment fields (`context.custom.*`, role "derived"), so this is
 * the single source. We only add per-action filtering for custom fields (their
 * `appliesTo` lives in `manifest-gen/registry.ts`, not the gloss).
 *
 * A field's `fieldKind` drives which operators and value widget the form shows
 * ({@link operatorsFor}, {@link valueKindForField}).
 */

import { allGloss, type FieldKind } from "../../editor-v9/gloss/paths";
import { ENRICHMENT_FIELDS } from "../../editor-v9/manifest-gen/registry";

import type { FormOp, FormTrigger, FormValue } from "./model";

const CUSTOM_PREFIX = "context.custom.";

export interface FieldOption {
  /** Dotted path, e.g. `context.custom.inputUsd`. */
  path: string;
  /** Korean display label shown in the dropdown. */
  label: string;
  fieldKind: FieldKind;
  source: "base" | "custom";
  /** Optional unit suffix (e.g. "USD", "bp", "초"). */
  unit?: string;
}

/** The `action.tag` an enrichment field's `appliesTo` is keyed by; null = "any"
 *  (show every custom field). Action ids are PascalCase (`Swap`); enrichment
 *  tags are lowercase (`swap`). */
function triggerTag(trigger: FormTrigger): string | null {
  return trigger.kind === "actionEq" ? trigger.id.toLowerCase() : null;
}

/** Every field selectable for `trigger`: all base gloss fields + the custom
 *  enrichment fields valid for the trigger's action. */
export function fieldsForTrigger(trigger: FormTrigger): FieldOption[] {
  const tag = triggerTag(trigger);
  const out: FieldOption[] = [];
  for (const g of allGloss()) {
    const customName = g.path.startsWith(CUSTOM_PREFIX)
      ? g.path.slice(CUSTOM_PREFIX.length)
      : null;
    if (customName) {
      const def = ENRICHMENT_FIELDS[customName];
      if (tag !== null && def && !def.appliesTo.includes(tag)) continue;
      out.push({ path: g.path, label: g.ko, fieldKind: g.fieldKind, source: "custom", unit: g.unit?.ko });
    } else {
      out.push({ path: g.path, label: g.ko, fieldKind: g.fieldKind, source: "base", unit: g.unit?.ko });
    }
  }
  return out;
}

/** Operators offered for a field of `kind`. */
export function operatorsFor(kind: FieldKind): FormOp[] {
  switch (kind) {
    case "primitive.Bool":
      return ["==", "!="];
    case "primitive.Long":
      return ["==", "!=", "<", "<=", ">", ">="];
    case "primitive.decimal":
      return ["<", "<=", ">", ">=", "==", "!="];
    case "primitive.String":
      return ["==", "!=", "in"];
    case "collection":
      return ["contains"];
    case "ref":
      return ["==", "!="];
    case "record":
      return [];
  }
}

/** The value-widget kind for a field. NOTE: when the chosen operator is `in`,
 *  the value is always a `set` regardless of field kind — the UI overrides. */
export function valueKindForField(kind: FieldKind): FormValue["kind"] {
  switch (kind) {
    case "primitive.Bool":
      return "bool";
    case "primitive.Long":
      return "long";
    case "primitive.decimal":
      return "decimal";
    default:
      return "string"; // String / ref / collection / record
  }
}
