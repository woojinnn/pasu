/**
 * Domain categories for the new list view.
 *
 * The `cat` field on a ManagedPolicy is free-form; we render anything
 * unknown via the `core` fallback. The order here drives the chip row
 * ordering in `policy-table`. Colors are the Cloudy Pond accents
 * (cyan / sage / slate families) — `hex` for icon strokes, `soft` for
 * tag fills, `ink` for tag text.
 */

import { i18n } from "../../../i18n";

export type CategoryKey =
  | "swap"
  | "amm"
  | "perp"
  | "bridge"
  | "security"
  | "airdrop"
  | "lending"
  | "nft"
  | "core"
  | "token";

interface CategoryDef {
  ko: string;
  en: string;
  hex: string;
  soft: string;
  ink: string;
}

export const CAT: Record<CategoryKey, CategoryDef> = {
  swap: { ko: "스왑", en: "Swap", hex: "#688186", soft: "#DCEAED", ink: "#2B3639" },
  amm: { ko: "AMM·LP", en: "AMM·LP", hex: "#85A4AB", soft: "#EDF4F6", ink: "#2B3639" },
  perp: { ko: "퍼프", en: "Perp", hex: "#485A5E", soft: "#CAE0E4", ink: "#2B3639" },
  bridge: { ko: "브릿지", en: "Bridge", hex: "#A4C9D1", soft: "#EDF4F6", ink: "#485A5E" },
  security: { ko: "보안", en: "Security", hex: "#637E59", soft: "#EBF3E8", ink: "#283523" },
  airdrop: { ko: "에어드랍", en: "Airdrop", hex: "#44583D", soft: "#D9E9D3", ink: "#283523" },
  lending: { ko: "렌딩", en: "Lending", hex: "#384455", soft: "#D7DBDF", ink: "#0D1118" },
  nft: { ko: "NFT", en: "NFT", hex: "#697485", soft: "#EFF0F2", ink: "#1B222C" },
  core: { ko: "코어", en: "Core", hex: "#2A3441", soft: "#D7DBDF", ink: "#0D1118" },
  token: { ko: "토큰", en: "Token", hex: "#9099A5", soft: "#EFF0F2", ink: "#2A3441" },
};

export const CAT_ORDER: CategoryKey[] = [
  "security",
  "swap",
  "lending",
  "airdrop",
  "perp",
  "bridge",
  "nft",
  "amm",
  "core",
  "token",
];

export function catKey(cat: string | undefined): CategoryKey {
  if (cat && cat in CAT) return cat as CategoryKey;
  return "core";
}

export function catLabel(cat: string | undefined): string {
  const c = CAT[catKey(cat)];
  return i18n.language?.startsWith("en") ? c.en : c.ko;
}

export function catStyle(cat: string | undefined): {
  iconWrap: React.CSSProperties;
  tag: React.CSSProperties;
  hex: string;
  soft: string;
  ink: string;
} {
  const c = CAT[catKey(cat)];
  return {
    iconWrap: { background: c.soft, color: c.hex },
    tag: { background: c.soft, color: c.ink },
    hex: c.hex,
    soft: c.soft,
    ink: c.ink,
  };
}
