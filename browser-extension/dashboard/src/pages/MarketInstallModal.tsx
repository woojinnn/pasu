import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { getListing, pickI18n, type ListingSummary } from "../server-api";
import { getOverview, UNCATEGORIZED_PKG, type MarketInstallScope } from "../server-api/policy-store";
import { listWallets } from "../server-api/wallets";
import { installListingV2 } from "./market-install-v2";
import type { MarketLocale } from "./market-locale";

/** "받기" 모달 v2 — 적용 범위(지갑/모든 지갑/라이브러리만) + 패키지를 고르고
 *  ps2:install-market로 설치한다. 마켓 목록/상세 두 진입점이 공유. */
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

  const wallets = useMemo(() => {
    const addrs = new Set([
      ...(walletsQ.data ?? []).map((w) => w.address.toLowerCase()),
      ...Object.keys(overviewQ.data?.wallets.byAddress ?? {}),
    ]);
    return [...addrs].sort();
  }, [walletsQ.data, overviewQ.data]);
  const packages = useMemo(
    () => Object.values(overviewQ.data?.library.packages ?? {}),
    [overviewQ.data],
  );

  const isSet = listing.kind === "set";
  const name = pickI18n(listing.display_name, locale) || listing.slug;
  const memberCount = detailQ.data?.latest_version?.members?.length ?? 0;

  const [mode, setMode] = useState<"wallets" | "all" | "library-only">("wallets");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [packageId, setPackageId] = useState(UNCATEGORIZED_PKG);
  const [applyToNewWallets, setApplyToNewWallets] = useState(true);
  const [done, setDone] = useState(false);

  const mut = useMutation({
    mutationFn: async () => {
      const scope: MarketInstallScope =
        mode === "library-only"
          ? { kind: "library-only" }
          : mode === "all"
            ? { kind: "all" }
            : { kind: "wallets", addresses: [...picked] };
      return installListingV2(detailQ.data!, locale, {
        scope,
        applyToNewWallets,
        packageId: isSet ? null : packageId === UNCATEGORIZED_PKG ? null : packageId,
      });
    },
    onSuccess: async () => {
      setDone(true);
      await qc.invalidateQueries({ queryKey: ["ps2-overview"] });
      await qc.invalidateQueries({ queryKey: ["market-listing", listing.slug] });
    },
  });

  const invalid = mode === "wallets" && picked.size === 0;
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
        {!done ? (
          <>
            <div className="im-kind">{isSet ? (ko ? "패키지" : "Package") : ko ? "정책" : "Policy"}</div>
            <h3 className="im-title">{name}</h3>
            <p className="im-sub">
              {ko
                ? isSet
                  ? `이 패키지${memberCount ? ` (정책 ${memberCount}개)` : ""}를 어디에 적용할까요?`
                  : "이 정책을 어디에 적용할까요?"
                : "Where should this apply?"}
            </p>

            <div className="im-scope">
              <label className="im-field">
                <input
                  type="radio"
                  name="mim-scope"
                  checked={mode === "wallets"}
                  disabled={wallets.length === 0}
                  onChange={() => setMode("wallets")}
                />
                {ko ? "선택한 지갑에 적용" : "Selected wallets"}
              </label>
              {mode === "wallets" && (
                <div className="im-wallets">
                  {wallets.map((a) => (
                    <label key={a} className="im-field">
                      <input type="checkbox" checked={picked.has(a)} onChange={() => togglePick(a)} />
                      <span className="im-addr">{a}</span>
                    </label>
                  ))}
                  {wallets.length === 0 && (
                    <div className="im-note">{ko ? "등록된 지갑이 없어요" : "No wallets yet"}</div>
                  )}
                </div>
              )}
              <label className="im-field">
                <input
                  type="radio"
                  name="mim-scope"
                  checked={mode === "all"}
                  disabled={wallets.length === 0}
                  onChange={() => setMode("all")}
                />
                {ko ? `모든 지갑에 적용 (${wallets.length}개)` : `All wallets (${wallets.length})`}
              </label>
              <label className="im-field">
                <input
                  type="radio"
                  name="mim-scope"
                  checked={mode === "library-only"}
                  onChange={() => setMode("library-only")}
                />
                {ko ? "라이브러리에만 저장 (나중에 적용)" : "Library only"}
              </label>

              {!isSet && (
                <label className="im-field">
                  {ko ? "패키지" : "Package"}
                  <select value={packageId} onChange={(e) => setPackageId(e.target.value)}>
                    {packages.map((p) => (
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
                  checked={applyToNewWallets}
                  onChange={(e) => setApplyToNewWallets(e.target.checked)}
                />
                {ko ? "앞으로 추가되는 지갑에도 기본 적용" : "Apply to future wallets by default"}
              </label>
            </div>

            {mut.isError && <div className="publish-error">{(mut.error as Error).message}</div>}
            <div className="im-actions">
              <button type="button" className="btn-secondary" onClick={onClose}>
                {ko ? "취소" : "Cancel"}
              </button>
              <button
                type="button"
                className="btn-primary"
                disabled={!detailQ.data || mut.isPending || invalid}
                onClick={() => mut.mutate()}
              >
                {mut.isPending ? (ko ? "받는 중…" : "Installing…") : ko ? "받기" : "Install"}
              </button>
            </div>
          </>
        ) : (
          <>
            <div className="im-ok">✓</div>
            <h3 className="im-title">{ko ? "받았어요" : "Installed"}</h3>
            <p className="im-sub">
              {ko ? `"${name}"을(를) 정책 라이브러리에 추가했습니다.` : `Added "${name}" to your library.`}
            </p>
            <div className="im-actions">
              <button type="button" className="btn-secondary" onClick={onClose}>
                {ko ? "닫기" : "Close"}
              </button>
              <button type="button" className="btn-primary" onClick={() => navigate("/editor?tab=apply")}>
                {ko ? "적용 현황 보기" : "View apply status"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
