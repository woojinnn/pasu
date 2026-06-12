/**
 * Curated "what this package blocks" copy for the package detail summary.
 * Sourced from agentBase/policy-packages/*.md. Interim front-end map (mock
 * content) until the `docs` backend lands. The strings live in the `market`
 * i18n namespace under `packageCopy.<slug>` (ko/en).
 */
import { i18n } from "../i18n";

export interface PackageBlock {
  t: string;
  d: string;
}
export interface PackageCopy {
  intro: string;
  blocks: PackageBlock[];
}

/** Slugs that ship curated copy (keys under `market:packageCopy.*`). */
const PACKAGE_COPY_SLUGS: readonly string[] = [
  "wallet-first-shield",
  "no-mistake-swap",
  "never-again",
  "nft-vault-guard",
  "leverage-safety",
  "claim-and-vote-guard",
];

export function packageCopy(slug: string): PackageCopy | undefined {
  if (!PACKAGE_COPY_SLUGS.includes(slug)) return undefined;
  const blocks = i18n.t(`market:packageCopy.${slug}.blocks`, {
    returnObjects: true,
  }) as unknown;
  return {
    intro: i18n.t(`market:packageCopy.${slug}.intro`),
    blocks: Array.isArray(blocks) ? (blocks as PackageBlock[]) : [],
  };
}
