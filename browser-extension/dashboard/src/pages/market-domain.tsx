/**
 * Domain metadata + visual primitives reused across the market browse and
 * detail pages. The palette is the original Cloudy Pond scheme: three color
 * families (Cyan = trading, Sage = safety/holding, Slate = assets/infra),
 * each containing four domains at varying lightness so a card's family is
 * recognizable at a glance.
 *
 * SVG icon paths are kept literal (24x24 viewBox, no fill); render with
 * `<DomainGlyph domain="swap" size={16} />`.
 *
 * Display names live in the `market` i18n namespace (`domain.<id>` /
 * `category.<id>`); the lookups below resolve them at call time.
 */
import { i18n } from "../i18n";

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

export function isDomainKey(s: string | undefined | null): s is DomainKey {
  return !!s && s in DOMAIN_COLOR;
}

export function domainNameOf(d: string | undefined, locale: "en" | "ko"): string {
  if (isDomainKey(d)) return i18n.t(`market:domain.${d}`, { lng: locale });
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

export type CategoryKey =
  | "approvals" | "signing" | "transfer" | "swap" | "derivatives" | "perps"
  | "liquidity" | "lending" | "rewards" | "governance" | "intents" | "others";

// 4×3 grid order (12). Split out of the original 10 per the taxonomy design:
// `signing` peels EIP-712 allowance signing off `approvals`; `derivatives`
// peels position-trading off `perps` (which is now perp account ops only).
export const CATEGORY_ORDER: CategoryKey[] = [
  "approvals", "signing", "transfer", "swap", "derivatives", "perps",
  "liquidity", "lending", "rewards", "governance", "intents", "others",
];

export const CATEGORY_COLOR: Record<CategoryKey, DomainColor> = {
  // cyan — trading
  swap:        { family: "cyan",  hex: "#688186", soft: "#DCEAED", ink: "#2B3639" },
  derivatives: { family: "cyan",  hex: "#4E6468", soft: "#D3E3E6", ink: "#2B3639" },
  liquidity:   { family: "cyan",  hex: "#85A4AB", soft: "#EDF4F6", ink: "#2B3639" },
  // sage — safety / permission
  approvals:   { family: "sage",  hex: "#637E59", soft: "#EBF3E8", ink: "#283523" },
  signing:     { family: "sage",  hex: "#9CC58D", soft: "#EEF5E9", ink: "#44583D" },
  transfer:    { family: "sage",  hex: "#7FA172", soft: "#EBF3E8", ink: "#283523" },
  rewards:     { family: "sage",  hex: "#44583D", soft: "#D9E9D3", ink: "#283523" },
  // slate — assets / infra / account
  perps:       { family: "slate", hex: "#586273", soft: "#E2E6EA", ink: "#1B222C" },
  lending:     { family: "slate", hex: "#384455", soft: "#D7DBDF", ink: "#0D1118" },
  governance:  { family: "slate", hex: "#2A3441", soft: "#D7DBDF", ink: "#0D1118" },
  intents:     { family: "slate", hex: "#697485", soft: "#EFF0F2", ink: "#1B222C" },
  others:      { family: "slate", hex: "#9099A5", soft: "#EFF0F2", ink: "#2A3441" },
};

export const CATEGORY_ICON: Record<CategoryKey, string> = {
  swap:        "M7 7h11l-3-3M17 17H6l3 3",
  approvals:   "M9 12.5l2 2 4-4.5M12 3l7 3v5c0 4-3 7-7 9-4-2-7-5-7-9V6z",
  signing:     "M4 20h4L18.5 9.5a2.12 2.12 0 00-3-3L5 17zM13.5 6.5l3 3",
  transfer:    "M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z",
  derivatives: "M3 17l5-6 4 3 5-7 4 4",
  perps:       "M3 8h18v11H3zM3 8l3-4h12l3 4M16 13h3",
  liquidity:   "M12 3c3 4 6 7 6 10a6 6 0 01-12 0c0-3 3-6 6-10z",
  lending:     "M3 10h18M5 10v8h14v-8M9 14h6",
  rewards:     "M12 3a6 6 0 016 6c0 3-6 9-6 9S6 12 6 9a6 6 0 016-6M12 21v-3",
  governance:  "M5 21h14M6 21V9M18 21V9M4 9l8-5 8 5M9 13v4M15 13v4",
  intents:     "M9.1 9.2a3 3 0 015.6 1.2c0 2-3 2.3-3 4M12 17.5h.01",
  others:      "M5 12h.01M12 12h.01M19 12h.01",
};

/**
 * slug → category, derived from each policy's manifest `trigger.action.tag`.
 * Covers the current phase1A market seed plus the phase1 fixture set, so it
 * keeps working after the seed is regenerated. Unknown slugs fall back to
 * `others`. This map is the client-side stand-in for the future DB column.
 */
const CATEGORY_BY_SLUG: Record<string, CategoryKey> = {
  // approvals — erc20_approve / nft_set_approval_for_all / permit(2) / multicall
  "unlimited-approval-deny": "approvals",
  "increase-allowance-cap-warn": "approvals",
  "reapprove-already-granted-warn": "approvals",
  "bridge-unlimited-approval-deny": "approvals",
  "nft-bid-weth-unlimited-warn": "approvals",
  "nft-setapprovalforall-conduit-warn": "approvals",
  "setapprovalforall-operator-warning": "approvals",
  "multicall-hidden-approval-warn": "approvals",
  // signing — off-chain EIP-712 allowance signatures (permit / permit2)
  "permit-allowance-horizon-warn": "signing",
  "permit2-sign-allowance-confirm": "signing",
  "permit2-sign-allowance-far-expiry-warn": "signing",
  "signature-chain-mismatch-permit-warn": "signing",
  // transfer — erc20_transfer / nft_transfer
  "bridge-recipient-not-self-deny": "transfer",
  "holding-pct-outflow-warn": "transfer",
  "send-first-time-or-burn-recipient-warn": "transfer",
  "transfer-to-token-own-contract-deny": "transfer",
  "nft-transfer-burn-recipient-deny": "transfer",
  // swap
  "swap-recipient-not-self-deny": "swap",
  "swap-slippage-high-warn": "swap",
  "values-recipient-denylist-deny": "swap",
  // derivatives — perp position trading (open / leverage / short)
  "hl-confirm-high-leverage": "derivatives",
  "hl-no-short-perp": "derivatives",
  "hl-confirm-unknown": "derivatives",
  "perp-leverage-cap-deny": "derivatives",
  "perp-leverage-increase-warn": "derivatives",
  "perp-market-slippage-warn": "derivatives",
  "perp-reduce-only-flip-deny": "derivatives",
  // perps — perp account ops (agent approval / funds in-out)
  "hl-confirm-approve-agent": "perps",
  "hl-confirm-usd-send": "perps",
  "hl-confirm-withdraw": "perps",
  // liquidity — remove_liquidity / collect_fees / lp commit
  "ammlp-remove-recipient-not-self-deny": "liquidity",
  "ammlp-collect-recipient-not-self-deny": "liquidity",
  "lp-commit-platform-allowlist-deny": "liquidity",
  // lending — delegate_borrow
  "aave-delegate-borrow-allowlist-deny": "lending",
  // rewards — claim / airdrop
  "air-permit-on-held-token-deny": "rewards",
  "air-recipient-not-self-deny": "rewards",
  "air-claim-locks-received-warn": "rewards",
  "air-merkle-without-proof-warn": "rewards",
  // governance — delegate
  "gov-delegatee-allowlist-deny": "governance",
  "air-delegatee-not-self-deny": "governance",
  // intents — blind sign / orders
  "unknown-blind-sign-warning": "intents",
  "nft-far-expiry-order-warn": "intents",
  "nft-untrusted-blur-root-deny": "intents",
  // others — bridge misc / gas
  "bridge-refund-not-self-warn": "others",
  "bridge-target-not-allowlisted-deny": "others",
  "gas-cost-ratio-warn": "others",
  "gas-cost-usd-cap-deny": "others",
};

export function isCategoryKey(s: string | undefined | null): s is CategoryKey {
  return !!s && s in CATEGORY_COLOR;
}

/** Resolve a listing's category from its slug. Falls back to `others`. */
export function categoryOf(slug: string | undefined): CategoryKey {
  if (slug && CATEGORY_BY_SLUG[slug]) return CATEGORY_BY_SLUG[slug];
  return "others";
}

export function categoryNameOf(c: string | undefined, locale: "en" | "ko"): string {
  if (isCategoryKey(c)) return i18n.t(`market:category.${c}`, { lng: locale });
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
