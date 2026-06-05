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

export type ColorFamily = "cyan" | "sage" | "slate";

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
