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

import {
  allGloss,
  getGloss,
  type FieldKind,
  type GlossEntry,
  type Role,
} from "../../editor-v9/gloss/paths";
import { ENRICHMENT_FIELDS } from "../../editor-v9/manifest-gen/registry";

import { catalogFor } from "./schema-catalog";
import type { FormOp, FormTrigger, FormValue } from "./model";

const CUSTOM_PREFIX = "context.custom.";

export interface FieldOption {
  /** Dotted path, e.g. `context.direction.amountInNano`. */
  path: string;
  /** Korean display label shown in the dropdown. */
  label: string;
  fieldKind: FieldKind;
  /** Category — drives grouping + the colour dot in the picker. */
  role: Role;
  source: "base" | "custom";
  /** Optional unit suffix (e.g. "USD", "bp", "초"). */
  unit?: string;
  /** One-line plain-language hint shown under the label. */
  desc?: string;
  /** True when the field is optional in the schema (the form auto-adds the
   *  required `has` guards on save; surfaced so the picker can hint it). */
  optional?: boolean;
}

/** The `action.tag` an enrichment field's `appliesTo` is keyed by; null = "any"
 *  (show every custom field). Action ids are PascalCase (`Swap`); enrichment
 *  tags are lowercase (`swap`). */
function triggerTag(trigger: FormTrigger): string | null {
  return trigger.kind === "actionEq" ? trigger.id.toLowerCase() : null;
}

// ── gloss as a label/role layer over the schema-derived paths ───────────────
// The schema catalog owns the TRUTH (path, type, optionality); the gloss only
// supplies friendly ko labels / roles / units. We match by exact path first,
// then by an unambiguous leaf-name (so `context.direction.amountInNano` still
// picks up the "입력 수량 (nano)" gloss whose legacy path was `context.amountInNano`).
const BASE_GLOSS = allGloss().filter(
  (g) => !g.path.startsWith(CUSTOM_PREFIX) && !g.path.startsWith("context.enrichment."),
);
const GLOSS_BY_LEAF: Map<string, GlossEntry> = (() => {
  const m = new Map<string, GlossEntry>();
  const dup = new Set<string>();
  for (const g of BASE_GLOSS) {
    const leaf = g.path.split(".").pop()!;
    if (m.has(leaf)) dup.add(leaf);
    else m.set(leaf, g);
  }
  for (const d of dup) m.delete(d); // ambiguous leaf names → no fallback label
  return m;
})();

function glossFor(path: string): GlossEntry | undefined {
  return getGloss(path) ?? GLOSS_BY_LEAF.get(path.split(".").pop()!);
}

/** Readable fallback label when the gloss has nothing: drop `context.` and the
 *  `key` plumbing segment, join the rest. e.g. `context.tokenIn.key.address`
 *  → "tokenIn › address". */
function humanize(path: string): string {
  return path
    .split(".")
    .slice(1)
    .filter((s) => s !== "key")
    .join(" › ");
}

/** Heuristic role for a schema field with no gloss entry — drives grouping. */
function inferRole(path: string, fieldKind: FieldKind): Role {
  if (fieldKind === "primitive.Long" || fieldKind === "primitive.decimal") return "numeric";
  if (fieldKind === "primitive.Bool") return "enum";
  if (fieldKind === "collection") return "auth";
  const leaf = path.split(".").pop()!;
  if (
    /recipient|spender|delegatee|onbehalf|address|contract|operator|owner|account|target|destination|swapper|offerer|victim|staker|withdrawer|representative|builder|agent|approver|zone|conduit|validator|^user$|from$/i.test(
      leaf,
    )
  )
    return "address";
  if (/kind|mode|side|^type$|direction|tif|standard|support|effect/i.test(leaf)) return "enum";
  return "ref";
}

/** Every field selectable for `trigger`: the schema's leaf fields for the
 *  action + the custom enrichment fields valid for that action. */
export function fieldsForTrigger(trigger: FormTrigger): FieldOption[] {
  const out: FieldOption[] = [];

  // Schema-derived base fields — only those the chosen action actually exposes.
  for (const f of catalogFor(trigger)) {
    const g = glossFor(f.path);
    out.push({
      path: f.path,
      label: g?.ko ?? humanize(f.path),
      fieldKind: f.fieldKind,
      role: g?.role ?? inferRole(f.path, f.fieldKind),
      source: "base",
      unit: g?.unit?.ko,
      desc: g?.desc?.ko,
      optional: f.optional,
    });
  }

  // Custom enrichment fields (context.custom.*) — not in the schema catalog;
  // scoped by their registry `appliesTo`.
  const tag = triggerTag(trigger);
  for (const [name, def] of Object.entries(ENRICHMENT_FIELDS)) {
    if (tag !== null && !def.appliesTo.includes(tag)) continue;
    const path = `${CUSTOM_PREFIX}${name}`;
    const g = getGloss(path);
    if (!g) continue;
    out.push({
      path,
      label: g.ko,
      fieldKind: g.fieldKind,
      role: g.role,
      source: "custom",
      unit: g.unit?.ko,
      desc: g.desc?.ko,
      optional: true,
    });
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

// The trigger (검사 대상) action list is schema-derived — see ./actions.
export { KNOWN_ACTIONS, ACTION_GROUPS, type KnownAction } from "./actions";

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
