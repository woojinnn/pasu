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

// ── Korean label composition for un-glossed schema fields ───────────────────
// The curated gloss only names ~50 well-known paths; the schema exposes ~500
// more (mostly `<tokenSlot>.key.address` / `.key.chain` and deep state
// records). Rather than show raw English breadcrumbs ("tokenIn › address"),
// we compose a plain-Korean label from a segment dictionary so the picker
// never leaks a path. Unknown segments fall back to a spaced camelCase form
// (only reachable for the hidden "advanced" long-tail).

/** Leaf-segment → Korean. These repeat across hundreds of fields, so naming
 *  them well covers most of the un-glossed set. */
const LEAF_KO: Record<string, string> = {
  address: "주소",
  chain: "체인",
  name: "이름",
  symbol: "심볼",
  pool: "풀",
  poolType: "풀 종류",
  poolId: "풀 ID",
  contract: "컨트랙트",
  vault: "볼트",
  market: "마켓",
  marketId: "마켓 ID",
  amount: "수량",
  size: "크기",
  side: "방향",
  kind: "종류",
  price: "가격",
  recipient: "수신자",
  spender: "지출 승인 대상",
  owner: "소유자",
  operator: "오퍼레이터",
  router: "라우터",
  factory: "팩토리",
  platform: "플랫폼",
  protocol: "프로토콜",
  token: "토큰",
  tokenId: "토큰 ID",
  asset: "자산",
  id: "ID",
  deadline: "마감",
  expiry: "만료",
  leverage: "레버리지",
  collateral: "담보",
  fee: "수수료",
  nonce: "논스",
  from: "보내는 주소",
  destination: "목적지",
  delegatee: "위임 대상",
  staker: "스테이커",
};

/** Slot-prefix → Korean. The first segment is usually a named token/asset
 *  slot; translate the common ones, leave the rest to the camelCase fallback. */
const PREFIX_KO: Record<string, string> = {
  tokenIn: "입력 토큰",
  tokenOut: "출력 토큰",
  assetIn: "입력 자산",
  assetOut: "출력 자산",
  baseAsset: "기준 자산",
  collateralToken: "담보 토큰",
  collateralAsset: "담보 자산",
  collatAsset: "담보 자산",
  addCollateralToken: "추가 담보 토큰",
  debtAsset: "부채 자산",
  claimToken: "청구 토큰",
  rewardToken: "보상 토큰",
  refundToken: "환불 토큰",
  payToken: "지불 토큰",
  externalToken: "외부 토큰",
  stakeToken: "스테이킹 토큰",
  allocatedToken: "배정 토큰",
  lpToken: "LP 토큰",
  venue: "거래 장소",
  market: "선물 시장",
  asset: "자산",
  token: "토큰",
  buy: "매수",
  sell: "매도",
  source: "출처",
  claimTarget: "청구 대상",
  nftKey: "NFT",
  // common depth-1 fields that would otherwise leak English
  gasEstimate: "예상 가스비",
  routeEstimatedOut: "예상 수령량",
  expectedAmountOut: "예상 수령량",
  sourceDex: "출발 거래소",
  destinationDex: "도착 거래소",
  triggerPrice: "발동 가격",
  limitPrice: "지정가",
  oraclePrice: "오라클 가격",
  currentPrice: "현재 가격",
  entryPrice: "진입 가격",
  healthFactor: "건전성 지표",
};

/** Spaced, lower-cased camelCase fallback for a segment with no dictionary
 *  entry, e.g. `reserveState` → "reserve state". Only the hidden long-tail
 *  reaches this. */
function camelWords(seg: string): string {
  return seg.replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase();
}

function koSegment(seg: string, isFirst: boolean): string {
  if (isFirst && PREFIX_KO[seg]) return PREFIX_KO[seg];
  if (LEAF_KO[seg]) return LEAF_KO[seg];
  return camelWords(seg);
}

/** Compose a plain-Korean label for a dotted path with no gloss entry.
 *  Drops `context.` and the `.key` plumbing node and joins the rest, so
 *  `context.tokenIn.key.address` → "입력 토큰 주소", `context.venue.pool`
 *  → "베뉴 풀". */
function composeLabel(path: string): string {
  const segs = path.split(".").slice(1).filter((s) => s !== "key");
  if (segs.length === 0) return path;
  return segs.map((s, i) => koSegment(s, i === 0)).join(" ");
}

/** True for engine-internal fields the form hides by default (Rule 3): nano
 *  mirrors, raw-hex amount strings, deep `.key`/state sub-fields, chain ids.
 *  A field stays prominent if it is glossed, a depth-1 primitive scalar, or a
 *  token-identity `<slot>.key.address`. */
function isAdvancedField(path: string, fieldKind: FieldKind, glossed: boolean): boolean {
  if (glossed) return false;
  const segs = path.split(".").slice(1); // drop "context"
  const leaf = segs[segs.length - 1] ?? "";
  // The token-identity address (what most token gates compare) stays prominent.
  if (/\.key\.address$/.test(path)) return false;
  // Bare record nodes (`.key`, state blobs) aren't comparable → hide.
  if (fieldKind === "record") return true;
  // Raw-hex amount strings (no ordering) — the nano/USD siblings are friendlier.
  if (fieldKind === "primitive.String" && /amount|^buyMin|^sellAmount|^netInput|^minOut/i.test(leaf)) return true;
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
    const leaf = f.path.split(".").pop()!;
    // nano mirror (Long named `*Nano`): present in plain token units, hide the
    // engine word "nano" entirely — the widget scales by 10⁹ under the hood.
    const isNano = f.fieldKind === "primitive.Long" && /Nano$/.test(leaf);
    const glossed = Boolean(g);
    const label = (g?.ko ?? composeLabel(f.path)).replace(/\s*\(\s*nano\s*\)/i, "").trim();
    // Safety net: never show a half-translated label (a leftover lowercase
    // English run like "gas estimate") in the visible list — demote it to the
    // "고급 필드" tray so what users see is always plain Korean.
    const hasEnglish = !glossed && /[a-z]{2,}/.test(label);
    out.push({
      path: f.path,
      label,
      fieldKind: f.fieldKind,
      role: g?.role ?? inferRole(f.path, f.fieldKind),
      source: "base",
      unit: isNano ? "토큰" : g?.unit?.ko,
      desc: g?.desc?.ko,
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
