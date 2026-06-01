import { useMemo, useState } from "react";
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import {
  deleteWallet,
  getDashboardSummary,
  getAuditCounts,
  listAuditVerdicts,
  listFindings,
  listPolicies,
  setVerdictDecision,
  syncWallet,
  type DashboardSummary,
  type DashboardWalletSummary,
  type InstalledPolicy,
  type VerdictDto,
} from "../server-api";

import { AddWalletModal } from "../components/AddWalletModal";
import { RenameWalletModal } from "../components/RenameWalletModal";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { Topbar } from "../shell/Topbar";
import "./home.css";

const MAX_EXPANDED = 3;

/**
 * Home dashboard:
 * - Context bar with workspace summary + 오늘 평가 카운터.
 * - Triage card: unresolved findings (warn/fail), with Editor link + Trust/Cancel.
 * - Wallet list: expandable cards (max 3 open simultaneously). Inside:
 *   active policies (per-user) + recent activity (per-wallet verdicts).
 *
 * Per-wallet verdict aggregation is computed client-side from the latest
 * 24h audit log; this keeps the server contract narrow (one /audit query
 * per wallet) without inventing a dedicated aggregate endpoint.
 */
export function HomePage() {
  const [addOpen, setAddOpen] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [toastMsg, setToastMsg] = useState<string | null>(null);

  const summaryQ = useQuery({ queryKey: ["dashboard", "summary"], queryFn: getDashboardSummary });
  const countsQ = useQuery({
    queryKey: ["audit", "counts", "today"],
    queryFn: () => getAuditCounts({ range: "24h" }),
    refetchInterval: (q) => (q.state.error ? false : 60_000),
    retry: false,
  });
  const findingsQ = useQuery({
    queryKey: ["findings", "home"],
    queryFn: () => listFindings({ limit: 50 }),
    refetchInterval: (q) => (q.state.error ? false : 30_000),
    retry: false,
  });
  const policiesQ = useQuery({ queryKey: ["policies"], queryFn: listPolicies });

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

  const toggle = (addr: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(addr)) {
        next.delete(addr);
        return next;
      }
      if (next.size >= MAX_EXPANDED) {
        setToastMsg(`최대 ${MAX_EXPANDED}개까지 동시에 펼칠 수 있어요`);
        setTimeout(() => setToastMsg(null), 2600);
        return prev;
      }
      next.add(addr);
      return next;
    });
  };

  // Today-evaluated total (PASS+WARN+FAIL).
  const todayTotal = countsQ.data ? countsQ.data.pass + countsQ.data.warn + countsQ.data.fail : null;

  return (
    <>
      <Topbar
        here="Scopeball Home"
        subtitle={summaryQ.data ? `${summaryQ.data.wallet_count} wallets` : "…"}
        counts={countsQ.data}
      />
      <ContextBar
        summary={summaryQ.data}
        loading={summaryQ.isLoading}
        todayTotal={todayTotal}
        onAddWallet={() => setAddOpen(true)}
      />

      <TriageCard findings={findingsQ.data ?? []} loading={findingsQ.isLoading} error={findingsQ.error} />

      <WalletList
        wallets={wallets}
        loading={summaryQ.isLoading}
        error={summaryQ.error}
        agg={walletStatusAgg}
        verdictsByAddr={verdictsByAddr}
        policies={policiesQ.data ?? []}
        expanded={expanded}
        onToggle={toggle}
        onAddWallet={() => setAddOpen(true)}
      />

      <AddWalletModal open={addOpen} onClose={() => setAddOpen(false)} />
      {toastMsg && <div className="toast show">{toastMsg}</div>}
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
        {/* `unresolved_findings` moved to chrome.storage.local along with
            the verdicts table — compute it locally from the same audit
            counts query we already issue (range 24h, verdict warn). */}
        <span className="since">미해결 finding —</span>
        <button className="ctx-cta" type="button" onClick={onAddWallet}>
          지갑 추가 +
        </button>
      </div>
    </div>
  );
}

// ── Triage queue ────────────────────────────────────────────────────────

function TriageCard({
  findings,
  loading,
  error,
}: {
  findings: VerdictDto[];
  loading: boolean;
  error: unknown;
}) {
  const unresolved = findings.filter((f) => f.user_decision === null);
  const fail = unresolved.filter((f) => f.verdict === "fail");
  const warn = unresolved.filter((f) => f.verdict === "warn");

  return (
    <section className="card triage">
      <div className="triage-head">
        <span className="title">
          처리 대기 <span className="cnt">{loading ? "…" : `${unresolved.length}건`}</span>
        </span>
        <div className="sev-row">
          {fail.length > 0 && <span className="sev-pill fail"><span className="pd" />FAIL {fail.length}</span>}
          {warn.length > 0 && <span className="sev-pill warn"><span className="pd" />WARN {warn.length}</span>}
        </div>
      </div>

      {loading && <div className="tq-loading">불러오는 중…</div>}

      {!loading && error ? (
        <div className="tq-empty">
          <span className="ico" style={{ background: "var(--fog-300)", color: "var(--slate-500)" }}>!</span>
          <span className="et">verdict 데이터를 가져올 수 없어요</span>
          <span className="es">
            verdict는 브라우저 익스텐션의 chrome.storage.local에 저장됩니다. 익스텐션이 설치돼 있지 않거나
            트랜잭션을 아직 평가하지 않았을 가능성이 높아요. ({(error as Error)?.message ?? "bridge error"})
          </span>
        </div>
      ) : null}

      {!loading && !error && unresolved.length === 0 && (
        <div className="tq-empty">
          <span className="ico">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round">
              <path d="m5 12 5 5L20 7" />
            </svg>
          </span>
          <span className="et">조치할 항목 없음 · 모두 정상</span>
          <span className="es">새 FAIL/WARN이 생기면 여기에 모입니다.</span>
        </div>
      )}

      {!loading && !error && unresolved.length > 0 && (
        <div className="tq-list">
          {unresolved.map((f) => (
            <TriageRow key={f.id} f={f} />
          ))}
        </div>
      )}
    </section>
  );
}

function TriageRow({ f }: { f: VerdictDto }) {
  const qc = useQueryClient();
  const [trustConfirm, setTrustConfirm] = useState(false);
  const decideMut = useMutation({
    mutationFn: (decision: "trusted" | "cancelled") => setVerdictDecision(f.id, decision),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["findings"] });
      qc.invalidateQueries({ queryKey: ["audit"] });
      qc.invalidateQueries({ queryKey: ["wallet-verdicts"] });
      setTrustConfirm(false);
    },
  });
  const reason = f.reason?.ko ?? f.reason?.en ?? f.policy?.name ?? "—";
  const policyName = f.policy?.name ?? "—";
  const origin = f.dapp_origin ?? "—";
  const wallet = f.wallet ?? "—";
  const fnLabel = f.decoded_fn ?? f.method ?? "—";
  const editorHref = f.policy?.id ? `/editor?policy=${f.policy.id}` : "/editor";

  // FAIL은 "신뢰" 누르려면 confirm 한 번 거치게 (실제로 자금 흐름에 영향).
  // WARN은 빠른 처리 가능.
  const isFail = f.verdict === "fail";

  return (
    <>
      <div className={`tq-row ${f.verdict}`}>
        <span className="tq-pill">
          <span className="pd" />
          {f.verdict.toUpperCase()}
        </span>
        <div className="tq-main">
          <div className="tq-sum">
            <span className="b">{policyName}</span> · {fnLabel}
          </div>
          <div className="tq-meta">
            <span className="wname">{shortAddr(wallet)}</span>
            <span className="dotsep" />
            <span>{origin}</span>
            <span className="dotsep" />
            <span>{timeAgo(f.ts)}</span>
            {f.policy?.id && (
              <>
                <span className="dotsep" />
                <span className="rule-id">policy#{f.policy.id}</span>
              </>
            )}
            <span className="dotsep" />
            <span title={reason} style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", maxWidth: 320 }}>
              {reason}
            </span>
          </div>
        </div>
        <div className="tq-actions">
          <button
            className="btn primary"
            disabled={decideMut.isPending}
            onClick={() => (isFail ? setTrustConfirm(true) : decideMut.mutate("trusted"))}
          >
            신뢰
          </button>
          <button
            className="btn"
            disabled={decideMut.isPending}
            onClick={() => decideMut.mutate("cancelled")}
          >
            {isFail ? "차단 유지" : "무시"}
          </button>
          <Link className="tq-editor" to={editorHref} title="Editor에서 이 정책 열기">
            Editor
            <svg className="arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round">
              <path d="M5 12h14M13 6l6 6-6 6" />
            </svg>
          </Link>
        </div>
      </div>

      <ConfirmDialog
        open={trustConfirm}
        onClose={() => setTrustConfirm(false)}
        onConfirm={() => decideMut.mutate("trusted")}
        title="FAIL verdict 신뢰 처리"
        confirmLabel="신뢰 · 차단 해제"
        tone="fail"
        pending={decideMut.isPending}
        description={
          <>
            <p style={{ marginTop: 0 }}>
              <b>{policyName}</b>이 이 트랜잭션을 <b>FAIL</b>로 차단했습니다.
              "신뢰" 처리하면 이 verdict는 audit log에 신뢰됨으로 기록되며,
              <b> 동일한 향후 패턴은 다시 차단됩니다</b> (정책 자체는 그대로).
            </p>
            <div style={{ background: "var(--fog-200)", padding: "10px 12px", borderRadius: 8, fontSize: 12, marginTop: 8 }}>
              <div><b>지갑</b>: {shortAddr(wallet)}</div>
              <div><b>출처</b>: {origin}</div>
              <div><b>이유</b>: {reason}</div>
            </div>
            <p style={{ fontSize: 11.5, color: "var(--slate-500)", marginBottom: 0 }}>
              정책 자체를 끄려면 Editor에서 disable하세요.
            </p>
          </>
        }
      />
    </>
  );
}

// ── Wallet list ─────────────────────────────────────────────────────────

function WalletList({
  wallets,
  loading,
  error,
  agg,
  verdictsByAddr,
  policies,
  expanded,
  onToggle,
  onAddWallet,
}: {
  wallets: DashboardWalletSummary[];
  loading: boolean;
  error: unknown;
  agg: { pass: number; warn: number; fail: number };
  verdictsByAddr: Map<string, VerdictDto[]>;
  policies: InstalledPolicy[];
  expanded: Set<string>;
  onToggle: (addr: string) => void;
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
        {wallets.length > 1 && (
          <div className="controls" style={{ fontSize: 12, color: "var(--slate-400)" }}>
            최대 {MAX_EXPANDED}개 동시 펼침
          </div>
        )}
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
              policies={policies}
              expanded={expanded.has(w.address)}
              onToggle={() => onToggle(w.address)}
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
  policies,
  expanded,
  onToggle,
}: {
  w: DashboardWalletSummary;
  verdicts: VerdictDto[];
  policies: InstalledPolicy[];
  expanded: boolean;
  onToggle: () => void;
}) {
  const qc = useQueryClient();
  const [renameOpen, setRenameOpen] = useState(false);

  const tone = worstToneOf(verdicts);
  const cardTone =
    tone === "fail" ? "fail" : tone === "warn" || w.pending_count > 0 || w.unlimited_count > 0 ? "warn" : "calm";

  const failCount = verdicts.filter((v) => v.verdict === "fail" && v.user_decision === null).length;
  const warnCount = verdicts.filter((v) => v.verdict === "warn" && v.user_decision === null).length;
  const passToday = verdicts.filter((v) => v.verdict === "pass").length;
  const evaluatedToday = verdicts.length;

  const initial = (w.label ?? w.address).slice(0, 1).toUpperCase();
  const totalUsd = Number(w.total_usd ?? "0");

  const syncMut = useMutation({
    mutationFn: () => syncWallet(w.address),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      qc.invalidateQueries({ queryKey: ["holdings", w.address] });
      qc.invalidateQueries({ queryKey: ["approvals", w.address, "with_risk"] });
      qc.invalidateQueries({ queryKey: ["wallet-verdicts", w.address] });
    },
  });

  const deleteMut = useMutation({
    mutationFn: () => deleteWallet(w.address),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["dashboard"] }),
  });

  const onDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!confirm(`${w.label ?? w.address}를 삭제할까요? holdings·approval·verdict 기록은 유지됩니다.`)) return;
    deleteMut.mutate();
  };

  const onSync = (e: React.MouseEvent) => {
    e.stopPropagation();
    syncMut.mutate();
  };

  const onRename = (e: React.MouseEvent) => {
    e.stopPropagation();
    setRenameOpen(true);
  };

  return (
    <article className={`wallet-card ${cardTone}${expanded ? " expanded" : ""}`}>
      <header
        className="wallet-head"
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        onClick={onToggle}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onToggle();
          }
        }}
      >
        <svg className="caret" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round">
          <path d="m9 6 6 6-6 6" />
        </svg>
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

      <div className="wallet-body">
        <div className="wb-inner">
          <div className="wb-content">
            <ActivePoliciesPanel policies={policies} />
            <ActivityLogPanel verdicts={verdictsByAddrTopN(verdicts, 4)} />
            <div className="wb-foot-meta">
              <span>{policies.length} 정책</span>
              <span className="sep" />
              <span>{evaluatedToday} 평가 today · PASS {passToday}</span>
              <span className="sep" />
              <span className="live">live sync</span>
            </div>
          </div>
        </div>
      </div>

      <div className="wallet-actions">
        <button className="btn" onClick={onSync} disabled={syncMut.isPending} title="balance + price 즉시 동기화">
          {syncMut.isPending ? "동기화 중…" : "지금 동기화"}
        </button>
        <button className="btn" onClick={onRename}>이름 변경</button>
        <button className="btn danger" onClick={onDelete} disabled={deleteMut.isPending}>
          {deleteMut.isPending ? "삭제 중…" : "삭제"}
        </button>
        {syncMut.isSuccess && <span className="sync-result">✓ 완료</span>}
        {syncMut.error && <span className="sync-result" style={{ color: "var(--fail-600)" }}>실패: {String(syncMut.error)}</span>}
      </div>

      <RenameWalletModal
        open={renameOpen}
        onClose={() => setRenameOpen(false)}
        address={w.address}
        initial={w.label}
      />
    </article>
  );
}

// ── Inner panels: active policies + activity log ────────────────────────

function ActivePoliciesPanel({ policies }: { policies: InstalledPolicy[] }) {
  const enabled = policies.filter((p) => p.enabled);
  return (
    <section className="panel">
      <div className="panel-head">
        <span className="panel-ttl">
          활성 정책<b>{enabled.length}</b>
        </span>
        <span className="panel-meta">user-wide</span>
      </div>
      {enabled.length === 0 ? (
        <div style={{ fontSize: 11.5, color: "var(--slate-400)", padding: "4px 4px 8px" }}>
          활성 정책이 없습니다. Editor에서 정책을 만드세요.
        </div>
      ) : (
        <div className="policy-list">
          {enabled.map((p) => {
            const cls = p.severity === "deny" ? "fail" : p.severity === "warn" ? "warn" : "";
            return (
              <div key={p.id} className={`policy-item ${cls}`}>
                <span className="pd" />
                <span className="nm">
                  {p.name}
                  <span className="id">policy#{p.id} · {p.severity}</span>
                </span>
                <span />
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}

function ActivityLogPanel({ verdicts }: { verdicts: VerdictDto[] }) {
  return (
    <section className="panel">
      <div className="panel-head">
        <span className="panel-ttl">
          활동 로그<b>{verdicts.length}</b>
        </span>
        <span className="panel-meta">최근 24h</span>
      </div>
      {verdicts.length === 0 ? (
        <div style={{ fontSize: 11.5, color: "var(--slate-400)", padding: "4px 4px 8px" }}>
          최근 활동이 없습니다.
        </div>
      ) : (
        <div className="hist">
          {verdicts.map((v) => (
            <div key={v.id} className="hist-row">
              <span className={`hist-pill ${v.verdict}`}>{v.verdict.toUpperCase()}</span>
              <span className="h-sum" title={v.reason?.ko ?? v.reason?.en ?? ""}>
                {v.policy?.name ?? v.decoded_fn ?? "—"}{v.dapp_origin ? ` · ${v.dapp_origin}` : ""}
              </span>
              <span className="h-time">{timeAgo(v.ts)}</span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

// ── helpers ─────────────────────────────────────────────────────────────

function worstToneOf(verdicts: VerdictDto[]): "calm" | "warn" | "fail" {
  const open = verdicts.filter((v) => v.user_decision === null);
  if (open.some((v) => v.verdict === "fail")) return "fail";
  if (open.some((v) => v.verdict === "warn")) return "warn";
  return "calm";
}

function verdictsByAddrTopN(verdicts: VerdictDto[], n: number): VerdictDto[] {
  return verdicts.slice(0, n);
}

function shortAddr(addr: string): string {
  if (!addr || addr.length < 12) return addr;
  return `${addr.slice(0, 6)}···${addr.slice(-4)}`;
}

function timeAgo(unixSec: number): string {
  const diff = Math.max(0, Math.floor(Date.now() / 1000) - unixSec);
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}
