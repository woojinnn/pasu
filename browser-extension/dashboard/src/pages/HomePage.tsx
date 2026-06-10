import { useEffect, useMemo, useState } from "react";
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";

import {
  deleteWallet,
  getDashboardSummary,
  getAuditCounts,
  listAuditVerdicts,
  subscribeToBroadcast,
  syncWallet,
  type DashboardSummary,
  type DashboardWalletSummary,
  type VerdictDto,
} from "../server-api";
import { getOverview, isEffectiveOn, updateBinding } from "../server-api/policy-store";

import { AddWalletModal } from "../components/AddWalletModal";
import { RenameWalletModal } from "../components/RenameWalletModal";
import { Topbar } from "../shell/Topbar";
import "./home.css";

/**
 * Home dashboard:
 * - Context bar with workspace summary + 오늘 평가 카운터.
 * - Active-policies card (links into /editor).
 * - Wallet list: static cards (per-wallet tone driven by 24h verdict log).
 *
 * Per-wallet verdict aggregation is computed client-side from the latest
 * 24h audit log so the wallet card can colour its tone pill; the heavy
 * activity log + policy list previously rendered inside the expanded
 * card moved to the dedicated /history and /editor pages.
 */
interface RelatedPolicy {
  bindingId: string;
  defId: string;
  name: string;
  enabled: boolean;
  effective: boolean;
}

export function HomePage() {
  const [addOpen, setAddOpen] = useState(false);
  const qc = useQueryClient();

  const summaryQ = useQuery({ queryKey: ["dashboard", "summary"], queryFn: getDashboardSummary });
  const countsQ = useQuery({
    queryKey: ["audit", "counts", "today"],
    queryFn: () => getAuditCounts({ range: "24h" }),
    refetchInterval: (q) => (q.state.error ? false : 60_000),
    retry: false,
  });
  // 에디터/popup과 같은 ps2 스토어를 읽어 어느 표면이 토글하든 수치가 일치한다.
  const overviewQ = useQuery({ queryKey: ["ps2-overview"], queryFn: getOverview });

  // popup 등 다른 컨텍스트가 ps2 키를 쓰면 재조회.
  useEffect(() => {
    const unsubscribe = subscribeToBroadcast((keys) => {
      if (keys.some((k) => k.startsWith("ps2:"))) {
        void qc.invalidateQueries({ queryKey: ["ps2-overview"] });
      }
    });
    return unsubscribe;
  }, [qc]);

  const wallets = summaryQ.data?.wallets ?? [];

  // Per-wallet verdict aggregation (counts + activity rows).
  // Single extension-bridge verdict query per wallet, 24h window, limit 50.
  const walletVerdictsQs = useQueries({
    queries: wallets.map((w) => ({
      queryKey: ["wallet-verdicts", w.address],
      queryFn: () => listAuditVerdicts({ wallet: w.address, range: "24h" as const, limit: 50 }),
      enabled: summaryQ.isSuccess,
      refetchInterval: 60_000,
      retry: false,
    })),
  });
  const verdictsByAddr = useMemo(() => {
    const map = new Map<string, VerdictDto[]>();
    wallets.forEach((w, i) => map.set(w.address, walletVerdictsQs[i]?.data ?? []));
    return map;
  }, [wallets, walletVerdictsQs]);

  // Aggregate PASS/WARN/FAIL for the wallets-head submeta.
  const walletStatusAgg = useMemo(() => {
    let pass = 0;
    let warn = 0;
    let fail = 0;
    wallets.forEach((w) => {
      const v = verdictsByAddr.get(w.address) ?? [];
      const tone = worstToneOf(v);
      if (tone === "fail") fail++;
      else if (tone === "warn") warn++;
      else pass++;
    });
    return { pass, warn, fail };
  }, [wallets, verdictsByAddr]);

  // Today-evaluated total (PASS+WARN+FAIL).
  const todayTotal = countsQ.data ? countsQ.data.pass + countsQ.data.warn + countsQ.data.fail : null;

  const snap = overviewQ.data ?? null;
  const totalManagedCount = snap ? Object.keys(snap.library.defs).length : 0;
  // "적용 중" = effective 바인딩(패키지∧개별 토글 on)이 하나라도 있는 정의 수.
  const enabledPolicyCount = useMemo(() => {
    if (!snap) return 0;
    const active = new Set<string>();
    for (const w of Object.values(snap.wallets.byAddress)) {
      for (const b of Object.values(w.bindings)) {
        if (isEffectiveOn(w, b)) active.add(b.defId);
      }
    }
    return active.size;
  }, [snap]);
  const policiesLoading = overviewQ.isLoading;

  // 지갑 카드의 관련 정책 = 그 지갑의 바인딩(정의 이름 + effective 상태).
  const relatedByWallet = useMemo(() => {
    const m = new Map<string, RelatedPolicy[]>();
    if (!snap) return m;
    for (const w of wallets) {
      const ws = snap.wallets.byAddress[w.address.toLowerCase()];
      if (!ws) {
        m.set(w.address, []);
        continue;
      }
      m.set(
        w.address,
        Object.values(ws.bindings)
          .map((b) => ({
            bindingId: b.id,
            defId: b.defId,
            name: snap.library.defs[b.defId]?.displayName ?? b.defId,
            enabled: b.enabled,
            effective: isEffectiveOn(ws, b),
          }))
          .sort((a, b) => a.name.localeCompare(b.name, "ko")),
      );
    }
    return m;
  }, [wallets, snap]);

  const toggleMut = useMutation({
    mutationFn: (v: { address: string; bindingId: string; on: boolean }) =>
      updateBinding({ address: v.address, bindingId: v.bindingId, patch: { enabled: v.on } }),
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ["ps2-overview"] });
    },
  });
  const togglePolicy = (address: string, bindingId: string, on: boolean) => {
    toggleMut.mutate({ address: address.toLowerCase(), bindingId, on });
  };

  return (
    <>
      <Topbar
        here="Pasu Home"
        subtitle={summaryQ.data ? `${summaryQ.data.wallet_count} wallets` : "…"}
        counts={countsQ.data}
      />
      <ContextBar
        summary={summaryQ.data}
        loading={summaryQ.isLoading}
        todayTotal={todayTotal}
        onAddWallet={() => setAddOpen(true)}
      />

      <div className="active-cards">
        <ActivePoliciesCard
          label="활성 정책"
          enabledCount={enabledPolicyCount}
          totalCount={totalManagedCount}
          loading={policiesLoading}
        />
      </div>

      <WalletList
        wallets={wallets}
        loading={summaryQ.isLoading}
        error={summaryQ.error}
        agg={walletStatusAgg}
        verdictsByAddr={verdictsByAddr}
        relatedByWallet={relatedByWallet}
        onTogglePolicy={togglePolicy}
        onAddWallet={() => setAddOpen(true)}
      />

      <AddWalletModal open={addOpen} onClose={() => setAddOpen(false)} />
    </>
  );
}

// ── Context bar ─────────────────────────────────────────────────────────

function ContextBar({
  summary,
  loading,
  todayTotal,
  onAddWallet,
}: {
  summary?: DashboardSummary;
  loading: boolean;
  todayTotal: number | null;
  onAddWallet: () => void;
}) {
  if (loading || !summary) {
    return (
      <div className="ctx-bar">
        <span className="pulse" />
        <span className="summary">workspace 정보 가져오는 중…</span>
      </div>
    );
  }
  const totalUsd = Number(summary.total_portfolio_usd ?? "0");
  return (
    <div className="ctx-bar">
      <span className="protected">
        <span className="pulse" />
        보호 중
      </span>
      <span className="sep" />
      <span className="summary">
        <b>{summary.wallet_count}</b> 지갑
        <span className="mute"> · 포트폴리오 ${totalUsd.toLocaleString("en-US", { maximumFractionDigits: 0 })}</span>
      </span>
      <div className="right">
        {todayTotal !== null && (
          <span className="since">오늘 {todayTotal}건 평가</span>
        )}
        <button className="ctx-cta" type="button" onClick={onAddWallet}>
          지갑 추가 +
        </button>
      </div>
    </div>
  );
}

// ── Active policies / packages card ─────────────────────────────────────

function ActivePoliciesCard({
  label,
  enabledCount,
  totalCount,
  loading,
}: {
  label: string;
  enabledCount: number;
  totalCount: number;
  loading: boolean;
}) {
  return (
    <Link to="/editor" className="policies-card" aria-label={`${label} — 정책 편집기로 이동`}>
      <div className="pc-left">
        <span className="pc-label">{label}</span>
        <span className="pc-count">
          {loading ? "…" : enabledCount}
          {!loading && totalCount > 0 && (
            <span className="pc-total"> / {totalCount}</span>
          )}
        </span>
      </div>
      <div className="pc-right">
        <span className="pc-cta">Editor</span>
        <svg className="pc-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round">
          <path d="M5 12h14M13 6l6 6-6 6" />
        </svg>
      </div>
    </Link>
  );
}

// ── Wallet list ─────────────────────────────────────────────────────────

function WalletList({
  wallets,
  loading,
  error,
  agg,
  verdictsByAddr,
  relatedByWallet,
  onTogglePolicy,
  onAddWallet,
}: {
  wallets: DashboardWalletSummary[];
  loading: boolean;
  error: unknown;
  agg: { pass: number; warn: number; fail: number };
  verdictsByAddr: Map<string, VerdictDto[]>;
  relatedByWallet: Map<string, RelatedPolicy[]>;
  onTogglePolicy: (address: string, bindingId: string, on: boolean) => void;
  onAddWallet: () => void;
}) {
  return (
    <>
      <div className="wallets-head">
        <h2>
          내 지갑들
          <span className="submeta">{wallets.length}개</span>
          {wallets.length > 0 && (
            <span className="submeta-chips">
              <span className="chip pass">PASS {agg.pass}</span>
              {agg.warn > 0 && <span className="chip warn">WARN {agg.warn}</span>}
              {agg.fail > 0 && <span className="chip fail">FAIL {agg.fail}</span>}
            </span>
          )}
        </h2>
      </div>
      {error ? <div className="err-banner">dashboard 불러오기 실패: {String(error)}</div> : null}
      {loading && (
        <div className="wallet-list">
          <div className="skeleton" style={{ height: 64 }} />
          <div className="skeleton" style={{ height: 64 }} />
        </div>
      )}
      {!loading && wallets.length === 0 && (
        <div className="tq-empty">
          <span className="et">등록된 지갑이 없습니다</span>
          <span className="es">아래 버튼으로 첫 지갑을 추가하세요. ERC-20 보유 토큰이 자동으로 디스커버됩니다.</span>
          <button className="btn primary" style={{ marginTop: 14 }} onClick={onAddWallet}>
            지갑 추가 +
          </button>
        </div>
      )}
      {!loading && wallets.length > 0 && (
        <div className="wallet-list">
          {wallets.map((w) => (
            <WalletCard
              key={w.address}
              w={w}
              verdicts={verdictsByAddr.get(w.address) ?? []}
              related={relatedByWallet.get(w.address) ?? []}
              onTogglePolicy={onTogglePolicy}
            />
          ))}
        </div>
      )}
    </>
  );
}

function WalletCard({
  w,
  verdicts,
  related,
  onTogglePolicy,
}: {
  w: DashboardWalletSummary;
  verdicts: VerdictDto[];
  related: RelatedPolicy[];
  onTogglePolicy: (address: string, bindingId: string, on: boolean) => void;
}) {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [renameOpen, setRenameOpen] = useState(false);
  const [policiesOpen, setPoliciesOpen] = useState(false);

  // Click the wallet identity row → open this wallet in the Assets tab,
  // pre-selected (`?wallet=`). The action buttons below sit in a separate row
  // so they don't trigger navigation.
  const openInAssets = () => navigate(`/monitoring?wallet=${w.address}`);

  const tone = worstToneOf(verdicts);
  const cardTone =
    tone === "fail" ? "fail" : tone === "warn" || w.pending_count > 0 || w.unlimited_count > 0 ? "warn" : "calm";

  const failCount = verdicts.filter((v) => v.verdict === "fail" && v.user_decision === null).length;
  const warnCount = verdicts.filter((v) => v.verdict === "warn" && v.user_decision === null).length;

  const initial = (w.label ?? w.address).slice(0, 1).toUpperCase();
  const totalUsd = Number(w.total_usd ?? "0");

  const syncMut = useMutation({
    mutationFn: () => syncWallet(w.address),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      qc.invalidateQueries({ queryKey: ["holdings", w.address] });
      qc.invalidateQueries({ queryKey: ["approvals", w.address, "with_risk"] });
      qc.invalidateQueries({ queryKey: ["positions", w.address] });
      qc.invalidateQueries({ queryKey: ["pending", w.address] });
      qc.invalidateQueries({ queryKey: ["wallet-verdicts", w.address] });
    },
  });

  const deleteMut = useMutation({
    mutationFn: () => deleteWallet(w.address),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["dashboard"] }),
  });

  const onDelete = () => {
    if (!confirm(`${w.label ?? w.address}를 삭제할까요? holdings·approval·verdict 기록은 유지됩니다.`)) return;
    deleteMut.mutate();
  };

  return (
    <article className={`wallet-card ${cardTone}`}>
      <header
        className="wallet-head"
        role="button"
        tabIndex={0}
        style={{ cursor: "pointer" }}
        title="Assets 탭에서 이 지갑 열기"
        onClick={openInAssets}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            openInAssets();
          }
        }}
      >
        <span className="w-icon">{initial}</span>
        <div className="w-id">
          <span className="name">{w.label ?? "—"}</span>
          <span className="addr">{w.address}</span>
        </div>
        <div className="w-stats">
          <span>
            <b>${totalUsd.toLocaleString("en-US", { maximumFractionDigits: 0 })}</b>
          </span>
        </div>
        <div className="w-status">
          {failCount > 0 && <span className="w-pill fail"><span className="pd" />FAIL {failCount}</span>}
          {warnCount > 0 && <span className="w-pill warn"><span className="pd" />WARN {warnCount}</span>}
          {failCount === 0 && warnCount === 0 && (
            <span className="w-pill calm"><span className="pd" />CALM</span>
          )}
        </div>
        {(w.pending_count > 0 || w.unlimited_count > 0) && (
          <span className="w-pending">
            {w.pending_count > 0 && `${w.pending_count} pending`}
            {w.pending_count > 0 && w.unlimited_count > 0 && " · "}
            {w.unlimited_count > 0 && `${w.unlimited_count} 무제한`}
          </span>
        )}
      </header>

      <div className="wallet-actions">
        <button className="btn" onClick={() => syncMut.mutate()} disabled={syncMut.isPending} title="balance + price 즉시 동기화">
          {syncMut.isPending ? "동기화 중…" : "지금 동기화"}
        </button>
        <button className="btn" onClick={() => setRenameOpen(true)}>이름 변경</button>
        <button className="btn danger" onClick={onDelete} disabled={deleteMut.isPending}>
          {deleteMut.isPending ? "삭제 중…" : "삭제"}
        </button>
        {syncMut.isSuccess && <span className="sync-result">✓ 완료</span>}
        {syncMut.error && <span className="sync-result" style={{ color: "var(--fail-600)" }}>실패: {String(syncMut.error)}</span>}
      </div>

      {related.length > 0 && (
        <div className="wallet-policies">
          <button
            type="button"
            className="wp-head"
            onClick={() => setPoliciesOpen((v) => !v)}
            aria-expanded={policiesOpen}
          >
            <span className={`wp-caret${policiesOpen ? " open" : ""}`}>▶</span>
            이 지갑 관련 정책 <b>{related.length}</b>
            <span className="wp-onoff">
              {related.filter((p) => p.effective).length}/{related.length} 켜짐
            </span>
          </button>
          {policiesOpen &&
            related.map((p) => {
              const on = p.enabled;
              return (
                <div key={p.bindingId} className={`wp-row${p.effective ? "" : " off"}`}>
                  <Link to={`/editor/${encodeURIComponent(p.defId)}`} className="wp-name" title="에디터에서 열기">
                    {p.name}
                  </Link>
                  <button
                    type="button"
                    className={`wp-tg${on ? " on" : ""}`}
                    onClick={() => onTogglePolicy(w.address, p.bindingId, !on)}
                    title={on ? "이 정책 끄기" : "이 정책 켜기"}
                    aria-pressed={on}
                  >
                    <span className="sw" />
                  </button>
                </div>
              );
            })}
        </div>
      )}

      <RenameWalletModal
        open={renameOpen}
        onClose={() => setRenameOpen(false)}
        address={w.address}
        initial={w.label}
      />
    </article>
  );
}

// ── helpers ─────────────────────────────────────────────────────────────

function worstToneOf(verdicts: VerdictDto[]): "calm" | "warn" | "fail" {
  const open = verdicts.filter((v) => v.user_decision === null);
  if (open.some((v) => v.verdict === "fail")) return "fail";
  if (open.some((v) => v.verdict === "warn")) return "warn";
  return "calm";
}
