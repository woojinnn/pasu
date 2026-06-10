/** 마켓 설치 v2 — 리스팅을 ps2 def로 변환해 `ps2:install-market`로 설치한다.
 *  재설치(같은 def id)는 SW가 업데이트로 처리(바인딩 params 보존). */
import { installListing, pickI18n, type ListingDetail } from "../server-api";
import {
  installMarket,
  type HoleValue,
  type MarketInstallScope,
  type PackageDef,
} from "../server-api/policy-store";
import { textToBlocks } from "../cedar";
import { listingToDefs, type ListingMeta } from "./market-install-convert";

export { listingToDefs } from "./market-install-convert";

export interface InstallChoice {
  scope: MarketInstallScope;
  applyToNewWallets: boolean;
  /** kind=policy일 때 사용자가 고른 패키지 (set은 자동 패키지). */
  packageId: string | null;
}

/** 서버 install 기록 → def 변환 → ps2:install-market. 설치된 def id들을 반환. */
export async function installListingV2(
  detail: ListingDetail,
  locale: "ko" | "en",
  choice: InstallChoice,
): Promise<{ kind: "policy" | "set"; defIds: string[] }> {
  if (!detail.current_version) {
    throw new Error(locale === "ko" ? "이 listing에는 발행된 버전이 없습니다." : "This listing has no published version.");
  }
  const body = await installListing(detail.id, detail.current_version);
  const meta: ListingMeta = {
    id: detail.id,
    kind: detail.kind,
    displayName: pickI18n(detail.display_name, locale) || detail.slug,
    version: detail.current_version,
    cat: detail.category ?? detail.domain ?? undefined,
  };
  const defs = await listingToDefs(meta, body, textToBlocks);

  const pkg: PackageDef | undefined =
    detail.kind === "set"
      ? {
          id: `pkg::market.${detail.id}`,
          displayName: meta.displayName,
          source: "market",
          sourceListingId: detail.id,
          sourceVersion: detail.current_version,
          updatedAtMs: Date.now(),
        }
      : undefined;

  const packageId = pkg?.id ?? choice.packageId ?? undefined;
  for (const d of defs) {
    d.defaults = { enabled: choice.applyToNewWallets, params: {}, packageId };
  }

  const params: Record<string, Record<string, HoleValue>> = {};
  await installMarket({ defs, ...(pkg ? { pkg } : {}), scope: choice.scope, params });
  return { kind: detail.kind, defIds: defs.map((d) => d.id) };
}
