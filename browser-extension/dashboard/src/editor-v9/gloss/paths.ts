/**
 * Human-readable glossary of well-known Cedar policy paths.
 *
 * Each entry maps a dotted attribute path (rooted at `context.*` / `meta.*` /
 * `enrichment.*`) to:
 *   - ko / en   display labels for the block + dropdown
 *   - role      coarse category that drives toolbox grouping + colour
 *   - fieldKind detailed type for future operator filtering (reserved)
 *   - desc      one-line plain-language hint for the tooltip
 *   - unit      optional unit suffix (USD / bp / 초)
 *
 * Phase G uses ko + role + desc + unit. fieldKind is reserved for later
 * operator-aware UX (filter `expr_binary` ops by LHS field's kind).
 *
 * Ported verbatim from editor-v7's V7_GLOSS — the 40 paths the original
 * Scratch-style builder had pinned for users.
 */

import { ENRICHMENT_FIELDS } from "../manifest-gen/registry";

/** Coarse category for toolbox layout + block colour. Mirrors V7_GLOSS.group. */
export type Role = "address" | "ref" | "numeric" | "enum" | "auth" | "derived";

/** Detailed type — `${primitive|ref|collection|record}.${cedarType}` for
 *  primitives. Reserved for operator filtering; not consumed in Phase G. */
export type FieldKind =
  | "primitive.String"
  | "primitive.Long"
  | "primitive.decimal"
  | "primitive.Bool"
  | "ref"
  | "collection"
  | "record";

export interface GlossEntry {
  /** Dotted path; doubles as the canonical id. e.g. `context.tokenIn.key.address`. */
  path: string;
  ko: string;
  en: string;
  role: Role;
  fieldKind: FieldKind;
  /** Plain-language one-liner shown under the label / as tooltip. */
  desc: { ko: string; en: string };
  unit?: { ko: string; en: string };
}

/**
 * Master gloss table. Order here drives the default toolbox order within
 * each role category. Keep alphabetical within a role for predictability,
 * with frequent-use entries lifted to the top.
 */
const BASE_GLOSS_ENTRIES: readonly GlossEntry[] = [
  // ── address (8) ──────────────────────────────────────────────────────
  { path: "context.recipient", ko: "수신자", en: "Recipient",
    role: "address", fieldKind: "primitive.String",
    desc: { ko: "거래로 토큰을 받는 주소", en: "Address that receives tokens from this tx" } },
  { path: "context.spender", ko: "지출 승인 대상", en: "Spender",
    role: "address", fieldKind: "primitive.String",
    desc: { ko: "내 토큰을 가져갈 권한을 얻는 컨트랙트", en: "Contract granted permission to pull my tokens" } },
  { path: "context.delegatee", ko: "위임 대상", en: "Delegatee",
    role: "address", fieldKind: "primitive.String",
    desc: { ko: "내 권한(거버넌스 투표 등)을 위임받는 주소", en: "Address receiving my delegated rights" } },
  { path: "context.onBehalfOf", ko: "대리 대상", en: "On behalf of",
    role: "address", fieldKind: "primitive.String",
    desc: { ko: "이 거래가 누구를 대신해 실행되는지", en: "Who this tx is executed on behalf of" } },
  { path: "context.contract", ko: "컨트랙트 주소", en: "Contract",
    role: "address", fieldKind: "primitive.String",
    desc: { ko: "거래 상대 컨트랙트의 주소", en: "Counterparty contract address" } },
  // `principal` 자체로 표현 가능 (expr_var) — 별도 field 블록 안 둠. V7의 `meta.from`은 여기로 대체.

  // ── ref — token / venue / market / platform (10) ─────────────────────
  { path: "context.venue", ko: "베뉴 (거래소/풀)", en: "Venue",
    role: "ref", fieldKind: "ref",
    desc: { ko: "어느 DEX/풀에서 거래하는지 (Uniswap V3, Curve 등)", en: "Which DEX/pool the trade happens on" } },
  { path: "context.token", ko: "토큰", en: "Token",
    role: "ref", fieldKind: "ref",
    desc: { ko: "거래 대상 토큰 (단일)", en: "The single token this action targets" } },
  { path: "context.tokenIn", ko: "입력 토큰", en: "Token in",
    role: "ref", fieldKind: "ref",
    desc: { ko: "스왑에서 내가 넣는 토큰 (예: USDC→ETH 면 USDC)", en: "Token I'm spending in a swap" } },
  { path: "context.tokenOut", ko: "출력 토큰", en: "Token out",
    role: "ref", fieldKind: "ref",
    desc: { ko: "스왑에서 받는 토큰 (예: USDC→ETH 면 ETH)", en: "Token I receive from a swap" } },
  { path: "context.asset", ko: "자산", en: "Asset",
    role: "ref", fieldKind: "ref",
    desc: { ko: "대출/예치/차입 대상 토큰", en: "Token being lent / supplied / borrowed" } },
  { path: "context.market", ko: "마켓", en: "Market",
    role: "ref", fieldKind: "ref",
    desc: { ko: "선물 마켓 (예: BTC-PERP)", en: "Perp market identifier" } },
  { path: "context.platform", ko: "플랫폼", en: "Platform",
    role: "ref", fieldKind: "ref",
    desc: { ko: "어느 프로토콜인지 (Aave, Compound, …)", en: "Which protocol" } },
  { path: "context.lpToken", ko: "LP 토큰", en: "LP token",
    role: "ref", fieldKind: "ref",
    desc: { ko: "유동성 공급 시 받는 영수증(LP) 토큰", en: "Receipt (LP) token from providing liquidity" } },
  { path: "context.nftKey", ko: "NFT 키", en: "NFT key",
    role: "ref", fieldKind: "ref",
    desc: { ko: "특정 NFT를 식별하는 키", en: "Key identifying a specific NFT" } },

  // ── numeric — amounts / slippage / size (11) ─────────────────────────
  { path: "context.amount", ko: "수량 (raw)", en: "Amount (raw)",
    role: "numeric", fieldKind: "primitive.String",
    desc: { ko: "토큰 단위 그대로의 거래량 (hex). USD 비교는 'amountUsd' 권장", en: "Token-unit amount (hex)" } },
  { path: "context.amountUsd", ko: "수량 (USD 환산)", en: "Amount (USD)",
    role: "numeric", fieldKind: "primitive.decimal",
    desc: { ko: "이번 거래의 달러 환산 가치", en: "USD-denominated value of the tx" },
    unit: { ko: "USD", en: "USD" } },
  { path: "context.slippageBp", ko: "슬리피지", en: "Slippage",
    role: "numeric", fieldKind: "primitive.Long",
    desc: { ko: "허용하는 최대 슬리피지 (100bp = 1%)", en: "Max slippage allowed (100bp = 1%)" },
    unit: { ko: "bp", en: "bp" } },
  { path: "context.priceImpactBp", ko: "프라이스 임팩트", en: "Price impact",
    role: "numeric", fieldKind: "primitive.Long",
    desc: { ko: "이 거래가 시장 가격을 얼마나 움직이는지 (100bp = 1%)", en: "Price-impact of this tx (100bp = 1%)" },
    unit: { ko: "bp", en: "bp" } },
  { path: "context.minAmountOut", ko: "최소 출력 수량", en: "Min amount out",
    role: "numeric", fieldKind: "primitive.String",
    desc: { ko: "스왑에서 최소한 받아야 하는 출력 토큰 양", en: "Floor on output tokens to receive" } },
  { path: "context.maxAmountIn", ko: "최대 입력 수량", en: "Max amount in",
    role: "numeric", fieldKind: "primitive.String",
    desc: { ko: "스왑에서 최대로 쓸 수 있는 입력 토큰 양", en: "Cap on input tokens to spend" } },
  { path: "context.minLpOut", ko: "최소 LP 수령", en: "Min LP out",
    role: "numeric", fieldKind: "primitive.String",
    desc: { ko: "유동성 공급 후 최소한 받아야 하는 LP 토큰 양", en: "Floor on LP tokens received" } },
  { path: "context.amountDesired", ko: "희망 수량", en: "Amount desired",
    role: "numeric", fieldKind: "record",
    desc: { ko: "풀 입금 시 원하는 두 토큰의 양 (a/b)", en: "Desired (a, b) amounts when adding liquidity" } },
  { path: "context.maxLeverage", ko: "최대 레버리지", en: "Max leverage",
    role: "numeric", fieldKind: "primitive.String",
    desc: { ko: "선물 포지션의 최대 허용 레버리지", en: "Max leverage allowed on a perp position" } },
  { path: "context.markPrice", ko: "마크 가격", en: "Mark price",
    role: "numeric", fieldKind: "primitive.String",
    desc: { ko: "선물 마켓의 마크 가격 (청산 기준)", en: "Mark price (perp liquidation reference)" } },
  { path: "context.size", ko: "포지션 크기", en: "Size",
    role: "numeric", fieldKind: "ref",
    desc: { ko: "선물 포지션 크기 (단위 포함)", en: "Perp position size with units" } },
  { path: "context.sellAmount", ko: "매도 수량", en: "Sell amount",
    role: "numeric", fieldKind: "primitive.String",
    desc: { ko: "Intent(주문) 에서 팔려는 토큰 양", en: "Amount to sell in an intent order" } },
  { path: "context.buyMin", ko: "최소 매수량", en: "Buy min",
    role: "numeric", fieldKind: "primitive.String",
    desc: { ko: "Intent(주문) 에서 최소로 사야 하는 양", en: "Minimum to buy in an intent order" } },

  // ── enum — direction / side / mode (5) ───────────────────────────────
  { path: "context.direction.kind", ko: "스왑 방향", en: "Swap direction",
    role: "enum", fieldKind: "primitive.String",
    desc: { ko: "exact_input(입력 고정) vs exact_output(출력 고정)", en: "exact_input vs exact_output" } },
  { path: "context.rateMode", ko: "금리 모드", en: "Rate mode",
    role: "enum", fieldKind: "primitive.String",
    desc: { ko: "변동금리 vs 고정금리 (Aave 등 대출 시)", en: "Variable vs stable rate" } },
  { path: "context.side", ko: "방향 (롱/숏)", en: "Side",
    role: "enum", fieldKind: "primitive.String",
    desc: { ko: "선물 포지션 방향: 롱 또는 숏", en: "Perp side: long or short" } },
  { path: "context.orderKind", ko: "주문 종류", en: "Order kind",
    role: "enum", fieldKind: "primitive.String",
    desc: { ko: "Intent 주문 타입 (limit 등)", en: "Intent order type" } },
  { path: "context.reduceOnly", ko: "감소 전용", en: "Reduce only",
    role: "enum", fieldKind: "primitive.Bool",
    desc: { ko: "기존 포지션을 줄이기만 하는 주문인지", en: "Order can only reduce an existing position" } },

  // ── auth — proof / position id (2) ───────────────────────────────────
  { path: "context.proof", ko: "머클 증명", en: "Merkle proof",
    role: "auth", fieldKind: "collection",
    desc: { ko: "에어드롭/화이트리스트 자격 증명용 머클 proof", en: "Merkle proof for eligibility" } },
  { path: "context.positionId", ko: "포지션 ID", en: "Position ID",
    role: "auth", fieldKind: "primitive.String",
    desc: { ko: "선물 포지션 고유 ID", en: "Unique id for a perp position" } },

  // ── derived — host-populated, all routed via context.enrichment.* so
  //    Cedar text round-trip stays clean (root must be a valid request var).
  { path: "context.enrichment.validityDeltaSec", ko: "마감까지 남은 시간", en: "Time to deadline",
    role: "derived", fieldKind: "primitive.Long",
    desc: { ko: "이 거래가 만료되기까지 남은 초 (deadline까지 시간)", en: "Seconds until the tx deadline" },
    unit: { ko: "초", en: "sec" } },
  { path: "context.enrichment.recipientIsContract", ko: "수신자가 컨트랙트", en: "Recipient is contract",
    role: "derived", fieldKind: "primitive.Bool",
    desc: { ko: "수신자 주소가 컨트랙트인가? (EOA가 아닌 경우 true)", en: "Is the recipient a contract" } },
  { path: "context.enrichment.totalInputUsd", ko: "입력 가치 (USD)", en: "Input value (USD)",
    role: "derived", fieldKind: "primitive.decimal",
    desc: { ko: "이번 거래로 빠져나가는 총 가치(달러 환산)", en: "Total USD value leaving my wallet" },
    unit: { ko: "USD", en: "USD" } },
  { path: "context.enrichment.effectiveRateVsOracleBps", ko: "오라클 대비 슬리피지", en: "Slippage vs oracle",
    role: "derived", fieldKind: "primitive.Long",
    desc: { ko: "실제 거래 가격이 오라클 가격에서 얼마나 벗어나는지 (bp)", en: "Effective rate divergence from oracle (bp)" },
    unit: { ko: "bp", en: "bp" } },
  { path: "context.expectedAmountOut", ko: "예상 출력", en: "Expected out",
    role: "derived", fieldKind: "primitive.String",
    desc: { ko: "실행 전 견적된 출력 토큰 양", en: "Pre-execution quote of output tokens" } },
];

/** custom_context type spelling → editor `FieldKind`. */
const ENRICHMENT_FIELD_KIND: Record<string, FieldKind> = {
  decimal: "primitive.decimal",
  Long: "primitive.Long",
  Bool: "primitive.Bool",
  String: "primitive.String",
};

/**
 * Enrichment fields surfaced as palette blocks, DERIVED from the manifest
 * generator's registry so the field list and the auto-generated manifest share
 * a single source of truth. The path is the real `context.custom.<field>` the
 * generator detects and the engine reads — drop the block, write a threshold,
 * save, and the manifest that fills it is generated automatically.
 */
const ENRICHMENT_GLOSS: readonly GlossEntry[] = Object.entries(ENRICHMENT_FIELDS).map(
  ([field, def]) => ({
    path: `context.custom.${field}`,
    ko: def.label.ko,
    en: def.label.en,
    role: "derived" as const,
    fieldKind: ENRICHMENT_FIELD_KIND[def.type] ?? "primitive.String",
    desc: {
      ko: def.note ?? "정책 저장 시 manifest가 자동 생성되어 채워지는 보강 값.",
      en: def.note ?? "Auto-enriched by a generated manifest on save.",
    },
  }),
);

/** All gloss entries: the base table plus registry-derived enrichment fields. */
export const GLOSS_ENTRIES: readonly GlossEntry[] = [
  ...BASE_GLOSS_ENTRIES,
  ...ENRICHMENT_GLOSS,
];

/** Lookup by dotted path. O(1) via the materialised map below. */
const GLOSS_INDEX: Map<string, GlossEntry> = new Map(
  GLOSS_ENTRIES.map((e) => [e.path, e] as const),
);

export function getGloss(path: string): GlossEntry | undefined {
  return GLOSS_INDEX.get(path);
}

export function allGloss(): readonly GlossEntry[] {
  return GLOSS_ENTRIES;
}

/** Group entries by role, preserving GLOSS_ENTRIES order within each group. */
export function glossByRole(): Record<Role, GlossEntry[]> {
  const out: Record<Role, GlossEntry[]> = {
    address: [],
    ref: [],
    numeric: [],
    enum: [],
    auth: [],
    derived: [],
  };
  for (const e of GLOSS_ENTRIES) out[e.role].push(e);
  return out;
}

/** Block-type id for a given dotted path. Stable transformation:
 *  `context.tokenIn.key.address` → `field_context_tokenIn_key_address`.
 *  Path keeps its camelCase; only the dots become underscores. */
export function blockTypeForPath(path: string): string {
  return "field_" + path.replace(/\./g, "_");
}

/** Inverse — recover the dotted path from a field block type id. Returns
 *  null if the prefix isn't `field_` or the path isn't in the gloss. */
export function pathForBlockType(blockType: string): string | null {
  if (!blockType.startsWith("field_")) return null;
  const path = blockType.slice("field_".length).replace(/_/g, ".");
  return GLOSS_INDEX.has(path) ? path : null;
}

/** Colour per role — matches the toolbox category colour so the block reads
 *  as "from that bucket" the moment it lands on the canvas. */
export const ROLE_COLOUR: Record<Role, number> = {
  address: 30,
  ref: 230,
  numeric: 60,
  enum: 120,
  auth: 0,
  derived: 290,
};

/** Korean role names for toolbox category labels. */
export const ROLE_LABEL_KO: Record<Role, string> = {
  address: "주소",
  ref: "토큰·베뉴",
  numeric: "금액·수량",
  enum: "방향·주문",
  auth: "인증·포지션",
  derived: "파생값",
};

export const ROLE_LABEL_EN: Record<Role, string> = {
  address: "Address",
  ref: "Token & Venue",
  numeric: "Amount",
  enum: "Direction & Order",
  auth: "Auth & Position",
  derived: "Derived",
};
