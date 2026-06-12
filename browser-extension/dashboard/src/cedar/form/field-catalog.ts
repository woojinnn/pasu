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

import { i18n } from "../../i18n";
import enCuratedJson from "../../i18n/locales/en/fields-curated.json";

import { CURATED_FIELD_META } from "./curated-field-meta.generated";
import { catalogFor } from "./schema-catalog";
import type { FormOp, FormTrigger, FormValue } from "./model";

const CUSTOM_PREFIX = "context.custom.";

/** English overlay for the curated (ko) field meta — filled by the translation
 *  pipeline; any path missing here falls back to the ko entry. */
const EN_CURATED_FIELD_META = enCuratedJson as Record<
  string,
  { label: string; desc?: string }
>;

/** True when the app currently speaks English (labels compose differently). */
const isEn = () => i18n.language === "en";

/** Curated label/desc for `path` under the CURRENT language: the en overlay
 *  when active and present (desc falls back per-field to ko), else the ko
 *  source of truth ({@link CURATED_FIELD_META}). */
function curatedFor(path: string): { label: string; desc?: string } | undefined {
  const ko = CURATED_FIELD_META[path];
  if (isEn()) {
    const en = EN_CURATED_FIELD_META[path];
    if (en) return { label: en.label, desc: en.desc ?? ko?.desc };
  }
  return ko;
}

export interface FieldOption {
  /** Dotted path, e.g. `context.direction.amountInNano`. */
  path: string;
  /** Localized display label shown in the dropdown. */
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
  /** True for engine-internal fields (nano mirrors, raw-hex amounts, deep
   *  `.key`/state sub-fields, …) hidden behind the picker's "고급 필드 보기"
   *  toggle so ordinary users see only the meaningful handful. */
  advanced?: boolean;
  /** Value scaling for the widget. `"nano"` = the field stores a Long in nano
   *  (token × 10⁹); the form lets the user enter/read plain token units and
   *  converts under the hood, so "nano" never reaches the user. */
  scale?: "nano";
}

/** The `action.tag` an enrichment field's `appliesTo` is keyed by; null = "any"
 *  (show every custom field). Action ids are PascalCase (`Erc20Transfer`);
 *  enrichment tags are snake_case (`erc20_transfer`) — same as the manifest
 *  generator's `actionTag`, so the picker and generation agree. */
function triggerTag(trigger: FormTrigger): string | null {
  if (trigger.kind !== "actionEq") return null;
  return trigger.id.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
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

// ── Plain-language label composition for un-glossed schema fields ───────────
// The curated gloss only names ~50 well-known paths; the schema exposes ~500
// more (mostly `<tokenSlot>.key.address` / `.key.chain` and deep state
// records). Rather than show raw English breadcrumbs ("tokenIn › address"),
// we compose a plain label from segment dictionaries (i18n "fields" namespace,
// `leaf.*` / `prefix.*`) so the picker never leaks a path. Unknown segments
// fall back to a spaced camelCase form (only reachable for the hidden
// "advanced" long-tail). The slot-prefix dictionary (`prefix.*`) covers the
// first segment (usually a named token/asset slot); leaf segments (`leaf.*`)
// repeat across hundreds of fields, so naming them well covers most of the
// un-glossed set.

/** Spaced, lower-cased camelCase fallback for a segment with no dictionary
 *  entry, e.g. `reserveState` → "reserve state". Only the hidden long-tail
 *  reaches this. */
function camelWords(seg: string): string {
  return seg.replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase();
}

/** Localized text for one path segment — slot-prefix dictionary first (head
 *  segment only), then the leaf dictionary, then the camelCase fallback. */
function segmentLabel(seg: string, isFirst: boolean): string {
  if (isFirst && i18n.exists(`prefix.${seg}`, { ns: "fields" })) {
    return i18n.t(`prefix.${seg}`, { ns: "fields" });
  }
  if (i18n.exists(`leaf.${seg}`, { ns: "fields" })) {
    return i18n.t(`leaf.${seg}`, { ns: "fields" });
  }
  return camelWords(seg);
}

/** Compose a plain-language label for a dotted path with no gloss entry.
 *  Drops `context.` and the `.key` plumbing node and joins the rest, so
 *  `context.tokenIn.key.address` → "입력 토큰 주소" (en: "Input token
 *  address"), `context.venue.pool` → "베뉴 풀". English keeps the same
 *  attribute-last order and capitalizes the sentence head. */
function composeLabel(path: string): string {
  const segs = path.split(".").slice(1).filter((s) => s !== "key");
  if (segs.length === 0) return path;
  const joined = segs.map((s, i) => segmentLabel(s, i === 0)).join(" ");
  return isEn() ? joined.charAt(0).toUpperCase() + joined.slice(1) : joined;
}

/** The gloss label in the current language (the gloss carries both ko + en). */
function glossLabel(g: GlossEntry | undefined): string | undefined {
  if (!g) return undefined;
  return isEn() ? g.en : g.ko;
}

/** The form's display label for any dotted path — curated gloss first,
 *  composed label otherwise (en additionally checks the curated en overlay).
 *  Exported so other surfaces (the structure diagram) speak the same
 *  vocabulary instead of leaking raw paths. */
export function labelForPath(path: string): string {
  const raw = isEn()
    ? (EN_CURATED_FIELD_META[path]?.label ?? glossFor(path)?.en ?? composeLabel(path))
    : (glossFor(path)?.ko ?? composeLabel(path));
  return raw.replace(/\s*\(\s*nano\s*\)/i, "").trim();
}

/** True for engine-internal fields the form hides by default (Rule 3): nano
 *  mirrors, raw-hex amount strings, deep `.key`/state sub-fields, chain ids.
 *  A field stays prominent if it is glossed, a depth-1 primitive scalar, or a
 *  token-identity `<slot>.key.address`. */
function isAdvancedField(path: string, fieldKind: FieldKind, glossed: boolean): boolean {
  const segs = path.split(".").slice(1); // drop "context"
  const leaf = segs[segs.length - 1] ?? "";
  // Raw amount strings ("…(원본)") demote EVEN when glossed — they're exact
  // uint256 strings with no ordering, so the form can only ==/≠ them; the
  // nano/USD siblings are the comparable versions users actually want.
  if (fieldKind === "primitive.String" && /amount|^buyMin|^sellAmount|^netInput|^minOut/i.test(leaf)) return true;
  if (glossed) return false;
  // The token-identity address (what most token gates compare) stays prominent.
  if (/\.key\.address$/.test(path)) return false;
  // Bare record nodes (`.key`, state blobs) aren't comparable → hide.
  if (fieldKind === "record") return true;
  // Anything nested two or more levels deep (chain ids, venue internals, …).
  if (segs.filter((s) => s !== "key").length >= 2) return true;
  return false;
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
    const cur = curatedFor(f.path);
    const leaf = f.path.split(".").pop()!;
    // nano mirror (Long named `*Nano`): the WIDGET still scales by 10⁹ under the
    // hood (user enters/reads plain token units), but the label/desc now come
    // from the curated catalog verbatim ("수량 (비교용)" etc.).
    const isNano = f.fieldKind === "primitive.Long" && /Nano$/.test(leaf);
    const glossed = Boolean(g);
    const composed = (glossLabel(g) ?? composeLabel(f.path)).replace(/\s*\(\s*nano\s*\)/i, "").trim();
    // Curated catalog (func_module/field-explorer) wins for label + desc; fall
    // back to gloss / composed label for any path it doesn't cover.
    const label = cur?.label ?? composed;
    // Safety net (ko only — under en every label is English by design): never
    // show a half-translated label (a leftover lowercase English run like
    // "gas estimate") in the visible list — demote it to the "고급 필드" tray.
    // A curated label is always plain Korean, so it's exempt.
    const hasEnglish = !cur && !glossed && !isEn() && /[a-z]{2,}/.test(label);
    out.push({
      path: f.path,
      label,
      fieldKind: f.fieldKind,
      role: g?.role ?? inferRole(f.path, f.fieldKind),
      source: "base",
      unit: isNano ? i18n.t("unit.token", { ns: "fields" }) : isEn() ? g?.unit?.en : g?.unit?.ko,
      desc: cur?.desc ?? (isEn() ? g?.desc?.en : g?.desc?.ko),
      optional: f.optional,
      advanced: isAdvancedField(f.path, f.fieldKind, glossed) || hasEnglish,
      ...(isNano ? { scale: "nano" as const } : {}),
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
    const cur = curatedFor(path);
    out.push({
      path,
      label: cur?.label ?? (isEn() ? g.en : g.ko),
      fieldKind: g.fieldKind,
      role: g.role,
      source: "custom",
      unit: isEn() ? g.unit?.en : g.unit?.ko,
      desc: cur?.desc ?? (isEn() ? g.desc?.en : g.desc?.ko),
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
      return ["==", "!=", "in", "notIn"];
    case "collection":
      return ["contains", "notContains"];
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
