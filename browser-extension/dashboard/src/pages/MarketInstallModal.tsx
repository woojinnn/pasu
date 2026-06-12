import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";

import { getListing, pickI18n, type ListingSummary } from "../server-api";
import { getOverview, UNCATEGORIZED_PKG } from "../server-api/policy-store";
import { listWallets } from "../server-api/wallets";
import {
  holeInputToValue,
  installListingV2,
  installListingWalletOnlyV2,
  requiredHoleInputs,
  type InstallParams,
  type WalletPkgPick,
} from "./market-install-v2";
import type { MarketLocale } from "./market-locale";

function shortAddr(a: string): string {
  return a.length > 12 ? `${a.slice(0, 6)}…${a.slice(-4)}` : a;
}

/** "받기" 모달 v3 — 정책 저장과 같은 2단계: ① 지갑 전용 vs 라이브러리,
 *  ② 지갑 경로 = 지갑별 패키지(+일괄 새 패키지, 이름 충돌 시 재사용),
 *     라이브러리 경로 = 폴더 + 지금 모든 지갑 적용 + 새 지갑 기본 적용.
 *  마켓 목록/상세 두 진입점이 공유. */
export function MarketInstallModal({
  listing,
  locale,
  onClose,
}: {
  listing: ListingSummary;
  locale: MarketLocale;
  onClose: () => void;
}) {
  const { t } = useTranslation("market");
  const navigate = useNavigate();
  const qc = useQueryClient();

  const detailQ = useQuery({
    queryKey: ["market-listing", listing.slug],
    queryFn: () => getListing(listing.slug),
  });
  const walletsQ = useQuery({ queryKey: ["wallets"], queryFn: listWallets });
  const overviewQ = useQuery({ queryKey: ["ps2-overview"], queryFn: getOverview });
  const snap = overviewQ.data ?? null;

  const wallets = useMemo(() => {
    const addrs = new Set([
      ...(walletsQ.data ?? []).map((w) => w.address.toLowerCase()),
      ...Object.keys(snap?.wallets.byAddress ?? {}),
    ]);
    return [...addrs].sort().map((address) => ({
      address,
      packages: Object.values(snap?.wallets.byAddress[address]?.packages ?? {})
        .map((p) => ({ id: p.id, displayName: p.displayName }))
        .sort((a, b) => a.displayName.localeCompare(b.displayName, "ko")),
    }));
  }, [walletsQ.data, snap]);
  const libPackages = useMemo(
    () => Object.values(snap?.library.packages ?? {}),
    [snap],
  );

  const isSet = listing.kind === "set";
  const name = pickI18n(listing.display_name, locale) || listing.slug;
  const memberCount = detailQ.data?.latest_version?.members?.length ?? 0;

  // 게시 때 블랭킹된 required hole — 채워야 설치(바인딩)가 가능하다.
  const holesQ = useQuery({
    queryKey: ["market-required-holes", listing.slug, snap?.rev ?? -1],
    queryFn: () => requiredHoleInputs(detailQ.data!, locale, snap),
    enabled: !!detailQ.data && !!snap,
  });
  const holeReqs = holesQ.data ?? [];
  /** defId → hole 이름 → 입력 문자열. */
  const [holeVals, setHoleVals] = useState<Record<string, Record<string, string>>>({});
  const setHoleVal = (defId: string, name: string, v: string) =>
    setHoleVals((m) => ({ ...m, [defId]: { ...(m[defId] ?? {}), [name]: v } }));
  /** 전부 유효하게 채워졌으면 HoleValue로 변환, 아니면 null. */
  const filledParams = useMemo<InstallParams | null>(() => {
    const out: InstallParams = {};
    for (const req of holeReqs) {
      for (const h of req.holes) {
        const v = holeInputToValue(h.type, holeVals[req.defId]?.[h.name] ?? "");
        if (v === null) return null;
        (out[req.defId] ??= {})[h.name] = v;
      }
    }
    return out;
  }, [holeReqs, holeVals]);

  const [kind, setKind] = useState<"wallet" | "library" | null>(null);
  // 지갑 경로 — 지갑별 패키지 선택 + 일괄 새 패키지.
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [walletPkg, setWalletPkg] = useState<Record<string, string>>({});
  const [walletNewName, setWalletNewName] = useState<Record<string, string>>({});
  const [bulk, setBulk] = useState(false);
  const [bulkName, setBulkName] = useState(name); // 기본값 = 리스팅 이름
  // 라이브러리 경로.
  const [packageId, setPackageId] = useState(UNCATEGORIZED_PKG);
  const [applyToAllNow, setApplyToAllNow] = useState(false);
  const [applyToNewWallets, setApplyToNewWallets] = useState(true);
  const [done, setDone] = useState(false);

  const pkgOf = (addr: string) => walletPkg[addr] ?? UNCATEGORIZED_PKG;

  const bulkCollisions = useMemo(() => {
    const n = bulkName.trim();
    if (!bulk || !n) return [];
    return [...picked].filter((a) =>
      (wallets.find((w) => w.address === a)?.packages ?? []).some((p) => p.displayName === n),
    );
  }, [bulk, bulkName, picked, wallets]);

  const mut = useMutation({
    mutationFn: async () => {
      if (kind === "wallet") {
        const walletPackages: Record<string, WalletPkgPick> = {};
        for (const addr of picked) {
          if (bulk) {
            walletPackages[addr] = { newName: bulkName.trim() };
          } else {
            const sel = pkgOf(addr);
            walletPackages[addr] =
              sel === "__new__" ? { newName: (walletNewName[addr] ?? "").trim() } : { id: sel };
          }
        }
        return installListingWalletOnlyV2(detailQ.data!, locale, {
          addresses: [...picked],
          walletPackages,
          snap: snap!,
          params: filledParams ?? {},
        });
      }
      return installListingV2(detailQ.data!, locale, {
        scope: applyToAllNow ? { kind: "all" } : { kind: "library-only" },
        applyToNewWallets,
        packageId: isSet ? null : packageId === UNCATEGORIZED_PKG ? null : packageId,
        params: filledParams ?? {},
        snap,
      });
    },
    onSuccess: async () => {
      setDone(true);
      await qc.invalidateQueries({ queryKey: ["ps2-overview"] });
      await qc.invalidateQueries({ queryKey: ["market-listing", listing.slug] });
    },
  });

  const invalid =
    // required hole이 전부 유효하게 채워지기 전엔 설치 불가 (SW 가드와 동일 기준).
    filledParams === null ||
    holesQ.isLoading ||
    (kind === "wallet" &&
      (picked.size === 0 ||
        (bulk
          ? !bulkName.trim()
          : [...picked].some((a) => pkgOf(a) === "__new__" && !(walletNewName[a] ?? "").trim()))));

  const togglePick = (a: string) =>
    setPicked((prev) => {
      const n = new Set(prev);
      if (n.has(a)) n.delete(a);
      else n.add(a);
      return n;
    });

  return (
    <div className="im-overlay" onClick={onClose}>
      <div className="im-box" onClick={(e) => e.stopPropagation()}>
        <button type="button" className="im-x" onClick={onClose} aria-label="close">
          ×
        </button>
        {done ? (
          <>
            <div className="im-ok">✓</div>
            <h3 className="im-title">{t("modal.doneTitle")}</h3>
            <p className="im-sub">
              {kind === "wallet"
                ? t("modal.appliedToWallets", { name, count: picked.size })
                : t("modal.addedToLibrary", { name })}
            </p>
            <div className="im-actions">
              <button type="button" className="btn-secondary" onClick={onClose}>
                {t("common:close")}
              </button>
              <button type="button" className="btn-primary" onClick={() => navigate("/editor?tab=apply")}>
                {t("modal.viewWalletPolicies")}
              </button>
            </div>
          </>
        ) : kind === null ? (
          <>
            <div className="im-kind">{isSet ? t("kind.package") : t("kind.policy")}</div>
            <h3 className="im-title">{name}</h3>
            <p className="im-sub">
              {isSet
                ? memberCount
                  ? t("modal.howInstallPackageCount", { n: memberCount })
                  : t("modal.howInstallPackage")
                : t("modal.howInstallPolicy")}
            </p>
            <div className="im-scope">
              <button
                type="button"
                className="im-opt"
                disabled={wallets.length === 0}
                onClick={() => setKind("wallet")}
              >
                <span className="im-opt-t">{t("modal.walletOnly")}</span>
                <span className="im-opt-d">
                  {t("modal.walletOnlyDesc")}
                  {wallets.length === 0 && t("modal.noWalletsSuffix")}
                </span>
              </button>
              <button type="button" className="im-opt" onClick={() => setKind("library")}>
                <span className="im-opt-t">{t("modal.intoLibrary")}</span>
                <span className="im-opt-d">{t("modal.intoLibraryDesc")}</span>
              </button>
            </div>
            <div className="im-actions">
              <button type="button" className="btn-secondary" onClick={onClose}>
                {t("common:cancel")}
              </button>
            </div>
          </>
        ) : (
          <>
            <div className="im-kind">{isSet ? t("kind.package") : t("kind.policy")}</div>
            <h3 className="im-title">{name}</h3>
            <p className="im-sub">
              {kind === "wallet" ? t("modal.pickWallets") : t("modal.libraryOptions")}
            </p>

            <div className="im-scope">
              {kind === "wallet" && (
                <>
                  <div className="im-wallets">
                    {wallets.map((w) => (
                      <div key={w.address}>
                        <label className="im-field">
                          <input
                            type="checkbox"
                            checked={picked.has(w.address)}
                            onChange={() => togglePick(w.address)}
                          />
                          <span className="im-addr">{w.address}</span>
                        </label>
                        {picked.has(w.address) && !bulk && (
                          <div className="im-pkgrow">
                            <span className="im-pkglabel">{t("kind.package")}</span>
                            <select
                              value={pkgOf(w.address)}
                              onChange={(e) =>
                                setWalletPkg((m) => ({ ...m, [w.address]: e.target.value }))
                              }
                            >
                              <option value={UNCATEGORIZED_PKG}>{t("modal.uncategorized")}</option>
                              {w.packages.map((p) => (
                                <option key={p.id} value={p.id}>
                                  {p.displayName}
                                </option>
                              ))}
                              <option value="__new__">{t("modal.newPackageOption")}</option>
                            </select>
                            {pkgOf(w.address) === "__new__" && (
                              <input
                                value={walletNewName[w.address] ?? ""}
                                onChange={(e) =>
                                  setWalletNewName((m) => ({ ...m, [w.address]: e.target.value }))
                                }
                                placeholder={t("modal.newPackageName")}
                              />
                            )}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                  <label className="im-field">
                    <input
                      type="checkbox"
                      checked={bulk}
                      onChange={(e) => {
                        setBulk(e.target.checked);
                        // 일괄 모드를 켜면 모든 지갑을 선택해 준다(편의 기능).
                        if (e.target.checked) setPicked(new Set(wallets.map((w) => w.address)));
                      }}
                    />
                    {t("modal.bulkCreate")}
                  </label>
                  {bulk && (
                    <>
                      <label className="im-field">
                        <input
                          value={bulkName}
                          onChange={(e) => setBulkName(e.target.value)}
                          placeholder={t("modal.newPackageName")}
                        />
                      </label>
                      {bulkCollisions.length > 0 && (
                        <div className="im-info">
                          {t("modal.bulkCollisions", {
                            wallets: bulkCollisions.map(shortAddr).join(", "),
                          })}
                        </div>
                      )}
                    </>
                  )}
                </>
              )}

              {kind === "library" && (
                <>
                  {!isSet && (
                    <label className="im-field">
                      {t("modal.folder")}
                      <select value={packageId} onChange={(e) => setPackageId(e.target.value)}>
                        {libPackages.map((p) => (
                          <option key={p.id} value={p.id}>
                            {p.displayName}
                          </option>
                        ))}
                      </select>
                    </label>
                  )}
                  {isSet && (
                    <p className="im-note">{t("modal.groupedAs", { name })}</p>
                  )}
                  <label className="im-field">
                    <input
                      type="checkbox"
                      checked={applyToAllNow}
                      disabled={wallets.length === 0}
                      onChange={(e) => setApplyToAllNow(e.target.checked)}
                    />
                    {t("modal.applyAllNow", { n: wallets.length })}
                  </label>
                  <label className="im-field">
                    <input
                      type="checkbox"
                      checked={applyToNewWallets}
                      onChange={(e) => setApplyToNewWallets(e.target.checked)}
                    />
                    {t("modal.applyFuture")}
                  </label>
                </>
              )}
            </div>

            {holeReqs.length > 0 && (
              <div className="im-holes">
                <div className="im-holes-head">{t("modal.holesHead")}</div>
                {holeReqs.map((req) => (
                  <div key={req.defId} className="im-holes-def">
                    {holeReqs.length > 1 && <div className="im-holes-defname">{req.defName}</div>}
                    {req.holes.map((h) => {
                      const raw = holeVals[req.defId]?.[h.name] ?? "";
                      const bad = raw.trim() !== "" && holeInputToValue(h.type, raw) === null;
                      return (
                        <label key={h.name} className="im-field im-hole">
                          <span className="im-hole-label">{h.label}</span>
                          {h.type === "addressSet" ? (
                            <textarea
                              value={raw}
                              rows={2}
                              onChange={(e) => setHoleVal(req.defId, h.name, e.target.value)}
                              placeholder={t("modal.holeAddressSetPh")}
                            />
                          ) : (
                            <input
                              value={raw}
                              onChange={(e) => setHoleVal(req.defId, h.name, e.target.value)}
                              placeholder={
                                h.type === "address"
                                  ? "0x…"
                                  : h.type === "decimal"
                                    ? t("modal.holeDecimalPh")
                                    : h.type === "long"
                                      ? t("modal.holeLongPh")
                                      : ""
                              }
                            />
                          )}
                          {bad && (
                            <span className="im-hole-err">
                              {h.type === "address" || h.type === "addressSet"
                                ? t("modal.errAddress")
                                : h.type === "decimal"
                                  ? t("modal.errDecimal")
                                  : t("modal.errFormat")}
                            </span>
                          )}
                        </label>
                      );
                    })}
                  </div>
                ))}
              </div>
            )}

            {mut.isError && <div className="publish-error">{(mut.error as Error).message}</div>}
            <div className="im-actions">
              <button type="button" className="btn-secondary" onClick={() => setKind(null)}>
                {t("modal.back")}
              </button>
              <button
                type="button"
                className="btn-primary"
                disabled={!detailQ.data || !snap || mut.isPending || invalid}
                onClick={() => mut.mutate()}
              >
                {mut.isPending ? t("install.installing") : t("install.get")}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
