/**
 * V7 schema — ported verbatim from `front/scopeball-v3/editor-v7-data.js`'s
 * `V7_GLOSS`, with one addition per entry:
 *   `supported: boolean` — true iff our `policy-schema.json` (12 keys
 *   today) carries this field. Unsupported entries still render in the
 *   palette so the original designer intent is preserved, but the
 *   builder marks them with a "schema 미등록" warning and the compile
 *   pipeline refuses to save until they're either added to our schema
 *   or dropped from the doc.
 *
 * To re-sync: copy the latest `V7_GLOSS` object from editor-v7-data.js,
 * paste it into the `GLOSS` const below, then run the audit unit test
 * which diffs the keys against `/policy-schema.json`.
 */

// ── role / fieldKind / op type aliases ─────────────────────────────────

export type Role = "numeric" | "address" | "ref" | "enum" | "auth" | "derived" | "misc";

/** Stored on each predicate so the inspector knows which operator set
 *  applies. Mirrors V7's `fk` flag but spelled out. */
export type FieldKind =
  | "primitive.String"
  | "primitive.Long"
  | "primitive.decimal"
  | "primitive.Bool"
  | "ref"
  | "collection"
  | "record";

export type Op =
  | "eq" | "neq" | "lt" | "lte" | "gt" | "gte"
  | "in" | "notIn"
  | "startsWith" | "contains"
  | "isTrue" | "isFalse"
  | "containsAny" | "containsAll" | "isEmpty" | "sizeEq" | "sizeGt" | "sizeLt";

/** Predicate value as carried on a node. `kind` discriminates between
 *  literal scalars and dynamic refs (`@meta.from` etc.) the compiler
 *  resolves at evaluation time. */
export type PredicateValue =
  | { kind: "str"; text: string; unit?: string }
  | { kind: "num"; text: string; unit?: string }
  | { kind: "bool"; text: "true" | "false" }
  | { kind: "ref"; text: string };

export interface GlossEntry {
  /** English display label (palette item title). */
  en: string;
  /** Korean display label. */
  ko: string;
  group: Role;
  /** Coarse type (primitive | ref | collection | record). The fine
   *  `FieldKind` is `${fk}.${type}` for primitives. */
  fk: "primitive" | "ref" | "collection" | "record";
  type: string;
  /** True iff the value is host-derived (enrichment, oracle, etc.)
   *  — affects default `absence` behavior on the predicate node. */
  derived: boolean;
  /** Internal annotation — short tag like "Perp" or "27개 액션 공통".
   *  Not shown to end-users; survives for backwards compatibility. */
  note?: string;
  /** Plain-language one-liner shown under the label in the palette so
   *  the user doesn't have to know what `context.amountUsd` means. */
  desc: { ko: string; en: string };
  unit?: { ko: string; en: string };
}

// ── V7_GLOSS verbatim from editor-v7-data.js ───────────────────────────

const GLOSS: Record<string, GlossEntry> = {
  "context.recipient":                  { en: "Recipient",              ko: "수신자",                       group: "address", fk: "primitive",  type: "String",                derived: false, note: "출력/수령 주소",
    desc: { ko: "거래로 토큰을 받는 주소",                                         en: "Address that receives tokens from this tx" } },
  "context.spender":                    { en: "Spender",                ko: "지출 승인 대상(spender)",      group: "address", fk: "primitive",  type: "String",                derived: false, note: "approve 받는 컨트랙트",
    desc: { ko: "내 토큰을 가져갈 권한을 얻는 컨트랙트",                            en: "Contract granted permission to pull my tokens" } },
  "context.delegatee":                  { en: "Delegatee",              ko: "위임 대상",                    group: "address", fk: "primitive",  type: "String",                derived: false,
    desc: { ko: "내 권한(거버넌스 투표 등)을 위임받는 주소",                        en: "Address receiving my delegated rights (e.g. voting)" } },
  "context.onBehalfOf":                 { en: "On behalf of",           ko: "대리 대상(onBehalfOf)",        group: "address", fk: "primitive",  type: "String",                derived: false, note: "제3자 대리 실행",
    desc: { ko: "이 거래가 누구를 대신해 실행되는지",                                en: "Who this tx is executed on behalf of" } },
  "context.contract":                   { en: "Contract",               ko: "컨트랙트 주소",                group: "address", fk: "primitive",  type: "String",                derived: false,
    desc: { ko: "거래 상대 컨트랙트의 주소",                                        en: "Counterparty contract address" } },
  "meta.from":                          { en: "From (sender)",          ko: "보낸 지갑",                    group: "address", fk: "primitive",  type: "String",                derived: false, note: "principal. 동적참조 @meta.from",
    desc: { ko: "이 거래를 보내는 내 지갑 주소",                                    en: "My wallet address sending this tx" } },
  "context.venue":                      { en: "Venue",                  ko: "베뉴(거래소/풀)",              group: "ref",     fk: "ref",        type: "AmmVenue",              derived: false, note: "27개 액션 공통",
    desc: { ko: "어느 DEX/풀에서 거래하는지 (Uniswap V3, Curve 등)",                en: "Which DEX/pool the trade happens on (Uniswap V3, Curve, …)" } },
  "context.token":                      { en: "Token",                  ko: "토큰",                         group: "ref",     fk: "ref",        type: "Core::TokenRef",        derived: false,
    desc: { ko: "거래 대상 토큰 (단일)",                                            en: "The single token this action targets" } },
  "context.tokenIn":                    { en: "Token in",               ko: "입력 토큰",                    group: "ref",     fk: "ref",        type: "Core::TokenRef",        derived: false,
    desc: { ko: "스왑에서 내가 넣는 토큰 (예: USDC→ETH 면 USDC)",                   en: "Token I'm spending in a swap (e.g. USDC in USDC→ETH)" } },
  "context.tokenOut":                   { en: "Token out",              ko: "출력 토큰",                    group: "ref",     fk: "ref",        type: "Core::TokenRef",        derived: false,
    desc: { ko: "스왑에서 받는 토큰 (예: USDC→ETH 면 ETH)",                         en: "Token I receive from a swap (e.g. ETH in USDC→ETH)" } },
  "context.asset":                      { en: "Asset",                  ko: "자산",                         group: "ref",     fk: "ref",        type: "Core::TokenRef",        derived: false, note: "대출 등 대상 토큰",
    desc: { ko: "대출/예치/차입 대상 토큰",                                          en: "Token being lent / supplied / borrowed" } },
  "context.market":                     { en: "Market",                 ko: "마켓",                         group: "ref",     fk: "ref",        type: "MarketRef",             derived: false, note: "Perp 마켓",
    desc: { ko: "선물 마켓 (예: BTC-PERP)",                                          en: "Perp market identifier (e.g. BTC-PERP)" } },
  "context.platform":                   { en: "Platform",               ko: "플랫폼",                       group: "ref",     fk: "ref",        type: "Core::ProtocolRef",     derived: false,
    desc: { ko: "어느 프로토콜인지 (Aave, Compound, …)",                              en: "Which protocol (Aave, Compound, …)" } },
  "context.lpToken":                    { en: "LP token",               ko: "LP 토큰",                      group: "ref",     fk: "ref",        type: "Core::TokenRef",        derived: false,
    desc: { ko: "유동성 공급 시 받는 영수증(LP) 토큰",                                en: "Receipt (LP) token from providing liquidity" } },
  "context.nftKey":                     { en: "NFT key",                ko: "NFT 키",                       group: "ref",     fk: "ref",        type: "Core::TokenKey",        derived: false,
    desc: { ko: "특정 NFT를 식별하는 키",                                            en: "Key identifying a specific NFT" } },
  "context.amount":                     { en: "Amount (raw)",           ko: "수량(raw)",                    group: "numeric", fk: "primitive",  type: "String",                derived: false, note: "14개 액션 공통",
    desc: { ko: "토큰 단위 그대로의 거래량 (hex). USD 비교는 'amountUsd' 권장",       en: "Token-unit amount (hex). Prefer 'amountUsd' for USD comparisons" } },
  "context.amountUsd":                  { en: "Amount (USD)",           ko: "수량(USD 환산)",               group: "numeric", fk: "primitive",  type: "decimal",               derived: true,  note: "파생 USD",
    desc: { ko: "이번 거래의 달러 환산 가치",                                        en: "USD-denominated value of the tx" },
    unit: { en: "USD", ko: "USD" } },
  "context.slippageBp":                 { en: "Slippage",               ko: "슬리피지",                     group: "numeric", fk: "primitive",  type: "Long",                  derived: false, note: "허용 슬리피지",
    desc: { ko: "허용하는 최대 슬리피지 (100bp = 1%)",                                en: "Max slippage allowed (100bp = 1%)" },
    unit: { en: "bp",  ko: "bp"  } },
  "context.priceImpactBp":              { en: "Price impact",           ko: "프라이스 임팩트",              group: "numeric", fk: "primitive",  type: "Long",                  derived: true,
    desc: { ko: "이 거래가 시장 가격을 얼마나 움직이는지 (100bp = 1%)",                en: "How much this tx moves the market price (100bp = 1%)" },
    unit: { en: "bp",  ko: "bp"  } },
  "context.minAmountOut":               { en: "Min amount out",         ko: "최소 출력 수량",               group: "numeric", fk: "primitive",  type: "String",                derived: false, note: "exact_input 하한",
    desc: { ko: "스왑에서 최소한 받아야 하는 출력 토큰 양",                          en: "Floor on output tokens to receive" } },
  "context.maxAmountIn":                { en: "Max amount in",          ko: "최대 입력 수량",               group: "numeric", fk: "primitive",  type: "String",                derived: false, note: "exact_output 상한",
    desc: { ko: "스왑에서 최대로 쓸 수 있는 입력 토큰 양",                            en: "Cap on input tokens to spend" } },
  "context.minLpOut":                   { en: "Min LP out",             ko: "최소 LP 수령",                 group: "numeric", fk: "primitive",  type: "String",                derived: false,
    desc: { ko: "유동성 공급 후 최소한 받아야 하는 LP 토큰 양",                       en: "Floor on LP tokens received after providing liquidity" } },
  "context.amountDesired":              { en: "Amount desired",         ko: "희망 수량",                    group: "numeric", fk: "record",     type: "{ a: String, b: String }", derived: false,
    desc: { ko: "풀 입금 시 원하는 두 토큰의 양 (a/b)",                                en: "Desired (a, b) amounts when adding liquidity" } },
  "context.maxLeverage":                { en: "Max leverage",           ko: "최대 레버리지",                group: "numeric", fk: "primitive",  type: "String",                derived: false, note: "Perp",
    desc: { ko: "선물 포지션의 최대 허용 레버리지",                                   en: "Max leverage allowed on a perp position" } },
  "context.markPrice":                  { en: "Mark price",             ko: "마크 가격",                    group: "numeric", fk: "primitive",  type: "String",                derived: false,
    desc: { ko: "선물 마켓의 마크 가격 (청산 기준)",                                  en: "Mark price (perp liquidation reference)" } },
  "context.size":                       { en: "Size",                   ko: "포지션 크기",                  group: "numeric", fk: "ref",        type: "SizeSpec",              derived: false,
    desc: { ko: "선물 포지션 크기 (단위 포함)",                                       en: "Perp position size with units" } },
  "context.sellAmount":                 { en: "Sell amount",            ko: "매도 수량",                    group: "numeric", fk: "primitive",  type: "String",                derived: false, note: "Intent",
    desc: { ko: "Intent(주문) 에서 팔려는 토큰 양",                                    en: "Amount to sell in an intent order" } },
  "context.buyMin":                     { en: "Buy min",                ko: "최소 매수량",                  group: "numeric", fk: "primitive",  type: "String",                derived: false, note: "Intent",
    desc: { ko: "Intent(주문) 에서 최소로 사야 하는 양",                                en: "Minimum to buy in an intent order" } },
  "context.direction.kind":             { en: "Swap direction",         ko: "스왑 방향",                    group: "enum",    fk: "primitive",  type: "String",                derived: false, note: "exact_input · exact_output",
    desc: { ko: "exact_input(입력 고정) vs exact_output(출력 고정)",                  en: "exact_input vs exact_output" } },
  "context.rateMode":                   { en: "Rate mode",              ko: "금리 모드",                    group: "enum",    fk: "primitive",  type: "String",                derived: false, note: "고정/변동",
    desc: { ko: "변동금리 vs 고정금리 (Aave 등 대출 시)",                              en: "Variable vs stable rate (e.g. Aave borrow)" } },
  "context.side":                       { en: "Side",                   ko: "방향(롱/숏)",                  group: "enum",    fk: "primitive",  type: "String",                derived: false, note: "Perp",
    desc: { ko: "선물 포지션 방향: 롱 또는 숏",                                       en: "Perp side: long or short" } },
  "context.orderKind":                  { en: "Order kind",             ko: "주문 종류",                    group: "enum",    fk: "primitive",  type: "String",                derived: false, note: "Intent",
    desc: { ko: "Intent 주문 타입 (limit 등)",                                          en: "Intent order type (limit, …)" } },
  "context.reduceOnly":                 { en: "Reduce only",            ko: "감소 전용(reduceOnly)",        group: "enum",    fk: "primitive",  type: "Bool",                  derived: false,
    desc: { ko: "기존 포지션을 줄이기만 하는 주문인지",                                en: "Order can only reduce an existing position" } },
  "context.proof":                      { en: "Merkle proof",           ko: "머클 증명(proof)",             group: "auth",    fk: "collection", type: "Set<String>",           derived: false,
    desc: { ko: "에어드롭/화이트리스트 자격 증명용 머클 proof",                       en: "Merkle proof used to prove eligibility (airdrop, allowlist)" } },
  "context.positionId":                 { en: "Position ID",            ko: "포지션 ID",                    group: "auth",    fk: "primitive",  type: "String",                derived: false, note: "Perp",
    desc: { ko: "선물 포지션 고유 ID",                                                  en: "Unique id for a perp position" } },
  "enrichment.validityDeltaSec":        { en: "Time to deadline",       ko: "마감까지 남은 시간",           group: "derived", fk: "primitive",  type: "Long",                  derived: true,  note: "Host-derived",
    desc: { ko: "이 거래가 만료되기까지 남은 초 (deadline까지 시간)",                  en: "Seconds until the tx deadline expires" },
    unit: { en: "sec", ko: "초"  } },
  "enrichment.recipientIsContract":     { en: "Recipient is contract",  ko: "수신자가 컨트랙트",            group: "derived", fk: "primitive",  type: "Bool",                  derived: true,  note: "Bool, Host-derived",
    desc: { ko: "수신자 주소가 컨트랙트인가? (EOA가 아닌 경우 true)",                  en: "Is the recipient a contract (vs. an EOA)?" } },
  "enrichment.totalInputUsd":           { en: "Input value (USD)",      ko: "입력 가치(USD)",               group: "derived", fk: "primitive",  type: "decimal",               derived: true,  note: "Host-populated",
    desc: { ko: "이번 거래로 빠져나가는 총 가치(달러 환산)",                            en: "Total USD value leaving my wallet in this tx" },
    unit: { en: "USD", ko: "USD" } },
  "enrichment.effectiveRateVsOracleBps":{ en: "Slippage vs oracle",     ko: "오라클 대비 슬리피지",         group: "derived", fk: "primitive",  type: "Long",                  derived: true,  note: "oracle",
    desc: { ko: "실제 거래 가격이 오라클 가격에서 얼마나 벗어나는지 (bp)",             en: "How far the effective rate diverges from the oracle (bp)" },
    unit: { en: "bp",  ko: "bp"  } },
  "context.expectedAmountOut":          { en: "Expected out",           ko: "예상 출력",                    group: "derived", fk: "primitive",  type: "String",                derived: true,  note: "LiveField",
    desc: { ko: "실행 전 견적된 출력 토큰 양",                                          en: "Pre-execution quote of output tokens" } },
};

// ── audit: which params our server-side policy-schema.json supports ────
//
// Sourced 1:1 from `crates/simulation/server/static/policy-schema.json`
// (12 predicates at the time of this port). Update by re-running the
// audit unit test against `getPolicySchema()` when the server adds new
// fields.
const SUPPORTED_PARAMS = new Set<string>([
  "context.tokenIn",
  "context.tokenOut",
  "context.recipient",
  "context.slippageBp",
  "context.priceImpactBp",
  "context.amount",
  "meta.chainId",
  "meta.from",
  "meta.to",
  "enrichment.totalInputUsd",
  "enrichment.recipientIsContract",
  "enrichment.effectiveRateVsOracleBps",
]);

// ── per-fieldKind operator catalog ─────────────────────────────────────

export const OPS_BY_FIELDKIND: Record<FieldKind, Op[]> = {
  "primitive.String":  ["eq", "neq", "in", "notIn", "startsWith", "contains"],
  "primitive.Long":    ["eq", "neq", "lt", "lte", "gt", "gte"],
  "primitive.decimal": ["eq", "neq", "lt", "lte", "gt", "gte"],
  "primitive.Bool":    ["isTrue", "isFalse"],
  "ref":               ["eq", "neq", "in", "notIn"],
  "collection":        ["contains", "containsAny", "containsAll", "isEmpty", "sizeEq", "sizeGt", "sizeLt"],
  "record":            [],
};

export const OP_SYMBOL: Record<Op, string> = {
  eq: "==",  neq: "≠",  lt: "<",  lte: "≤",  gt: ">",  gte: "≥",
  in: "∈",   notIn: "∉",
  startsWith: "starts", contains: "has",
  isTrue: "= 참", isFalse: "= 거짓",
  containsAny: "⊇any", containsAll: "⊇all",
  isEmpty: "empty", sizeEq: "#=", sizeGt: "#>", sizeLt: "#<",
};

export const ROLES: Record<Role, { en: string; ko: string; tone: string; icon: string }> = {
  numeric: { en: "Number · limit", ko: "수량·한도", tone: "slate", icon: "hash"   },
  address: { en: "Address",         ko: "주체·주소",  tone: "cyan",  icon: "key"    },
  ref:     { en: "Selection",       ko: "대상 선택",  tone: "sage",  icon: "token"  },
  enum:    { en: "Mode",            ko: "모드·열거",  tone: "slate", icon: "switch" },
  auth:    { en: "Auth · time",     ko: "서명·시간",  tone: "cyan",  icon: "clock"  },
  derived: { en: "Derived",         ko: "파생",       tone: "sage",  icon: "spark"  },
  misc:    { en: "Other",           ko: "그 외",      tone: "slate", icon: "dot"    },
};

// ── public helpers ─────────────────────────────────────────────────────

export function getGlossEntry(param: string): GlossEntry | undefined {
  return GLOSS[param];
}

export function allGlossKeys(): string[] {
  return Object.keys(GLOSS);
}

/** All `(param, GlossEntry)` pairs grouped by their role. Used by the
 *  palette to render six collapsible sections. */
export function glossByRole(): Record<Role, Array<{ param: string; entry: GlossEntry }>> {
  const out: Record<Role, Array<{ param: string; entry: GlossEntry }>> = {
    numeric: [], address: [], ref: [], enum: [], auth: [], derived: [], misc: [],
  };
  for (const [param, entry] of Object.entries(GLOSS)) {
    out[entry.group].push({ param, entry });
  }
  return out;
}

/** Resolve the precise `FieldKind` (primitive.String, ref, …) for a
 *  param. Falls back to `primitive.String` when the param is unknown
 *  to the palette (user typed a custom param string in the inspector). */
export function fieldKindOf(param: string): FieldKind {
  const entry = GLOSS[param];
  if (!entry) return "primitive.String";
  switch (entry.fk) {
    case "primitive": return `primitive.${entry.type}` as FieldKind;
    case "ref":       return "ref";
    case "collection":return "collection";
    case "record":    return "record";
  }
}

/** Localized display label for the palette + canvas. */
export function displayParam(param: string, locale: "ko" | "en"): string {
  const entry = GLOSS[param];
  if (entry) return locale === "ko" ? entry.ko : entry.en;
  // Best-effort prettify when the param isn't in V7_GLOSS:
  // `context.someCamelField` → "Some camel field"
  const leaf = param.split(".").pop() ?? param;
  return leaf
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/^./, (c) => c.toUpperCase());
}

/** Whether the host runtime is expected to populate this value
 *  (enrichment fields, oracle-backed live fields). Drives the
 *  default `absence: "treatAsFalse"` flag on predicates so a missing
 *  oracle reading doesn't accidentally block the user. */
export function isLiveField(param: string): boolean {
  const entry = GLOSS[param];
  if (entry) return entry.derived;
  return /^enrichment\./.test(param);
}

/** Whether our server-side `policy-schema.json` knows about this
 *  param. Unsupported params still render in the palette but the
 *  builder marks them with a warning and refuses to compile. */
export function isSupportedParam(param: string): boolean {
  return SUPPORTED_PARAMS.has(param);
}
