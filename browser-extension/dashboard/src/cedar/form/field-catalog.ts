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

import { allGloss, type FieldKind, type Role } from "../../editor-v9/gloss/paths";
import { ENRICHMENT_FIELDS } from "../../editor-v9/manifest-gen/registry";

import type { FormOp, FormTrigger, FormValue } from "./model";

const CUSTOM_PREFIX = "context.custom.";

/** Field kinds the form can compare with a single leaf. `ref`/`record` are
 *  containers (a whole token / a `{a,b}` record) — comparing them to a literal
 *  doesn't typecheck in Cedar, so they're hidden from the form (use the Block
 *  tab to drill into their subfields). */
const COMPARABLE_KINDS = new Set<FieldKind>([
  "primitive.String",
  "primitive.Long",
  "primitive.decimal",
  "primitive.Bool",
  "collection",
]);

export interface FieldOption {
  /** Dotted path, e.g. `context.custom.inputUsd`. */
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
    if (!COMPARABLE_KINDS.has(g.fieldKind)) continue; // hide containers (ref/record)
    const customName = g.path.startsWith(CUSTOM_PREFIX)
      ? g.path.slice(CUSTOM_PREFIX.length)
      : null;
    const common = {
      path: g.path,
      label: g.ko,
      fieldKind: g.fieldKind,
      role: g.role,
      unit: g.unit?.ko,
      desc: g.desc?.ko,
    };
    if (customName) {
      const def = ENRICHMENT_FIELDS[customName];
      if (tag !== null && def && !def.appliesTo.includes(tag)) continue;
      out.push({ ...common, source: "custom" });
    } else {
      out.push({ ...common, source: "base" });
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

/** A selectable action for the trigger (검사 대상) dropdown. `entityType`/`id`
 *  map to `action == entityType::"id"`. v1 ships a curated list of the common
 *  actions; "any action" is offered separately by the UI. */
export interface KnownAction {
  entityType: string;
  id: string;
  label: string;
}

export const KNOWN_ACTIONS: readonly KnownAction[] = [
  { entityType: "Amm::Action", id: "Swap", label: "스왑" },
  { entityType: "Amm::Action", id: "RemoveLiquidity", label: "유동성 제거" },
  { entityType: "Token::Action", id: "Erc20Approve", label: "토큰 승인" },
  { entityType: "Token::Action", id: "Erc20Transfer", label: "토큰 전송" },
  { entityType: "Airdrop::Action", id: "Claim", label: "에어드랍 청구" },
  { entityType: "Core::Action", id: "Unknown", label: "알 수 없는 거래" },
];

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
