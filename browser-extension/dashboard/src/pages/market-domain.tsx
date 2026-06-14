/**
 * Domain metadata + visual primitives reused across the market browse and
 * detail pages. The palette is the original Cloudy Pond scheme: three color
 * families (Cyan = trading, Sage = safety/holding, Slate = assets/infra),
 * each containing four domains at varying lightness so a card's family is
 * recognizable at a glance.
 *
 * SVG icon paths are kept literal (24x24 viewBox, no fill); render with
 * `<DomainGlyph domain="swap" size={16} />`.
 */

export type DomainKey =
  | "swap" | "perp" | "ammlp" | "bridge"
  | "security" | "portfolio" | "staking" | "airdrop"
  | "lending" | "nft" | "sale" | "gov";

export type ColorFamily = "cyan" | "sage" | "slate" | "gold";

export interface DomainColor {
  family: ColorFamily;
  hex: string;
  soft: string;
  ink: string;
}

export const DOMAIN_ORDER: DomainKey[] = [
  "security", "swap", "perp", "lending", "nft", "airdrop",
  "portfolio", "ammlp", "bridge", "sale", "staking", "gov",
];

export const DOMAIN_COLOR: Record<DomainKey, DomainColor> = {
  // Cyan family — trading
  swap:      { family: "cyan",  hex: "#688186", soft: "#DCEAED", ink: "#2B3639" },
  perp:      { family: "cyan",  hex: "#485A5E", soft: "#CAE0E4", ink: "#2B3639" },
  ammlp:     { family: "cyan",  hex: "#85A4AB", soft: "#EDF4F6", ink: "#2B3639" },
  bridge:    { family: "cyan",  hex: "#A4C9D1", soft: "#EDF4F6", ink: "#485A5E" },
  // Sage family — safety / holding
  security:  { family: "sage",  hex: "#637E59", soft: "#EBF3E8", ink: "#283523" },
  portfolio: { family: "sage",  hex: "#7FA172", soft: "#EBF3E8", ink: "#283523" },
  staking:   { family: "sage",  hex: "#9CC58D", soft: "#F8F9F6", ink: "#44583D" },
  airdrop:   { family: "sage",  hex: "#44583D", soft: "#D9E9D3", ink: "#283523" },
  // Slate family — assets / infra
  lending:   { family: "slate", hex: "#384455", soft: "#D7DBDF", ink: "#0D1118" },
  nft:       { family: "slate", hex: "#697485", soft: "#EFF0F2", ink: "#1B222C" },
  sale:      { family: "slate", hex: "#2A3441", soft: "#D7DBDF", ink: "#0D1118" },
  gov:       { family: "slate", hex: "#9099A5", soft: "#EFF0F2", ink: "#2A3441" },
};

export const DOMAIN_ICON: Record<DomainKey, string> = {
  swap:      "M7 7h11l-3-3M17 17H6l3 3",
  perp:      "M3 17l5-6 4 3 5-7 4 4",
  lending:   "M3 10h18M5 10v8h14v-8M9 14h6",
  security:  "M12 3l7 3v5c0 4-3 7-7 9-4-2-7-5-7-9V6z",
  nft:       "M4 4h16v16H4zM8 10a1.5 1.5 0 100-3 1.5 1.5 0 000 3M4 16l5-4 4 3 3-2 4 3",
  airdrop:   "M12 3a6 6 0 016 6c0 3-6 9-6 9S6 12 6 9a6 6 0 016-6M12 21v-3",
  portfolio: "M21 12a9 9 0 11-9-9v9z",
  ammlp:     "M12 3c3 4 6 7 6 10a6 6 0 01-12 0c0-3 3-6 6-10z",
  bridge:    "M3 16c0-4 3-7 9-7s9 3 9 7M3 16v3M21 16v3M8 13v6M16 13v6",
  sale:      "M4 8l8-4 8 4-8 4zM4 8v8l8 4 8-4V8",
  staking:   "M4 18h16M6 18V9M10 18V6M14 18V11M18 18V8",
  gov:       "M5 21h14M6 21V9M18 21V9M4 9l8-5 8 5M9 13v4M15 13v4",
};

export const DOMAIN_NAME: Record<DomainKey, { en: string; ko: string }> = {
  swap:      { en: "Swap & DEX",            ko: "스왑 & DEX" },
  perp:      { en: "Perps & Derivatives",   ko: "파생/무기한" },
  lending:   { en: "Lending",               ko: "렌딩" },
  security:  { en: "Wallet Security Core",  ko: "지갑 보안 기본" },
  nft:       { en: "NFT",                   ko: "NFT" },
  airdrop:   { en: "Airdrop & Claim",       ko: "에어드랍 & 클레임" },
  portfolio: { en: "Portfolio & Self-control", ko: "포트폴리오 & 자기관리" },
  ammlp:     { en: "AMM Liquidity",         ko: "AMM 유동성" },
  bridge:    { en: "Bridge",                ko: "브릿지" },
  sale:      { en: "Launchpad & Sale",      ko: "런치패드 & 세일" },
  staking:   { en: "Staking & LST",         ko: "스테이킹 & LST" },
  gov:       { en: "Governance",            ko: "거버넌스" },
};

export function isDomainKey(s: string | undefined | null): s is DomainKey {
  return !!s && s in DOMAIN_COLOR;
}

export function domainNameOf(d: string | undefined, locale: "en" | "ko"): string {
  if (isDomainKey(d)) return DOMAIN_NAME[d][locale];
  return d ?? "";
}

export function colorOf(d: string | undefined): DomainColor | null {
  return isDomainKey(d) ? DOMAIN_COLOR[d] : null;
}

interface DomainGlyphProps {
  domain: string | undefined;
  size?: number;
  /** Override stroke color. Defaults to the domain's family `hex`. */
  color?: string;
  className?: string;
}

/**
 * 24x24 line glyph for a domain. Returns null when `domain` is missing or
 * isn't one of the 12 known keys, so callers can render it unconditionally.
 */
export function DomainGlyph({ domain, size = 16, color, className }: DomainGlyphProps) {
  if (!isDomainKey(domain)) return null;
  const stroke = color ?? DOMAIN_COLOR[domain].hex;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={stroke}
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <path d={DOMAIN_ICON[domain]} />
    </svg>
  );
}

// ─────────────────────────────────────────────────────────────────────────
// Categories — action/intent taxonomy for the landing's category grid.
//
// Distinct from `domain` (which is protocol-flavoured and drives card colour):
// a category answers "what kind of action does this policy guard?", derived
// from the policy manifest's `trigger.action.tag`. Until the server persists a
// `category` column (migration 0003), the dashboard derives it client-side via
// `categoryOf(slug)` below. The mapping mirrors the action.tag analysis of the
// phase1 default-policy fixtures.
// ─────────────────────────────────────────────────────────────────────────

// MK_v2 핸드오프 기준 — 프로토콜·자산 유형 12종 (색 = 3그룹: 자산=gold /
// 거래=cyan / 스테이킹·인프라=slate). 라벨은 프로토타입대로 영문 그대로 노출.
export type CategoryKey =
  | "Token" | "DEX" | "Lending" | "Perp" | "Bridge" | "LiquidStaking"
  | "Staking" | "Restaking" | "NFT" | "Airdrop" | "Launchpad" | "Others";

export const CATEGORY_ORDER: CategoryKey[] = [
  "Token", "DEX", "Lending", "Perp", "Bridge", "LiquidStaking",
  "Staking", "Restaking", "NFT", "Airdrop", "Launchpad", "Others",
];

export const CATEGORY_COLOR: Record<CategoryKey, DomainColor> = {
  // gold — 자산
  Token:         { family: "gold",  hex: "#A9781F", soft: "#F3E7CE", ink: "#4A330F" },
  NFT:           { family: "gold",  hex: "#C99B45", soft: "#F8F0DD", ink: "#5C421A" },
  Airdrop:       { family: "gold",  hex: "#7E591E", soft: "#EEE0C5", ink: "#3D2A0E" },
  Launchpad:     { family: "gold",  hex: "#B97E2A", soft: "#F5EAD2", ink: "#4F360F" },
  // cyan — 거래
  DEX:           { family: "cyan",  hex: "#688186", soft: "#DCEAED", ink: "#2B3639" },
  Perp:          { family: "cyan",  hex: "#4E6468", soft: "#D3E3E6", ink: "#2B3639" },
  Bridge:        { family: "cyan",  hex: "#3E7C82", soft: "#D7E9EA", ink: "#234044" },
  // slate — 스테이킹 · 인프라 · 기타
  Lending:       { family: "slate", hex: "#384455", soft: "#D7DBDF", ink: "#0D1118" },
  LiquidStaking: { family: "slate", hex: "#586273", soft: "#E2E6EA", ink: "#1B222C" },
  Staking:       { family: "slate", hex: "#697485", soft: "#EFF0F2", ink: "#1B222C" },
  Restaking:     { family: "slate", hex: "#2A3441", soft: "#D7DBDF", ink: "#0D1118" },
  Others:        { family: "slate", hex: "#9099A5", soft: "#EFF0F2", ink: "#2A3441" },
};

export const CATEGORY_ICON: Record<CategoryKey, string> = {
  Token:         "M12 3a9 9 0 100 18 9 9 0 000-18M9.5 9.5h3.2a1.8 1.8 0 010 3.6H9.5h3.8a1.8 1.8 0 010 3.6H9.5M12 7v1.6M12 16.6V18",
  DEX:           "M7 7h11l-3-3M17 17H6l3 3",
  Lending:       "M3 10l9-5 9 5M5 10v8h14v-8M9 18v-4h6v4",
  Perp:          "M3 17l5-6 4 3 5-8 4 5",
  Bridge:        "M2 16v-4M22 16v-4M2 12c5-4 15-4 20 0M2 15h20M9 16v-3M15 16v-3",
  LiquidStaking: "M12 3c3.2 4 6 7 6 10a6 6 0 01-12 0c0-3 2.8-6 6-10zM9 15a3 3 0 003 2.5",
  Staking:       "M12 3l9 5-9 5-9-5zM3 12l9 5 9-5M3 16l9 5 9-5",
  Restaking:     "M20 11a8 8 0 00-14-4.5M4 6v3h3M4 13a8 8 0 0014 4.5M20 18v-3h-3",
  NFT:           "M12 3l5 5-5 13-5-13zM7 8h10M12 3v18",
  Airdrop:       "M5 11a7 7 0 0114 0H5zM12 11v6m-3 0a3 3 0 006 0M9.5 17.5l2.5 3 2.5-3",
  Launchpad:     "M12 3c2.6 2.4 4 5.4 4 9l-1.8 3H9.8L8 12c0-3.6 1.4-6.6 4-9zM12 9.5v.01M9.4 18l-2 3M14.6 18l2 3",
  Others:        "M6 12h.01M12 12h.01M18 12h.01",
};

export const CATEGORY_NAME: Record<CategoryKey, { en: string; ko: string }> = {
  Token:         { en: "Token",         ko: "Token" },
  DEX:           { en: "DEX",           ko: "DEX" },
  Lending:       { en: "Lending",       ko: "Lending" },
  Perp:          { en: "Perp",          ko: "Perp" },
  Bridge:        { en: "Bridge",        ko: "Bridge" },
  LiquidStaking: { en: "LiquidStaking", ko: "LiquidStaking" },
  Staking:       { en: "Staking",       ko: "Staking" },
  Restaking:     { en: "Restaking",     ko: "Restaking" },
  NFT:           { en: "NFT",           ko: "NFT" },
  Airdrop:       { en: "Airdrop",       ko: "Airdrop" },
  Launchpad:     { en: "Launchpad",     ko: "Launchpad" },
  Others:        { en: "Others",        ko: "Others" },
};

/**
 * slug → category, derived from each policy's manifest `trigger.action.tag`.
 * Covers the current phase1A market seed plus the phase1 fixture set, so it
 * keeps working after the seed is regenerated. Unknown slugs fall back to
 * `others`. This map is the client-side stand-in for the future DB column.
 */
const CATEGORY_BY_SLUG: Record<string, CategoryKey> = {
  // MK_v2 핸드오프 기준 — 프로토콜·자산 유형 매핑.
  // ⚠️ 임시 시드(PASU Beginner Pack V1) 멤버 slug — market-seed-beginner.ts 제거 시 함께 삭제.
  "unapproved-contract-token-max-approval": "Token",
  "unapproved-marketplace-delegation": "NFT",
  "swap-asset-redirect": "DEX",
  "burn-address-transfer": "Token",
  "unsupported-protocol": "Others",
  // [Token] Beginner Shield 데모 시드 멤버 (Wallet Guardians docs) — 시드 제거 시 함께 삭제.
  "token-self-contract-transfer-warn": "Token",
  "permit2-max-signature-warn": "Token",
  "malicious-address-approval-deny": "Token",
  // Token — approve / permit / 전송 / 무제한 승인
  "unlimited-approval-deny": "Token",
  "multicall-hidden-approval-warn": "Token",
  "permit2-sign-allowance-confirm": "Token",
  "increase-allowance-cap-warn": "Token",
  "reapprove-already-granted-warn": "Token",
  "permit-allowance-horizon-warn": "Token",
  "permit2-sign-allowance-far-expiry-warn": "Token",
  "holding-pct-outflow-warn": "Token",
  "send-first-time-or-burn-recipient-warn": "Token",
  "transfer-to-token-own-contract-deny": "Token",
  // DEX — swap / AMM LP
  "swap-recipient-not-self-deny": "DEX",
  "swap-slippage-high-warn": "DEX",
  "values-recipient-denylist-deny": "DEX",
  "ammlp-remove-recipient-not-self-deny": "DEX",
  "ammlp-collect-recipient-not-self-deny": "DEX",
  "lp-commit-platform-allowlist-deny": "DEX",
  // Lending
  "aave-delegate-borrow-allowlist-deny": "Lending",
  // Perp
  "hl-confirm-high-leverage": "Perp",
  "perp-leverage-cap-deny": "Perp",
  "hl-no-short-perp": "Perp",
  "hl-confirm-unknown": "Perp",
  "perp-leverage-increase-warn": "Perp",
  "perp-market-slippage-warn": "Perp",
  "perp-reduce-only-flip-deny": "Perp",
  "hl-confirm-approve-agent": "Perp",
  "hl-confirm-usd-send": "Perp",
  "hl-confirm-withdraw": "Perp",
  // Bridge
  "bridge-recipient-not-self-deny": "Bridge",
  "bridge-dest-chain-mismatch-warn": "Bridge",
  "bridge-untrusted-router-deny": "Bridge",
  "bridge-unlimited-approval-deny": "Bridge",
  "bridge-refund-not-self-warn": "Bridge",
  "bridge-target-not-allowlisted-deny": "Bridge",
  // LiquidStaking
  "lst-mint-recipient-not-self-deny": "LiquidStaking",
  "lst-unstake-recipient-warn": "LiquidStaking",
  // Staking
  "stake-validator-allowlist-warn": "Staking",
  "stake-withdrawal-address-change-deny": "Staking",
  // Restaking
  "restake-operator-allowlist-deny": "Restaking",
  "restake-withdrawal-recipient-deny": "Restaking",
  // NFT
  "setapprovalforall-operator-warning": "NFT",
  "nft-untrusted-blur-root-deny": "NFT",
  "nft-bid-weth-unlimited-warn": "NFT",
  "nft-setapprovalforall-conduit-warn": "NFT",
  "nft-transfer-burn-recipient-deny": "NFT",
  "nft-far-expiry-order-warn": "NFT",
  // Airdrop
  "air-recipient-not-self-deny": "Airdrop",
  "air-merkle-without-proof-warn": "Airdrop",
  "air-permit-on-held-token-deny": "Airdrop",
  "air-claim-locks-received-warn": "Airdrop",
  // Launchpad
  "launchpad-claim-recipient-not-self-deny": "Launchpad",
  "launchpad-presale-approval-warn": "Launchpad",
  "launchpad-unaudited-sale-warn": "Launchpad",
  // Others — governance / blind-sign / gas / chain-mismatch
  "gov-delegatee-allowlist-deny": "Others",
  "air-delegatee-not-self-deny": "Others",
  "unknown-blind-sign-warning": "Others",
  "signature-chain-mismatch-permit-warn": "Others",
  "gas-cost-usd-cap-deny": "Others",
  "gas-cost-ratio-warn": "Others",
};

export function isCategoryKey(s: string | undefined | null): s is CategoryKey {
  return !!s && s in CATEGORY_COLOR;
}

/** Resolve a listing's category from its slug. Falls back to `others`. */
export function categoryOf(slug: string | undefined): CategoryKey {
  if (slug && CATEGORY_BY_SLUG[slug]) return CATEGORY_BY_SLUG[slug];
  return "Others";
}

export function categoryNameOf(c: string | undefined, locale: "en" | "ko"): string {
  if (isCategoryKey(c)) return CATEGORY_NAME[c][locale];
  return c ?? "";
}

/** 24x24 line glyph for a category. */
export function CategoryGlyph({
  category,
  size = 16,
  color,
  className,
}: {
  category: CategoryKey;
  size?: number;
  color?: string;
  className?: string;
}) {
  const stroke = color ?? CATEGORY_COLOR[category].hex;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={stroke}
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <path d={CATEGORY_ICON[category]} />
    </svg>
  );
}
