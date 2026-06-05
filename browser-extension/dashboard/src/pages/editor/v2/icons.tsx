import type { SVGProps } from "react";

/**
 * Shared SVG icon set for the v2 list/editor screens. Each icon takes
 * the full SVGProps so callers can pass className/style/title. Stroke
 * is `currentColor`, so the parent's text color controls the icon.
 */

type IconProps = SVGProps<SVGSVGElement>;

const base: IconProps = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round",
  strokeLinejoin: "round",
  width: 16,
  height: 16,
};

export const ShieldIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" {...base} {...p}>
    <path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z" />
    <path d="M9 12l2 2 4-4" />
  </svg>
);

export const SearchIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" {...base} {...p}>
    <circle cx="11" cy="11" r="7" />
    <path d="M16 16l5 5" />
  </svg>
);

export const XIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" {...base} strokeWidth={2.2} {...p}>
    <path d="M6 6l12 12M18 6L6 18" />
  </svg>
);

export const PlusIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" {...base} strokeWidth={2.3} {...p}>
    <path d="M12 5v14M5 12h14" />
  </svg>
);

export const CaretRightIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" {...base} {...p}>
    <path d="M9 6l6 6-6 6" />
  </svg>
);

export const GripIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" fill="currentColor" {...p} width={p.width ?? 16} height={p.height ?? 16}>
    <circle cx="9" cy="6" r="1.6" />
    <circle cx="15" cy="6" r="1.6" />
    <circle cx="9" cy="12" r="1.6" />
    <circle cx="15" cy="12" r="1.6" />
    <circle cx="9" cy="18" r="1.6" />
    <circle cx="15" cy="18" r="1.6" />
  </svg>
);

export const FolderIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" {...base} strokeWidth={1.9} {...p}>
    <path d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2z" />
  </svg>
);

export const LockIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" {...base} {...p}>
    <rect x="5" y="11" width="14" height="9" rx="2" />
    <path d="M8 11V8a4 4 0 018 0v3" />
  </svg>
);

export const PencilIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" {...base} {...p}>
    <path d="M4 20l4-1 10-10-3-3L5 16z" />
    <path d="M14 6l3 3" />
  </svg>
);

export const WarnIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" {...base} {...p}>
    <path d="M12 3l9 16H3z" />
    <path d="M12 10v4M12 17h.01" />
  </svg>
);

export const CheckIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" {...base} strokeWidth={2.4} {...p}>
    <path d="M5 13l4 4L19 7" />
  </svg>
);

export const DotIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" fill="currentColor" {...p} width={p.width ?? 16} height={p.height ?? 16}>
    <circle cx="12" cy="12" r="4" />
  </svg>
);

const CAT_PATHS: Record<string, string> = {
  swap: "M7 7h11l-3-3M17 17H6l3 3",
  amm: "M12 3c3 4 6 7 6 10a6 6 0 01-12 0c0-3 3-6 6-10z",
  perp: "M3 17l5-6 4 3 5-7 4 4",
  bridge: "M3 16c0-4 3-7 9-7s9 3 9 7M8 13v6M16 13v6",
  security: "M12 3l7 3v5c0 4-3 7-7 9-4-2-7-5-7-9V6z",
  airdrop: "M12 3a6 6 0 016 6c0 3-6 9-6 9S6 12 6 9a6 6 0 016-6M12 21v-3",
  lending: "M3 10h18M5 10v8h14v-8M9 14h6",
  nft: "M4 4h16v16H4zM8 10a1.5 1.5 0 100-3 1.5 1.5 0 000 3M4 16l5-4 4 3 3-2 4 3",
  core: "M12 3l8 4v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V7z",
  token: "M12 4a8 8 0 100 16 8 8 0 000-16M9.5 12l1.8 1.8 3.5-3.6",
};

export function CatIcon({ cat, ...p }: IconProps & { cat: string | undefined }) {
  const d = (cat && CAT_PATHS[cat]) || CAT_PATHS.core;
  return (
    <svg viewBox="0 0 24 24" {...base} strokeWidth={1.9} {...p}>
      <path d={d} />
    </svg>
  );
}
