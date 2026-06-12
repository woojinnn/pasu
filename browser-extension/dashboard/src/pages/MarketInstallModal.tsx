import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

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
  const ko = locale === "ko";
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
            <h3 className="im-title">{ko ? "받았어요" : "Installed"}</h3>
            <p className="im-sub">
              {kind === "wallet"
                ? ko
                  ? `"${name}"을(를) 지갑 ${picked.size}개에 적용했습니다.`
                  : `Applied "${name}" to ${picked.size} wallet(s).`
                : ko
                  ? `"${name}"을(를) 정책 라이브러리에 추가했습니다.`
                  : `Added "${name}" to your library.`}
            </p>
            <div className="im-actions">
              <button type="button" className="btn-secondary" onClick={onClose}>
                {ko ? "닫기" : "Close"}
              </button>
              <button type="button" className="btn-primary" onClick={() => navigate("/editor?tab=apply")}>
                {ko ? "지갑별 정책 보기" : "View wallet policies"}
              </button>
            </div>
          </>
        ) : kind === null ? (
          <>
            <div className="im-kind">{isSet ? (ko ? "패키지" : "Package") : ko ? "정책" : "Policy"}</div>
            <h3 className="im-title">{name}</h3>
            <p className="im-sub">
              {ko
                ? isSet
                  ? `이 패키지${memberCount ? ` (정책 ${memberCount}개)` : ""}를 어떻게 받을까요?`
                  : "이 정책을 어떻게 받을까요?"
                : "How should this be installed?"}
            </p>
            <div className="im-scope">
              <button
                type="button"
                className="im-opt"
                disabled={wallets.length === 0}
                onClick={() => setKind("wallet")}
              >
                <span className="im-opt-t">{ko ? "지갑 전용으로 받기" : "Wallet-only"}</span>
                <span className="im-opt-d">
                  {ko
                    ? `선택한 지갑에만 존재해요 — 라이브러리에는 보이지 않아요.${wallets.length === 0 ? " (등록된 지갑이 없어요)" : ""}`
                    : "Exists only on the selected wallets."}
                </span>
              </button>
              <button type="button" className="im-opt" onClick={() => setKind("library")}>
                <span className="im-opt-t">{ko ? "라이브러리로 받기" : "Into the library"}</span>
                <span className="im-opt-d">
                  {ko
                    ? "지갑 간 공유되는 템플릿으로 저장돼요 — 지갑별 정책에서 언제든 적용할 수 있어요."
                    : "Saved as a shared template you can apply to wallets later."}
                </span>
              </button>
            </div>
            <div className="im-actions">
              <button type="button" className="btn-secondary" onClick={onClose}>
                {ko ? "취소" : "Cancel"}
              </button>
            </div>
          </>
        ) : (
          <>
            <div className="im-kind">{isSet ? (ko ? "패키지" : "Package") : ko ? "정책" : "Policy"}</div>
            <h3 className="im-title">{name}</h3>
            <p className="im-sub">
              {kind === "wallet"
                ? ko
                  ? "어느 지갑에 적용할까요? 패키지는 지갑마다 따로 골라요."
                  : "Pick wallets — each wallet gets its own package."
                : ko
                  ? "라이브러리 설정을 골라주세요."
                  : "Library options."}
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
                            <span className="im-pkglabel">{ko ? "패키지" : "Package"}</span>
                            <select
                              value={pkgOf(w.address)}
                              onChange={(e) =>
                                setWalletPkg((m) => ({ ...m, [w.address]: e.target.value }))
                              }
                            >
                              <option value={UNCATEGORIZED_PKG}>{ko ? "미분류" : "Uncategorized"}</option>
                              {w.packages.map((p) => (
                                <option key={p.id} value={p.id}>
                                  {p.displayName}
                                </option>
                              ))}
                              <option value="__new__">{ko ? "+ 새 패키지…" : "+ New package…"}</option>
                            </select>
                            {pkgOf(w.address) === "__new__" && (
                              <input
                                value={walletNewName[w.address] ?? ""}
                                onChange={(e) =>
                                  setWalletNewName((m) => ({ ...m, [w.address]: e.target.value }))
                                }
                                placeholder={ko ? "새 패키지 이름" : "Package name"}
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
                    {ko ? "모든 지갑에 새 패키지를 만들어 넣기" : "Create one new package on every wallet"}
                  </label>
                  {bulk && (
                    <>
                      <label className="im-field">
                        <input
                          value={bulkName}
                          onChange={(e) => setBulkName(e.target.value)}
                          placeholder={ko ? "새 패키지 이름" : "Package name"}
                        />
                      </label>
                      {bulkCollisions.length > 0 && (
                        <div className="im-info">
                          {ko
                            ? `같은 이름의 패키지가 이미 있는 지갑은 그 패키지에 넣어요: ${bulkCollisions.map(shortAddr).join(", ")}`
                            : `Wallets with a same-name package reuse it: ${bulkCollisions.map(shortAddr).join(", ")}`}
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
                      {ko ? "폴더" : "Folder"}
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
                    <p className="im-note">
                      {ko ? `"${name}" 패키지로 묶여 설치돼요.` : `Installed grouped as the "${name}" package.`}
                    </p>
                  )}
                  <label className="im-field">
                    <input
                      type="checkbox"
                      checked={applyToAllNow}
                      disabled={wallets.length === 0}
                      onChange={(e) => setApplyToAllNow(e.target.checked)}
                    />
                    {ko ? `지금 모든 지갑에 적용 (${wallets.length}개)` : `Apply to all wallets now (${wallets.length})`}
                  </label>
                  <label className="im-field">
                    <input
                      type="checkbox"
                      checked={applyToNewWallets}
                      onChange={(e) => setApplyToNewWallets(e.target.checked)}
                    />
                    {ko ? "앞으로 추가되는 지갑에도 기본 적용" : "Apply to future wallets by default"}
                  </label>
                </>
              )}
            </div>

            {holeReqs.length > 0 && (
              <div className="im-holes">
                <div className="im-holes-head">
                  {ko
                    ? "게시자가 비워 둔 칸이 있어요 — 채워야 적용돼요"
                    : "This listing has blanks to fill before it can apply"}
                </div>
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
                              placeholder={
                                ko ? "0x… 주소 (쉼표/줄바꿈으로 여러 개)" : "0x… (comma/newline separated)"
                              }
                            />
                          ) : (
                            <input
                              value={raw}
                              onChange={(e) => setHoleVal(req.defId, h.name, e.target.value)}
                              placeholder={
                                h.type === "address"
                                  ? "0x…"
                                  : h.type === "decimal"
                                    ? ko
                                      ? "예: 3.0"
                                      : "e.g. 3.0"
                                    : h.type === "long"
                                      ? ko
                                        ? "숫자"
                                        : "number"
                                      : ""
                              }
                            />
                          )}
                          {bad && (
                            <span className="im-hole-err">
                              {h.type === "address" || h.type === "addressSet"
                                ? ko
                                  ? "0x로 시작하는 40자리 주소여야 해요"
                                  : "Must be a 0x… address"
                                : h.type === "decimal"
                                  ? ko
                                    ? "소수점 형식이어야 해요 (예: 3.0)"
                                    : "Decimal format (e.g. 3.0)"
                                  : ko
                                    ? "형식이 맞지 않아요"
                                    : "Invalid format"}
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
                {ko ? "← 이전" : "← Back"}
              </button>
              <button
                type="button"
                className="btn-primary"
                disabled={!detailQ.data || !snap || mut.isPending || invalid}
                onClick={() => mut.mutate()}
              >
                {mut.isPending ? (ko ? "받는 중…" : "Installing…") : ko ? "받기" : "Install"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
