/** 마켓 설치 v2 — 리스팅을 ps2 def로 변환해 `ps2:install-market`로 설치한다.
 *  재설치(같은 def id)는 SW가 업데이트로 처리(바인딩 params 보존). */
import { installListing, pickI18n, type ListingDetail } from "../server-api";
import {
  bindDef,
  installMarket,
  putWalletPackage,
  UNCATEGORIZED_PKG,
  type HoleValue,
  type MarketInstallScope,
  type PackageDef,
  type StoreSnapshot,
} from "../server-api/policy-store";
import { textToBlocks } from "../cedar";
import { listingToDefs, type ListingMeta } from "./market-install-convert";

export { listingToDefs } from "./market-install-convert";

export interface InstallChoice {
  scope: MarketInstallScope;
  applyToNewWallets: boolean;
  /** kind=policy일 때 사용자가 고른 폴더 (set은 자동 패키지). */
  packageId: string | null;
}

/** 서버 install 기록 → def 변환 → ps2:install-market. 설치된 def id들을 반환. */
export async function installListingV2(
  detail: ListingDetail,
  locale: "ko" | "en",
  choice: InstallChoice,
): Promise<{ kind: "policy" | "set"; defIds: string[] }> {
  const { meta, defs } = await convertListing(detail, locale);

  const pkg: PackageDef | undefined =
    detail.kind === "set"
      ? {
          id: `pkg::market.${detail.id}`,
          displayName: meta.displayName,
          source: "market",
          sourceListingId: detail.id,
          sourceVersion: detail.current_version!,
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

/** 지갑 전용 설치의 지갑별 패키지 결정 — 기존 패키지 id 또는 "이 이름의
 *  패키지에 넣기"(같은 이름이 이미 있으면 재사용, 없으면 생성). */
export type WalletPkgPick = { id: string } | { newName: string };

export interface WalletOnlyInstallChoice {
  addresses: string[];
  walletPackages: Record<string, WalletPkgPick>;
  /** find-or-create·중복 바인딩 가드용 현재 스토어 스냅샷. */
  snap: StoreSnapshot;
}

/** 지갑 전용 설치: def는 hidden으로 라이브러리에 넣고(카탈로그 비노출), 지갑별
 *  패키지를 find-or-create한 뒤 주소마다 바인딩한다. 이미 라이브러리에 보이는
 *  def(이전에 라이브러리로 설치)는 hidden을 덮지 않고 그대로 바인딩만 한다. */
export async function installListingWalletOnlyV2(
  detail: ListingDetail,
  locale: "ko" | "en",
  choice: WalletOnlyInstallChoice,
): Promise<{ kind: "policy" | "set"; defIds: string[] }> {
  const { defs } = await convertListing(detail, locale);

  for (const d of defs) {
    const prev = choice.snap.library.defs[d.id];
    if (prev) {
      // 재설치(업데이트): 기존 노출 상태·기본값을 보존한다.
      if (prev.hidden) d.hidden = true;
      d.defaults = prev.defaults;
    } else {
      d.hidden = true;
      d.defaults = { enabled: false, params: {}, packageId: undefined };
    }
  }
  await installMarket({ defs, scope: { kind: "library-only" }, params: {} });

  for (const address of choice.addresses) {
    const w = choice.snap.wallets.byAddress[address];
    const pick = choice.walletPackages[address] ?? { id: UNCATEGORIZED_PKG };
    let pkgId: string;
    if ("id" in pick) {
      pkgId = pick.id;
    } else {
      const existing = Object.values(w?.packages ?? {}).find(
        (p) => p.displayName === pick.newName,
      );
      if (existing) {
        pkgId = existing.id;
      } else {
        pkgId = `pkg::${crypto.randomUUID()}`;
        await putWalletPackage({ address, pkg: { id: pkgId, displayName: pick.newName } });
      }
    }
    for (const d of defs) {
      // 같은 패키지에 이미 들어 있으면 줄을 또 만들지 않는다(재설치 멱등).
      const dup = Object.values(w?.bindings ?? {}).some(
        (b) => b.defId === d.id && b.packageId === pkgId,
      );
      if (!dup) await bindDef({ defId: d.id, packageId: pkgId, addresses: [address] });
    }
  }
  return { kind: detail.kind, defIds: defs.map((d) => d.id) };
}

/** 공통: 서버 install 기록 → meta + ps2 def 변환. */
async function convertListing(
  detail: ListingDetail,
  locale: "ko" | "en",
): Promise<{ meta: ListingMeta; defs: Awaited<ReturnType<typeof listingToDefs>> }> {
  if (!detail.current_version) {
    throw new Error(
      locale === "ko" ? "이 listing에는 발행된 버전이 없습니다." : "This listing has no published version.",
    );
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
  return { meta, defs };
}
