import { useEffect, useMemo, useState } from "react";
import { useMutation, useQueries, useQuery } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";

import {
  getDashboardSummary,
  getWalletApprovalsWithRisk,
  getWalletHoldings,
  getWalletPending,
  getWalletPositions,
  hlAccountOf,
  listAuditVerdicts,
  setVerdictDecision,
  type ClassifiedApprovals,
  type ClassifiedErc20Approval,
  type ClassifiedPermit2Approval,
  type ClassifiedSetForAllApproval,
  type DashboardSummary,
  type DashboardWalletSummary,
  type HlAccount,
  type PendingKind,
  type PendingTx,
  type Position,
  type TokenHolding,
  type VerdictDto,
} from "../server-api";

import { AddWalletModal } from "../components/AddWalletModal";
import { Modal } from "../components/Modal";
import {
  planRevokesLocal,
  type RevokeItem,
  type RevokePlanResp,
} from "../tools/revoke-plan";
import { Topbar } from "../shell/Topbar";
import "./monitoring.css";

/**
 * Monitoring page — multi-wallet portfolio view.
 *
 * Two visual modes driven by `?wallet=` URL param (and the wallet
 * switcher):
 *
 *   `all`    → L1 layout: portfolio summary, chain breakdown, cross-wallet
 *              aggregated holdings, all-wallet approvals. Lens toggle
 *              re-sorts holdings by risk vs. USD.
 *   single   → L2 layout: per-wallet header band (FAIL/WARN/CALM + VaR
 *              + unlimited count), action queue (urgent findings + risky
 *              approvals), holdings, approvals. Closer to the original
 *              front/pasu-v3 drilldown experience.
 *
 * Risk overlay (UNLIMITED / BLOCKED inline on a holding) and VaR
 * (= min(allowance, balance) × price) are computed client-side by
 * joining approvals into holdings on `(chain, contract-address)`.
 */
export function MonitoringPage() {
  const [params, setParams] = useSearchParams();
  const [sel, setSel] = useState<"all" | string>(() => params.get("wallet") ?? "all");
  const [addOpen, setAddOpen] = useState(false);
  const [lens, setLens] = useState<"assets" | "risk">("assets");
  const [bannerDismissed, setBannerDismissed] = useState(false);

  const summaryQ = useQuery({ queryKey: ["dashboard", "summary"], queryFn: getDashboardSummary });
  const wallets = summaryQ.data?.wallets ?? [];

  useEffect(() => {
    const want = params.get("wallet");
    if (want && want !== sel) setSel(want);
    if (!want && sel !== "all") setSel("all");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params]);

  const setSelectionAndUrl = (next: "all" | string) => {
    setSel(next);
    const p = new URLSearchParams(params);
    if (next === "all") p.delete("wallet");
    else p.set("wallet", next);
    setParams(p, { replace: true });
  };

  const targetWallets = useMemo(() => {
    if (sel === "all") return wallets;
    return wallets.filter((w) => w.address === sel);
  }, [sel, wallets]);

  const holdingsQs = useQueries({
    queries: targetWallets.map((w) => ({
      queryKey: ["holdings", w.address],
      queryFn: () => getWalletHoldings(w.address),
      enabled: summaryQ.isSuccess,
    })),
  });
  const approvalsQs = useQueries({
    queries: targetWallets.map((w) => ({
      queryKey: ["approvals", w.address, "with_risk"],
      queryFn: () => getWalletApprovalsWithRisk(w.address),
      enabled: summaryQ.isSuccess,
    })),
  });
  const positionsQs = useQueries({
    queries: targetWallets.map((w) => ({
      queryKey: ["positions", w.address],
      queryFn: () => getWalletPositions(w.address),
      enabled: summaryQ.isSuccess,
      // Re-read the server's stored state every 30s so HL positions/orders
      // reflect backend syncs without a manual refresh. (Fresh HL data still
      // requires a backend sync — `POST /sync` or the sync_worker tick.)
      refetchInterval: 30_000,
    })),
  });
  const pendingQs = useQueries({
    queries: targetWallets.map((w) => ({
      queryKey: ["pending", w.address],
      queryFn: () => getWalletPending(w.address),
      enabled: summaryQ.isSuccess,
      refetchInterval: 30_000,
    })),
  });

  // Per-wallet recent verdicts — only fetched in L2 mode (single wallet).
  const findingsQ = useQuery({
    queryKey: ["wallet-findings", sel],
    queryFn: () => listAuditVerdicts({ wallet: sel, range: "24h", limit: 50 }),
    enabled: sel !== "all",
  });

  // Per-wallet approval+holding index — used for risk overlay + VaR.
  const indexes = useMemo(() => buildApprovalIndexes(targetWallets, approvalsQs.map((q) => q.data)), [
    targetWallets,
    approvalsQs,
  ]);

  const aggregateSummary = useMemo(() => aggregate(targetWallets), [targetWallets]);

  // FAIL signal across the current view — used by the risk suggest banner.
  const totalFailRows = useMemo(() => {
    let n = 0;
    targetWallets.forEach((w, i) => {
      const apIdx = indexes.get(w.address);
      const holdings = holdingsQs[i]?.data ?? [];
      holdings.forEach((h) => {
        const risk = riskTagsFor(h, apIdx);
        if (risk.includes("BLOCKED")) n++;
      });
    });
    return n;
  }, [targetWallets, holdingsQs, indexes]);

  // Empty state — no wallets tracked at all.
  if (summaryQ.isSuccess && wallets.length === 0) {
    return (
      <>
        <div className="card" style={{ padding: 36, textAlign: "center" }}>
          <p style={{ margin: "0 0 8px 0", fontSize: 14, color: "var(--slate-700)", fontWeight: 600 }}>
            추적 중인 지갑이 없습니다
          </p>
          <p style={{ margin: "0 0 16px 0", fontSize: 12, color: "var(--slate-500)" }}>
            지갑을 추가하면 holdings · approvals · 체인 분포가 여기에 나타납니다.
          </p>
          <button className="btn primary" onClick={() => setAddOpen(true)}>
            지갑 추가 +
          </button>
        </div>
        <AddWalletModal open={addOpen} onClose={() => setAddOpen(false)} />
      </>
    );
  }

  const isL2 = sel !== "all";
  const selectedWallet = isL2 ? wallets.find((w) => w.address === sel) : null;

  return (
    <>
      <Topbar
        here="Assets"
        subtitle={
          isL2
            ? `${selectedWallet?.label ?? shortAddr(sel)}`
            : `${wallets.length} wallets · ${summaryQ.data?.chain_breakdown.length ?? 0} chains`
        }
      />
      <WalletSwitch sel={sel} setSel={setSelectionAndUrl} wallets={wallets} loading={summaryQ.isLoading} />

      {!isL2 && <SummaryBar agg={aggregateSummary} loading={summaryQ.isLoading} />}

      {isL2 && selectedWallet && (
        <L2HeaderBand
          wallet={selectedWallet}
          findings={findingsQ.data ?? []}
          holdings={holdingsQs[0]?.data ?? []}
          apIdx={indexes.get(selectedWallet.address)}
        />
      )}

      {!isL2 && <ChainBreakdown summary={summaryQ.data} loading={summaryQ.isLoading} />}

      {!isL2 && (
        <WalletAssetRatio
          wallets={wallets}
          loading={summaryQ.isLoading}
          onWalletClick={(addr) => setSelectionAndUrl(addr)}
        />
      )}

      {/* Lens + risk suggest banner */}
      <div className="lens-row">
        <LensToggle lens={lens} setLens={setLens} />
        {!isL2 && (
          <span className="meta" style={{ marginLeft: "auto", fontSize: 12, color: "var(--slate-400)" }}>
            정렬: {lens === "risk" ? "위험 우선순위" : "USD 평가액"}
          </span>
        )}
      </div>

      {lens === "assets" && totalFailRows > 0 && !bannerDismissed && (
        <RiskSuggestBanner
          failCount={totalFailRows}
          onSwitch={() => setLens("risk")}
          onDismiss={() => setBannerDismissed(true)}
        />
      )}

      {isL2 && selectedWallet && (
        <ActionQueueCard
          wallet={selectedWallet}
          findings={findingsQ.data ?? []}
          loading={findingsQ.isLoading}
          apIdx={indexes.get(selectedWallet.address)}
        />
      )}

      <div className="sec-head">
        <h3>Holdings</h3>
        <span className="meta">
          {targetWallets.length} 지갑 · {holdingsQs.reduce((n, q) => n + (q.data?.length ?? 0), 0)} 토큰
        </span>
      </div>
      <HoldingsTable
        wallets={targetWallets}
        queries={holdingsQs}
        indexes={indexes}
        lens={lens}
        onWalletClick={(addr) => setSelectionAndUrl(addr)}
      />

      <div className="sec-head">
        <h3>Approvals (risk overlay)</h3>
        <span className="meta">UNLIMITED · KNOWN_VENUE · BLOCKED · OLD</span>
      </div>
      <ApprovalsTable wallets={targetWallets} queries={approvalsQs} />

      <div className="sec-head">
        <h3>Hyperliquid</h3>
        <span className="meta">포지션 · 레버리지 · 오픈 오더 · 마진</span>
      </div>
      <HyperliquidSection wallets={targetWallets} queries={positionsQs} />

      <div className="sec-head">
        <h3>대기 주문 (오프체인 인텐트 · 서명)</h3>
        <span className="meta">UniswapX · CoW · 1inch · permit</span>
      </div>
      <PendingTable wallets={targetWallets} queries={pendingQs} />

      <AddWalletModal open={addOpen} onClose={() => setAddOpen(false)} />
    </>
  );
}

// ── Wallet switcher ─────────────────────────────────────────────────────

function WalletSwitch({
  sel,
  setSel,
  wallets,
  loading,
}: {
  sel: "all" | string;
  setSel: (v: "all" | string) => void;
  wallets: DashboardWalletSummary[];
  loading: boolean;
}) {
  if (loading) return <div className="wallet-switch"><span className="ws-chip">불러오는 중…</span></div>;
  return (
    <div className="wallet-switch" role="group" aria-label="wallet switcher">
      <button className={`ws-chip${sel === "all" ? " on" : ""}`} onClick={() => setSel("all")}>
        전체 합산
        <span className="ws-amt">{wallets.length}</span>
      </button>
      {wallets.map((w) => {
        const status: "fail" | "warn" | "calm" =
          w.unlimited_count > 0 ? "warn" : w.pending_count > 0 ? "warn" : "calm";
        return (
          <button
            key={w.address}
            className={`ws-chip${sel === w.address ? " on" : ""}`}
            onClick={() => setSel(w.address)}
          >
            <span className={`ws-dot ${status}`} />
            {w.label ?? shortAddr(w.address)}
            <span className="ws-amt">{shortAddr(w.address)}</span>
          </button>
        );
      })}
    </div>
  );
}

// ── Summary bar (L1, 3 cells) ───────────────────────────────────────────

interface Aggregate {
  totalUsd: number;
  unlimited: number;
  pending: number;
  walletCount: number;
}

function aggregate(rows: DashboardWalletSummary[]): Aggregate {
  return rows.reduce(
    (acc, w) => ({
      totalUsd: acc.totalUsd + Number(w.total_usd ?? "0"),
      unlimited: acc.unlimited + w.unlimited_count,
      pending: acc.pending + w.pending_count,
      walletCount: acc.walletCount + 1,
    }),
    { totalUsd: 0, unlimited: 0, pending: 0, walletCount: 0 } as Aggregate,
  );
}

function SummaryBar({ agg, loading }: { agg: Aggregate; loading: boolean }) {
  if (loading) {
    return (
      <div className="summary-bar">
        <div className="sum-cell"><div className="skeleton-row" style={{ width: "60%" }} /></div>
        <div className="sum-cell"><div className="skeleton-row" style={{ width: "60%" }} /></div>
        <div className="sum-cell"><div className="skeleton-row" style={{ width: "60%" }} /></div>
      </div>
    );
  }
  return (
    <div className="summary-bar">
      <div className="sum-cell">
        <span className="sc-k">총 자산</span>
        <span className="sc-v">${agg.totalUsd.toLocaleString("en-US", { maximumFractionDigits: 0 })}</span>
        <span className="sc-sub">{agg.walletCount} 지갑 합산</span>
      </div>
      <div className="sum-cell">
        <span className="sc-k">대기 트랜잭션</span>
        <span className="sc-v">{agg.pending}</span>
        <span className="sc-sub">pending pool</span>
      </div>
      <div className="sum-cell">
        <span className="sc-k">위험 신호</span>
        <div className="risk-chips">
          <span className="risk-chip unl"><span className="rc-dot" />무제한 <b>{agg.unlimited}</b></span>
        </div>
      </div>
    </div>
  );
}

// ── L2 header band (single wallet) ──────────────────────────────────────

function L2HeaderBand({
  wallet,
  findings,
  holdings,
  apIdx,
}: {
  wallet: DashboardWalletSummary;
  findings: VerdictDto[];
  holdings: TokenHolding[];
  apIdx?: ApprovalIndex;
}) {
  const fails = findings.filter((f) => f.verdict === "fail" && f.user_decision === null).length;
  const warns = findings.filter((f) => f.verdict === "warn" && f.user_decision === null).length;
  const totalVar = holdings.reduce((s, h) => s + varOfHolding(h, apIdx), 0);
  const totalUsd = Number(wallet.total_usd ?? "0");
  return (
    <div className="l2-header">
      <div className="l2h-id">
        <div className="l2h-status">
          {fails > 0 && <span className="l2-chip fail"><span className="lc-dot" />FAIL <b>{fails}</b></span>}
          {warns > 0 && <span className="l2-chip warn"><span className="lc-dot" />WARN <b>{warns}</b></span>}
          {fails === 0 && warns === 0 && <span className="l2-chip calm"><span className="lc-dot" />Calm</span>}
          {wallet.pending_count > 0 && <span className="l2-pending">{wallet.pending_count} pending</span>}
        </div>
        <div className="l2h-addr mono">{wallet.address}</div>
      </div>
      <div className="l2h-metrics">
        <div className="l2m">
          <span className="l2m-k">총 자산</span>
          <span className="l2m-v">${totalUsd.toLocaleString("en-US", { maximumFractionDigits: 0 })}</span>
        </div>
        <div className="l2m">
          <span className="l2m-k">총 VaR · 현재 노출</span>
          <span className="l2m-v var">${totalVar.toLocaleString("en-US", { maximumFractionDigits: 0 })}</span>
          <span className="l2m-sub">min(allowance, balance) × price</span>
        </div>
        <div className="l2m">
          <span className="l2m-k">무제한 승인</span>
          <span className="l2m-v unl">{wallet.unlimited_count}</span>
          <span className="l2m-sub">potential exposure</span>
        </div>
      </div>
    </div>
  );
}

// ── Action queue (L2 only) ──────────────────────────────────────────────

interface QueueItem {
  kind: "detection" | "approval";
  id: string;
  severity: "fail" | "warn";
  data:
    | { type: "finding"; v: VerdictDto }
    | { type: "approval"; entry: ApprovalIndexEntry; tokenAddr: string; chain: string };
  varHint?: number;
}

function ActionQueueCard({
  wallet,
  findings,
  loading,
  apIdx,
}: {
  wallet: DashboardWalletSummary;
  findings: VerdictDto[];
  loading: boolean;
  apIdx?: ApprovalIndex;
}) {
  const items: QueueItem[] = useMemo(() => {
    const out: QueueItem[] = [];
    findings
      .filter((f) => (f.verdict === "fail" || f.verdict === "warn") && f.user_decision === null)
      .forEach((f) =>
        out.push({
          kind: "detection",
          id: `f-${f.id}`,
          severity: f.verdict as "fail" | "warn",
          data: { type: "finding", v: f },
        }),
      );
    if (apIdx) {
      for (const entries of apIdx.values()) {
        for (const entry of entries) {
          const isBlocked = entry.risk.has("BLOCKED");
          const isUnlimited = entry.risk.has("UNLIMITED");
          if (!isBlocked && !isUnlimited) continue;
          out.push({
            kind: "approval",
            id: `a-${entry.chain}-${entry.tokenAddr}-${entry.spender}`,
            severity: isBlocked ? "fail" : "warn",
            data: { type: "approval", entry, tokenAddr: entry.tokenAddr, chain: entry.chain },
          });
        }
      }
    }
    return out.sort((a, b) => (a.severity === b.severity ? 0 : a.severity === "fail" ? -1 : 1));
  }, [findings, apIdx]);

  return (
    <section className="card aq-card">
      <div className="aq-head">
        <h3 className="aq-title">긴급 항목 큐</h3>
        <span className="aq-meta">{items.length}건 · 우선순위순</span>
      </div>
      {loading && <div className="empty-cell">불러오는 중…</div>}
      {!loading && items.length === 0 && (
        <div className="aq-empty">
          ✓ 처리할 긴급 항목이 없어요 — 이 지갑은 정상입니다.
        </div>
      )}
      {!loading && items.length > 0 && (
        <div className="aq-list">
          {items.map((it) => <QueueRow key={it.id} item={it} walletLabel={wallet.label ?? shortAddr(wallet.address)} />)}
        </div>
      )}
    </section>
  );
}

function QueueRow({ item, walletLabel: _walletLabel }: { item: QueueItem; walletLabel: string }) {
  const qc = useMutation({
    mutationFn: (decision: "trusted" | "cancelled") =>
      item.data.type === "finding" ? setVerdictDecision(item.data.v.id, decision) : Promise.resolve(),
  });
  if (item.data.type === "finding") {
    const v = item.data.v;
    return (
      <div className={`aq-row ${item.severity}`}>
        <div className="aq-tag">
          <span className="type-tag detection">탐지</span>
          <span className={`v-pill ${item.severity}`}>{item.severity.toUpperCase()}</span>
        </div>
        <div className="aq-main">
          <div className="aq-line"><b>{v.policy?.name ?? "(no policy)"}</b> · {v.decoded_fn ?? v.method ?? "—"}</div>
          <div className="aq-sub mono">{v.dapp_origin ?? "—"} · {v.reason?.ko ?? v.reason?.en ?? "—"}</div>
        </div>
        <div className="aq-actions">
          <button className="btn primary" disabled={qc.isPending} onClick={() => qc.mutate("trusted")}>신뢰</button>
          <button className="btn" disabled={qc.isPending} onClick={() => qc.mutate("cancelled")}>무시</button>
        </div>
      </div>
    );
  }
  // approval
  const entry = item.data.entry;
  return (
    <div className={`aq-row ${item.severity}`}>
      <div className="aq-tag">
        <span className="type-tag approval">승인</span>
        <span className={`v-pill ${item.severity}`}>{item.severity.toUpperCase()}</span>
      </div>
      <div className="aq-main">
        <div className="aq-line">
          <b>{entry.spenderLabel ?? "(unknown spender)"}</b> · {entry.risk.has("UNLIMITED") ? "Unlimited" : "approved"}
        </div>
        <div className="aq-sub mono">
          token {shortAddr(entry.tokenAddr)} · spender {shortAddr(entry.spender)} · {entry.chain}
        </div>
      </div>
      <div className="aq-actions">
        <span style={{ fontSize: 11, color: "var(--slate-400)" }}>아래 Approvals 표에서 철회</span>
      </div>
    </div>
  );
}

// ── Lens toggle + risk suggest banner ───────────────────────────────────

function LensToggle({ lens, setLens }: { lens: "assets" | "risk"; setLens: (l: "assets" | "risk") => void }) {
  return (
    <div className="lens-toggle" role="tablist" aria-label="lens">
      <button
        role="tab"
        aria-selected={lens === "assets"}
        className={`lens-btn${lens === "assets" ? " on" : ""}`}
        onClick={() => setLens("assets")}
      >
        💰 자산 보기
      </button>
      <button
        role="tab"
        aria-selected={lens === "risk"}
        className={`lens-btn${lens === "risk" ? " on risk-on" : ""}`}
        onClick={() => setLens("risk")}
      >
        🛡 위험 보기
      </button>
    </div>
  );
}

function RiskSuggestBanner({
  failCount,
  onSwitch,
  onDismiss,
}: {
  failCount: number;
  onSwitch: () => void;
  onDismiss: () => void;
}) {
  return (
    <div className="risk-suggest">
      <span className="rs-ic">⚠</span>
      <span className="rs-txt">
        <b>BLOCKED {failCount}건</b>이 감지됐어요. 위험 보기로 전환하면 노출된 자산이 상단으로 올라옵니다.
      </span>
      <button className="rs-act" onClick={onSwitch}>위험 보기로 전환</button>
      <button className="rs-dismiss" onClick={onDismiss} aria-label="dismiss">✕</button>
    </div>
  );
}

// ── Chain breakdown ─────────────────────────────────────────────────────

const CHAIN_COLORS: Record<string, string> = {
  "eip155:1": "#627EEA",
  "eip155:42161": "#2D9BF0",
  "eip155:8453": "#0052FF",
  "eip155:10": "#FF0420",
  "eip155:137": "#8247E5",
  "eip155:56": "#F0B90B",
};
const CHAIN_NAMES: Record<string, string> = {
  "eip155:1": "Ethereum",
  "eip155:42161": "Arbitrum",
  "eip155:8453": "Base",
  "eip155:10": "Optimism",
  "eip155:137": "Polygon",
  "eip155:56": "BNB",
};
const VENUE_COLORS: Record<string, string> = {
  hyperliquid: "#0EA5A6",
};
const VENUE_NAMES: Record<string, string> = {
  hyperliquid: "Hyperliquid",
};
function chainColor(chain: string): string {
  return CHAIN_COLORS[chain] ?? "#9099A5";
}
function chainName(chain: string): string {
  return CHAIN_NAMES[chain] ?? chain;
}
function venueColor(venue: string): string {
  return VENUE_COLORS[venue] ?? "#6366F1";
}
function venueName(venue: string): string {
  return VENUE_NAMES[venue] ?? venue;
}

function ChainPill({ chain }: { chain: string | null }) {
  if (!chain) return <span className="mono">—</span>;
  return (
    <span className="chain-pill">
      <span className="cp-dot" style={{ background: chainColor(chain) }} />
      {chainName(chain)}
    </span>
  );
}

function ChainBreakdown({ summary, loading }: { summary?: DashboardSummary; loading: boolean }) {
  if (loading || !summary) {
    return <div className="chain-card"><div className="skeleton-row" style={{ width: "100%" }} /></div>;
  }
  const venueBreakdown = summary.venue_breakdown ?? [];
  const rows = [
    ...summary.chain_breakdown.map((c) => ({
      key: `chain:${c.chain}`,
      label: chainName(c.chain),
      usd: c.usd,
      pct: c.pct,
      color: chainColor(c.chain),
    })),
    ...venueBreakdown.map((v) => ({
      key: `venue:${v.venue}`,
      label: venueName(v.venue),
      usd: v.usd,
      pct: v.pct,
      color: venueColor(v.venue),
    })),
  ];
  if (rows.length === 0) {
    return (
      <div className="chain-card">
        <div className="cc-head">
          <span className="cc-ttl">자산 분포</span>
          <span className="cc-meta">잔고 없음</span>
        </div>
      </div>
    );
  }
  return (
    <div className="chain-card">
      <div className="cc-head">
        <span className="cc-ttl">자산 분포</span>
        <span className="cc-meta">{rows.length} sources</span>
      </div>
      <div className="chain-bar">
        {rows.map((r) => (
          <div
            key={r.key}
            className="chain-seg"
            style={{ width: `${r.pct}%`, background: r.color }}
            title={`${r.label} · ${r.pct.toFixed(2)}%`}
          />
        ))}
      </div>
      <div className="chain-legend">
        {rows.map((r) => (
          <span key={r.key} className="chain-leg">
            <span className="cl-dot" style={{ background: r.color }} />
            <span className="cl-name">{r.label}</span>
            <span className="cl-pct">
              ${Number(r.usd).toLocaleString("en-US", { maximumFractionDigits: 0 })} · {r.pct.toFixed(2)}%
            </span>
          </span>
        ))}
      </div>
    </div>
  );
}

// ── Wallet asset ratio (L1 only) ────────────────────────────────────────

/**
 * Per-wallet share of the workspace portfolio. Pulled straight from
 * `summary.wallets[*].total_usd` (the same number the wallet card shows)
 * so we don't have to fan out a holdings query just for the ratio.
 *
 * Wallets with 0 USD are shown as a single grey "기타" segment to avoid
 * a noisy zero-width row.
 */
const WALLET_COLORS = [
  "#0EA5A6", "#7C9CFF", "#F59E0B", "#EC4899", "#6366F1",
  "#10B981", "#F97316", "#06B6D4", "#A855F7", "#EF4444",
];
function walletColor(i: number): string {
  return WALLET_COLORS[i % WALLET_COLORS.length];
}

function WalletAssetRatio({
  wallets,
  loading,
  onWalletClick,
}: {
  wallets: DashboardWalletSummary[];
  loading: boolean;
  onWalletClick: (addr: string) => void;
}) {
  if (loading) {
    return <div className="chain-card"><div className="skeleton-row" style={{ width: "100%" }} /></div>;
  }
  const total = wallets.reduce((s, w) => s + Number(w.total_usd ?? "0"), 0);
  if (total === 0) {
    return (
      <div className="chain-card">
        <div className="cc-head">
          <span className="cc-ttl">지갑별 자산 비율</span>
          <span className="cc-meta">잔고 없음</span>
        </div>
      </div>
    );
  }
  const rows = wallets
    .map((w, i) => {
      const usd = Number(w.total_usd ?? "0");
      return {
        addr: w.address,
        label: w.label ?? shortAddr(w.address),
        usd,
        pct: (usd / total) * 100,
        color: walletColor(i),
      };
    })
    .sort((a, b) => b.usd - a.usd);
  const shown = rows.filter((r) => r.usd > 0);

  return (
    <div className="chain-card">
      <div className="cc-head">
        <span className="cc-ttl">지갑별 자산 비율</span>
        <span className="cc-meta">{wallets.length} wallets</span>
      </div>
      <div className="chain-bar">
        {shown.map((r) => (
          <div
            key={r.addr}
            className="chain-seg"
            style={{ width: `${r.pct}%`, background: r.color }}
            title={`${r.label} · ${r.pct.toFixed(2)}%`}
          />
        ))}
      </div>
      <div className="chain-legend">
        {rows.map((r) => (
          <button
            key={r.addr}
            type="button"
            className="chain-leg wallet-leg"
            onClick={() => onWalletClick(r.addr)}
            title={`${r.label} 드릴다운`}
          >
            <span className="cl-dot" style={{ background: r.color }} />
            <span className="cl-name">{r.label}</span>
            <span className="cl-pct">
              ${r.usd.toLocaleString("en-US", { maximumFractionDigits: 0 })} · {r.pct.toFixed(2)}%
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}

// ── Approval index — risk overlay + VaR ─────────────────────────────────

type RiskTag = "UNLIMITED" | "KNOWN_VENUE" | "BLOCKED" | "OLD" | "EXPIRED";

interface ApprovalIndexEntry {
  chain: string;
  tokenAddr: string;
  spender: string;
  spenderLabel?: string;
  allowance: number; // Number, may be Infinity for unlimited
  risk: Set<RiskTag>;
}

/** Per-wallet, then `(chain|tokenAddr)` → list of approvals targeting it. */
type ApprovalIndex = Map<string, ApprovalIndexEntry[]>;

function buildApprovalIndexes(
  wallets: DashboardWalletSummary[],
  approvals: Array<ClassifiedApprovals | undefined>,
): Map<string, ApprovalIndex> {
  const out = new Map<string, ApprovalIndex>();
  wallets.forEach((w, i) => {
    const map: ApprovalIndex = new Map();
    const data = approvals[i];
    if (!data) {
      out.set(w.address, map);
      return;
    }
    const add = (e: ApprovalIndexEntry) => {
      const k = `${e.chain}|${e.tokenAddr}`;
      const arr = map.get(k) ?? [];
      arr.push(e);
      map.set(k, arr);
    };
    data.erc20.forEach((a) => {
      add({
        chain: a.chain,
        tokenAddr: a.token.toLowerCase(),
        spender: a.spender.toLowerCase(),
        spenderLabel: undefined,
        allowance: a.is_unlimited ? Infinity : Number(a.amount) || 0,
        risk: new Set(a.risk as RiskTag[]),
      });
    });
    data.permit2.forEach((a) => {
      add({
        chain: a.chain,
        tokenAddr: a.token.toLowerCase(),
        spender: a.spender.toLowerCase(),
        spenderLabel: undefined,
        allowance: Number(a.amount) || 0,
        risk: new Set(a.risk as RiskTag[]),
      });
    });
    data.set_for_all.forEach((a) => {
      add({
        chain: a.chain,
        tokenAddr: a.collection.toLowerCase(),
        spender: a.operator.toLowerCase(),
        spenderLabel: undefined,
        allowance: Infinity,
        risk: new Set(a.risk as RiskTag[]),
      });
    });
    out.set(w.address, map);
  });
  return out;
}

function riskTagsFor(h: TokenHolding, idx?: ApprovalIndex): RiskTag[] {
  if (!idx) return [];
  const chain = chainOf(h);
  const addr = addressOf(h);
  if (!chain || !addr) return [];
  const entries = idx.get(`${chain}|${addr}`);
  if (!entries) return [];
  const tags = new Set<RiskTag>();
  entries.forEach((e) => e.risk.forEach((t) => tags.add(t)));
  return [...tags];
}

/** Raw on-chain integer amount (`balance.amount`, approval allowances) → human
 *  token units. The server stores amounts in base units (wei-like); the UI must
 *  divide by 10^decimals before display or USD/VaR math. */
function toHuman(rawUnits: number, decimals: number): number {
  return decimals > 0 ? rawUnits / 10 ** decimals : rawUnits;
}

function varOfHolding(h: TokenHolding, idx?: ApprovalIndex): number {
  if (!idx) return 0;
  const chain = chainOf(h);
  const addr = addressOf(h);
  if (!chain || !addr) return 0;
  const entries = idx.get(`${chain}|${addr}`);
  if (!entries) return 0;
  const balance = Number(h.balance.amount ?? "0");
  if (!isFinite(balance) || balance === 0) return 0;
  const price = h.price_usd ? Number(h.price_usd.value) : 0;
  if (price === 0) return 0;
  // VaR = sum over distinct spenders of min(allowance, balance) × price.
  // Sum is bounded by balance × price (an attacker can't move more than
  // the wallet holds, even across many spenders). `balance` and the approval
  // `allowance` are both raw base units (same decimals), so the min/cap is done
  // in base units and converted to human units once before applying the price.
  const exposureUnits = entries.reduce((s, e) => s + Math.min(e.allowance, balance), 0);
  const cappedUnits = Math.min(exposureUnits, balance);
  return toHuman(cappedUnits, h.decimals) * price;
}

// ── Holdings table ──────────────────────────────────────────────────────

interface HoldingRow {
  walletAddr: string;
  walletLabel: string | null;
  h: TokenHolding;
  usd: number;
}

interface AggregatedRow {
  groupKey: string;
  symbol: string;
  chain: string | null;
  standard: string;
  decimals: number;
  balanceSum: number;
  usdSum: number;
  varSum: number;
  priceUsd: number | null;
  lastSyncedAt: number;
  riskTags: RiskTag[];
  wallets: Array<{ addr: string; label: string | null; balance: number; usd: number }>;
}

function groupKeyOf(h: TokenHolding): string {
  const k = (h.key ?? {}) as Record<string, unknown>;
  const standard = typeof k.standard === "string" ? k.standard : "unknown";
  const chain = typeof k.chain === "string" ? k.chain : "";
  const address = addressOf(h) ?? "";
  return `${standard}|${chain}|${address || h.symbol || ""}`;
}
function chainOf(h: TokenHolding): string | null {
  const k = (h.key ?? {}) as Record<string, unknown>;
  return typeof k.chain === "string" ? k.chain : null;
}
function addressOf(h: TokenHolding): string | null {
  const k = (h.key ?? {}) as Record<string, unknown>;
  const raw =
    typeof k.address === "string"
      ? (k.address as string)
      : typeof k.contract === "string"
        ? (k.contract as string)
        : null;
  return raw ? raw.toLowerCase() : null;
}
function standardOf(h: TokenHolding): string {
  const k = (h.key ?? {}) as Record<string, unknown>;
  return typeof k.standard === "string" ? k.standard : "unknown";
}

function kindOfStandard(std: string): "native" | "erc20" | "nft" | "other" {
  if (std === "native") return "native";
  if (std === "erc20") return "erc20";
  if (std === "erc721" || std === "erc1155") return "nft";
  return "other";
}

function riskScore(tags: RiskTag[]): number {
  if (tags.includes("BLOCKED")) return 0;
  if (tags.includes("UNLIMITED")) return 1;
  if (tags.includes("OLD") || tags.includes("EXPIRED")) return 2;
  return 3;
}

function HoldingsTable({
  wallets,
  queries,
  indexes,
  lens,
  onWalletClick,
}: {
  wallets: DashboardWalletSummary[];
  queries: Array<ReturnType<typeof useQuery<TokenHolding[]>>>;
  indexes: Map<string, ApprovalIndex>;
  lens: "assets" | "risk";
  onWalletClick: (addr: string) => void;
}) {
  const anyLoading = queries.some((q) => q.isLoading);
  const rows: HoldingRow[] = wallets.flatMap((w, i) => {
    const data = queries[i]?.data ?? [];
    return data.map((h) => ({
      walletAddr: w.address,
      walletLabel: w.label,
      h,
      usd: Number(h.value_usd ?? "0"),
    }));
  });

  const aggregated: AggregatedRow[] = useMemo(() => {
    const map = new Map<string, AggregatedRow>();
    for (const r of rows) {
      const key = groupKeyOf(r.h);
      const balance = toHuman(Number(r.h.balance.amount ?? "0"), r.h.decimals);
      const idx = indexes.get(r.walletAddr);
      const tags = riskTagsFor(r.h, idx);
      const v = varOfHolding(r.h, idx);
      const exist = map.get(key);
      if (exist) {
        exist.balanceSum += isFinite(balance) ? balance : 0;
        exist.usdSum += r.usd;
        exist.varSum += v;
        exist.wallets.push({ addr: r.walletAddr, label: r.walletLabel, balance, usd: r.usd });
        tags.forEach((t) => exist.riskTags.includes(t) || exist.riskTags.push(t));
        if (r.h.last_synced_at > exist.lastSyncedAt) exist.lastSyncedAt = r.h.last_synced_at;
      } else {
        map.set(key, {
          groupKey: key,
          symbol: r.h.symbol || "?",
          chain: chainOf(r.h),
          standard: standardOf(r.h),
          decimals: r.h.decimals,
          balanceSum: isFinite(balance) ? balance : 0,
          usdSum: r.usd,
          varSum: v,
          priceUsd: r.h.price_usd ? Number(r.h.price_usd.value) : null,
          lastSyncedAt: r.h.last_synced_at,
          riskTags: [...tags],
          wallets: [{ addr: r.walletAddr, label: r.walletLabel, balance, usd: r.usd }],
        });
      }
    }
    const out = [...map.values()];
    if (lens === "risk") {
      return out.sort((a, b) => {
        const da = riskScore(a.riskTags) - riskScore(b.riskTags);
        if (da !== 0) return da;
        return b.varSum + b.usdSum - (a.varSum + a.usdSum);
      });
    }
    return out.sort((a, b) => b.usdSum - a.usdSum);
  }, [rows, indexes, lens]);

  const isAggregated = wallets.length > 1;

  const aggregatedHasRisk = aggregated.some((a) => a.riskTags.length > 0);
  const singleHasRisk = rows.some((r) => riskTagsFor(r.h, indexes.get(r.walletAddr)).length > 0);
  const noRiskFound = (isAggregated ? !aggregatedHasRisk : !singleHasRisk) && lens === "risk";

  return (
    <div className={`tbl-wrap lens-${lens}`}>
      {noRiskFound && (
        <div className="lens-empty-note">
          ✓ 이 view에서 위험 자산이 감지되지 않음. <b>자산 보기</b>와 정렬 순서가 동일합니다.
        </div>
      )}
      <table>
        <thead>
          <tr>
            <th>자산</th>
            <th>체인</th>
            <th>지갑</th>
            <th className="num">잔고</th>
            <th className={`num${lens === "assets" ? " col-emph" : ""}`}>USD</th>
            <th className={lens === "risk" ? "col-emph" : ""}>위험 오버레이</th>
            <th className={`num${lens === "risk" ? " col-emph" : ""}`}>VaR</th>
          </tr>
        </thead>
        <tbody>
          {anyLoading && (
            <tr><td colSpan={7} className="empty-cell">불러오는 중…</td></tr>
          )}
          {!anyLoading && aggregated.length === 0 && (
            <tr><td colSpan={7} className="empty-cell">표시할 holding이 없습니다</td></tr>
          )}
          {!anyLoading && isAggregated && aggregated.map((a) => {
            const rc = a.riskTags.includes("BLOCKED")
              ? "fail"
              : a.riskTags.includes("UNLIMITED")
                ? "warn"
                : null;
            const kind = kindOfStandard(a.standard);
            const dim = lens === "risk" && !rc;
            return (
              <tr
                key={a.groupKey}
                className={`${rc ? `risk-${rc}` : ""}${dim ? " row-dim" : ""}`.trim()}
                title={a.wallets.map((w) => `${w.label ?? shortAddr(w.addr)}: ${w.balance.toLocaleString("en-US", { maximumFractionDigits: 6 })} (≈$${w.usd.toLocaleString("en-US", { maximumFractionDigits: 0 })})`).join("\n")}
              >
                <td>
                  <div className="asset-cell">
                    <span className={`asset-ic ${kind}`}>{(a.symbol || "?").slice(0, 3).toUpperCase()}</span>
                    <span className="asset-txt">
                      <span className="asset-sym">
                        {a.symbol}
                        {kind === "nft" && <span className="kind-tag nft">NFT</span>}
                        {kind === "native" && <span className="kind-tag native">native</span>}
                      </span>
                    </span>
                  </div>
                </td>
                <td><ChainPill chain={a.chain} /></td>
                <td>
                  <WalletChips wallets={a.wallets} onClick={onWalletClick} />
                </td>
                <td className="num">{a.balanceSum.toLocaleString("en-US", { maximumFractionDigits: 6 })}</td>
                <td className="num strong">
                  {a.usdSum > 0 ? `$${a.usdSum.toLocaleString("en-US", { maximumFractionDigits: 2 })}` : "—"}
                </td>
                <td>
                  {a.riskTags.length === 0 ? (
                    <span className="r-safe">노출 없음</span>
                  ) : (
                    a.riskTags.map((t) => <span key={t} className={`risk-tag ${t}`}>{t}</span>)
                  )}
                </td>
                <td className="num strong">
                  {a.varSum > 0 ? (
                    <span style={{ color: a.varSum > 1000 ? "var(--warn-700)" : "var(--slate-700)" }}>
                      ${a.varSum.toLocaleString("en-US", { maximumFractionDigits: 0 })}
                    </span>
                  ) : (
                    <span style={{ color: "var(--slate-400)" }}>$0</span>
                  )}
                </td>
              </tr>
            );
          })}
          {!anyLoading && !isAggregated && rows
            .map((r) => ({ r, tags: riskTagsFor(r.h, indexes.get(r.walletAddr)), varUsd: varOfHolding(r.h, indexes.get(r.walletAddr)) }))
            .sort((x, y) => {
              if (lens === "risk") {
                const d = riskScore(x.tags) - riskScore(y.tags);
                if (d !== 0) return d;
              }
              return y.r.usd + y.varUsd - (x.r.usd + x.varUsd);
            })
            .map(({ r, tags, varUsd }, idx) => {
              const rc = tags.includes("BLOCKED") ? "fail" : tags.includes("UNLIMITED") ? "warn" : null;
              const kind = kindOfStandard(standardOf(r.h));
              const dim = lens === "risk" && !rc;
              return (
                <tr key={`${r.walletAddr}-${idx}`} className={`${rc ? `risk-${rc}` : ""}${dim ? " row-dim" : ""}`.trim()}>
                  <td>
                    <div className="asset-cell">
                      <span className={`asset-ic ${kind}`}>{(r.h.symbol || "?").slice(0, 3).toUpperCase()}</span>
                      <span className="asset-txt">
                        <span className="asset-sym">
                          {r.h.symbol || "?"}
                          {kind === "nft" && <span className="kind-tag nft">NFT</span>}
                          {kind === "native" && <span className="kind-tag native">native</span>}
                        </span>
                      </span>
                    </div>
                  </td>
                  <td><ChainPill chain={chainOf(r.h)} /></td>
                  <td className="mono">{r.walletLabel ?? shortAddr(r.walletAddr)}</td>
                  <td className="num">{fmtBalance(r.h)}</td>
                  <td className="num strong">
                    {r.usd > 0 ? `$${r.usd.toLocaleString("en-US", { maximumFractionDigits: 2 })}` : "—"}
                  </td>
                  <td>
                    {tags.length === 0 ? (
                      <span className="r-safe">노출 없음</span>
                    ) : (
                      tags.map((t) => <span key={t} className={`risk-tag ${t}`}>{t}</span>)
                    )}
                  </td>
                  <td className="num strong">
                    {varUsd > 0 ? (
                      <span style={{ color: varUsd > 1000 ? "var(--warn-700)" : "var(--slate-700)" }}>
                        ${varUsd.toLocaleString("en-US", { maximumFractionDigits: 0 })}
                      </span>
                    ) : (
                      <span style={{ color: "var(--slate-400)" }}>$0</span>
                    )}
                  </td>
                </tr>
              );
            })}
        </tbody>
      </table>
    </div>
  );
}

/** Wallet chip row with collapse beyond N. Sorted by USD descending so
 *  the most material holders surface first; the rest fold under "+N". */
const WALLET_CHIPS_VISIBLE = 4;
function WalletChips({
  wallets,
  onClick,
}: {
  wallets: Array<{ addr: string; label: string | null; balance: number; usd: number }>;
  onClick: (addr: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const sorted = useMemo(() => [...wallets].sort((a, b) => b.usd - a.usd), [wallets]);
  const visible = expanded ? sorted : sorted.slice(0, WALLET_CHIPS_VISIBLE);
  const hidden = sorted.length - visible.length;
  return (
    <div className="wallet-chips">
      {visible.map((w) => (
        <button
          key={w.addr}
          className="wallet-jump"
          onClick={() => onClick(w.addr)}
          title={`${w.balance.toLocaleString("en-US", { maximumFractionDigits: 6 })} · $${w.usd.toLocaleString("en-US", { maximumFractionDigits: 0 })}`}
        >
          {w.label ?? shortAddr(w.addr)}
        </button>
      ))}
      {!expanded && hidden > 0 && (
        <button
          className="wallet-jump more"
          onClick={(e) => {
            e.stopPropagation();
            setExpanded(true);
          }}
          title={`나머지 ${hidden}개 펼치기`}
        >
          +{hidden}
        </button>
      )}
      {expanded && wallets.length > WALLET_CHIPS_VISIBLE && (
        <button
          className="wallet-jump more"
          onClick={(e) => {
            e.stopPropagation();
            setExpanded(false);
          }}
          title="접기"
        >
          접기
        </button>
      )}
    </div>
  );
}

function fmtBalance(h: TokenHolding): string {
  const amt = h.balance.amount;
  if (!amt) return "—";
  const raw = Number(amt);
  if (!isFinite(raw)) return amt;
  return toHuman(raw, h.decimals).toLocaleString("en-US", { maximumFractionDigits: 6 });
}

// ── Approvals table ─────────────────────────────────────────────────────

interface ApprovalRow {
  walletAddr: string;
  walletLabel: string | null;
  kind: "erc20" | "set_for_all" | "permit2";
  data: ClassifiedErc20Approval | ClassifiedSetForAllApproval | ClassifiedPermit2Approval;
}

function ApprovalsTable({
  wallets,
  queries,
}: {
  wallets: DashboardWalletSummary[];
  queries: Array<ReturnType<typeof useQuery<ClassifiedApprovals>>>;
}) {
  const [revokeItem, setRevokeItem] = useState<RevokeItem | null>(null);

  const anyLoading = queries.some((q) => q.isLoading);
  const rows: ApprovalRow[] = wallets.flatMap((w, i) => {
    const data = queries[i]?.data;
    if (!data) return [];
    return [
      ...data.erc20.map((a) => ({ walletAddr: w.address, walletLabel: w.label, kind: "erc20" as const, data: a })),
      ...data.set_for_all.map((a) => ({ walletAddr: w.address, walletLabel: w.label, kind: "set_for_all" as const, data: a })),
      ...data.permit2.map((a) => ({ walletAddr: w.address, walletLabel: w.label, kind: "permit2" as const, data: a })),
    ];
  });

  const sevOf = (tags: string[]) =>
    tags.includes("BLOCKED") ? 0 : tags.includes("UNLIMITED") ? 1 : tags.includes("OLD") ? 2 : 3;
  rows.sort((a, b) => sevOf(a.data.risk) - sevOf(b.data.risk));

  return (
    <>
      <div className="tbl-wrap">
        <table>
          <thead>
            <tr>
              <th>유형</th>
              <th>토큰 / 컬렉션</th>
              <th>지갑</th>
              <th>spender / operator</th>
              <th>금액</th>
              <th>위험</th>
              <th style={{ width: 80 }}>액션</th>
            </tr>
          </thead>
          <tbody>
            {anyLoading && (
              <tr><td colSpan={7} className="empty-cell">불러오는 중…</td></tr>
            )}
            {!anyLoading && rows.length === 0 && (
              <tr><td colSpan={7} className="empty-cell">표시할 approval이 없습니다</td></tr>
            )}
            {!anyLoading && rows.map((r, idx) => {
              const isErc20 = r.kind === "erc20";
              const tokenOrColl = "token" in r.data ? r.data.token : (r.data as ClassifiedSetForAllApproval).collection;
              const spenderOrOp = "spender" in r.data ? r.data.spender : (r.data as ClassifiedSetForAllApproval).operator;
              return (
                <tr key={`${r.walletAddr}-${r.kind}-${idx}`}>
                  <td className="strong" style={{ textTransform: "uppercase", fontSize: 11 }}>{r.kind}</td>
                  <td className="mono">{shortAddr(tokenOrColl)}</td>
                  <td className="mono">{r.walletLabel ?? shortAddr(r.walletAddr)}</td>
                  <td>
                    <span className="mono">{shortAddr(spenderOrOp)}</span>
                  </td>
                  <td className="mono num">
                    {"amount" in r.data
                      ? (r.data as ClassifiedErc20Approval).is_unlimited
                        ? "Unlimited"
                        : r.data.amount === "0"
                          ? "0"
                          : fmtApprovalAmount(r.data.amount)
                      : "—"}
                  </td>
                  <td>
                    {r.data.risk.map((tag) => (
                      <span key={tag} className={`risk-tag ${tag}`}>{tag}</span>
                    ))}
                  </td>
                  <td>
                    {isErc20 ? (
                      <button
                        className="btn danger"
                        onClick={() =>
                          setRevokeItem({
                            chain: r.data.chain,
                            token: tokenOrColl,
                            spender: spenderOrOp,
                            label: shortAddr(spenderOrOp),
                          })
                        }
                      >
                        철회
                      </button>
                    ) : (
                      <span style={{ fontSize: 11, color: "var(--slate-400)" }}>—</span>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {revokeItem && <RevokeModal item={revokeItem} onClose={() => setRevokeItem(null)} />}
    </>
  );
}

// ── Hyperliquid ─────────────────────────────────────────────────────────

function HyperliquidSection({
  wallets,
  queries,
}: {
  wallets: DashboardWalletSummary[];
  queries: Array<ReturnType<typeof useQuery<Position[]>>>;
}) {
  const anyLoading = queries.some((q) => q.isLoading);
  const accounts = wallets
    .map((w, i) => ({ w, acct: hlAccountOf(queries[i]?.data ?? []) }))
    .filter((x): x is { w: DashboardWalletSummary; acct: HlAccount } => x.acct !== null);

  if (accounts.length === 0) {
    return (
      <div className="tbl-wrap">
        <div className="empty-cell" style={{ padding: 12 }}>
          {anyLoading ? "불러오는 중…" : "Hyperliquid 계정이 없습니다"}
        </div>
      </div>
    );
  }
  return (
    <div>
      {accounts.map(({ w, acct }) => (
        <HlAccountCard key={w.address} wallet={w} acct={acct} />
      ))}
    </div>
  );
}

/** Split a HL symbol like `xyz:MU` into a perp-dex tag + coin. */
function splitSym(sym: string | undefined, assetIndex: number): { coin: string; dex?: string } {
  if (!sym) return { coin: `#${assetIndex}` };
  const i = sym.indexOf(":");
  return i > 0 ? { dex: sym.slice(0, i), coin: sym.slice(i + 1) } : { coin: sym };
}

/** Classify a trigger order as 익절(tp) / 손절(sl). Prefers HL's authoritative
 *  `order_type` ("Take Profit …" / "Stop …"); falls back to comparing the
 *  trigger price against the open position's entry. */
function tpSlOf(
  o: HlAccount["open_orders"][number],
  pos: HlAccount["positions"][number] | undefined,
): "tp" | "sl" | null {
  const t = (o.order_type ?? "").toLowerCase();
  if (t.includes("take profit")) return "tp";
  if (t.includes("stop")) return "sl";
  if (!o.is_trigger || !o.reduce_only || !o.trigger_price || !pos) return null;
  const trig = Number(o.trigger_price);
  const entry = Number(pos.entry_price);
  if (!isFinite(trig) || !isFinite(entry)) return null;
  return pos.is_long ? (trig >= entry ? "tp" : "sl") : trig <= entry ? "tp" : "sl";
}

function hlStableSpotUsd(acct: HlAccount): number {
  return (acct.spot_balances ?? []).reduce((sum, balance) => {
    const coin = balance.coin.toUpperCase();
    if (!["USDC", "USDT", "USDT0", "USDE", "USDH", "USDXL"].includes(coin)) return sum;
    return sum + Number(balance.total ?? "0");
  }, 0);
}

function hlVaultUsd(acct: HlAccount): number {
  return (acct.vault_equities ?? []).reduce((sum, vault) => sum + Number(vault.equity ?? "0"), 0);
}

function HlAccountCard({ wallet, acct }: { wallet: DashboardWalletSummary; acct: HlAccount }) {
  const levByAsset = new Map(acct.leverage_settings.map((s) => [s.asset_index, s]));
  const posByAsset = new Map(acct.positions.map((p) => [p.asset_index, p]));
  const label = wallet.label ?? shortAddr(wallet.address);
  const empty = acct.positions.length === 0 && acct.open_orders.length === 0;
  const perpValue = acct.perp_account_value_usd ?? acct.perp_usdc;
  const spotUsd = hlStableSpotUsd(acct);
  const vaultUsd = hlVaultUsd(acct);

  return (
    <div className="hl-card">
      <div className="hl-card-head">
        <span className="hl-wallet">{label}</span>
        <div className="hl-meta">
          <span className="hl-chip muted">Perp {perpValue ? fmtUsd(perpValue, 2) : "$0"}</span>
          {spotUsd > 0 && <span className="hl-chip muted">Spot {fmtUsd(String(spotUsd), 2)}</span>}
          {vaultUsd > 0 && <span className="hl-chip muted">Vault {fmtUsd(String(vaultUsd), 2)}</span>}
          {Number(acct.pending_outflow) > 0 && (
            <span className="hl-chip danger">출금대기 {fmtUsd(acct.pending_outflow, 2)}</span>
          )}
          {acct.agents.length > 0 && <span className="hl-chip danger">agent {acct.agents.length}</span>}
        </div>
      </div>

      {acct.positions.map((p, i) => {
        const { coin, dex } = splitSym(p.symbol, p.asset_index);
        const lev = levByAsset.get(p.asset_index);
        const notional = Number(p.size) * Number(p.entry_price);
        return (
          <div className="hl-pos" key={`p${i}`}>
            <div className="hl-pos-main">
              <span className="hl-sym">{coin}</span>
              {dex && <span className="hl-dex">{dex}</span>}
              <span className={`hl-side ${p.is_long ? "long" : "short"}`}>{p.is_long ? "롱" : "숏"}</span>
              {lev && <span className="hl-chip">{lev.leverage}x {lev.is_cross ? "교차" : "격리"}</span>}
            </div>
            <div className="hl-stats">
              <span className="hl-stat"><span className="k">수량</span><span className="v">{fmtDec(p.size)}</span></span>
              <span className="hl-stat"><span className="k">진입가</span><span className="v">{fmtDec(p.entry_price)}</span></span>
              {isFinite(notional) && (
                <span className="hl-stat"><span className="k">평가</span><span className="v">{fmtUsd(notional, 0)}</span></span>
              )}
            </div>
          </div>
        );
      })}

      {acct.open_orders.length > 0 && (
        <>
          <div className="hl-group-label">오픈 오더 {acct.open_orders.length}</div>
          {acct.open_orders.map((o, i) => {
            const { coin } = splitSym(o.symbol, o.asset_index);
            const tag = tpSlOf(o, posByAsset.get(o.asset_index));
            return (
              <div className="hl-ord" key={`o${i}`}>
                <span className={`hl-side ${o.is_buy ? "long" : "short"}`}>{o.is_buy ? "매수" : "매도"}</span>
                <span className="hl-sym sm">{coin}</span>
                {o.is_trigger && o.trigger_price ? (
                  <span className="hl-trigger">
                    트리거 {fmtDec(o.trigger_price)} <span className="arrow">→</span> 지정 {fmtDec(o.price)}
                  </span>
                ) : (
                  <span className="hl-trigger">지정가 {fmtDec(o.price)}</span>
                )}
                {tag === "tp" && <span className="hl-tp">익절</span>}
                {tag === "sl" && <span className="hl-sl">손절</span>}
                <span className="hl-ord-meta">
                  수량 {o.is_position_tpsl || Number(o.size) === 0 ? "전량" : fmtDec(o.size)} ·{" "}
                  {o.tif.toUpperCase()}
                  {o.reduce_only ? " · 청산전용" : ""}
                </span>
              </div>
            );
          })}
        </>
      )}

      {empty && <div className="hl-empty">열린 포지션·오더 없음</div>}
    </div>
  );
}

// ── Pending (off-chain intent orders / signed permits) ───────────────────

const PENDING_LABELS: Record<PendingKind["kind"], string> = {
  offchain_limit_order: "오프체인 주문",
  perp_venue_order: "perp 주문",
  signed_permit2: "Permit2",
  signed_permit2_transfer: "Permit2 전송",
  signed_e_i_p2612: "EIP-2612",
};

function pendingSummary(k: PendingKind): string {
  switch (k.kind) {
    case "offchain_limit_order":
      return `최대매도 ${k.sell_max} → 최소매수 ${k.buy_min}`;
    case "perp_venue_order":
      return `${k.side} ${k.size_base} @ ${k.price}${k.reduce_only ? " (reduce)" : ""}`;
    case "signed_permit2":
    case "signed_permit2_transfer":
    case "signed_e_i_p2612":
      return `한도 ${k.amount} · spender ${shortAddr(k.spender)}`;
  }
}

function PendingTable({
  wallets,
  queries,
}: {
  wallets: DashboardWalletSummary[];
  queries: Array<ReturnType<typeof useQuery<PendingTx[]>>>;
}) {
  const anyLoading = queries.some((q) => q.isLoading);
  const rows = wallets.flatMap((w, i) => (queries[i]?.data ?? []).map((p) => ({ w, p })));
  return (
    <div className="tbl-wrap">
      <table>
        <thead>
          <tr><th>유형</th><th>지갑</th><th>요약</th><th>서명 시각</th></tr>
        </thead>
        <tbody>
          {anyLoading && rows.length === 0 && (
            <tr><td colSpan={4} className="empty-cell">불러오는 중…</td></tr>
          )}
          {!anyLoading && rows.length === 0 && (
            <tr><td colSpan={4} className="empty-cell">대기 중인 주문이 없습니다</td></tr>
          )}
          {rows.map(({ w, p }, idx) => (
            <tr key={idx}>
              <td className="strong" style={{ fontSize: 11 }}>{PENDING_LABELS[p.kind.kind] ?? p.kind.kind}</td>
              <td className="mono">{w.label ?? shortAddr(w.address)}</td>
              <td className="meta">{pendingSummary(p.kind)}</td>
              <td className="mono num">{p.signed_at ? new Date(p.signed_at * 1000).toLocaleString() : "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function fmtDec(d: string): string {
  const n = Number(d);
  if (!isFinite(n)) return d;
  return n.toLocaleString("en-US", { maximumFractionDigits: 6 });
}

/** USD formatter accepting a Decimal string or a number. */
function fmtUsd(d: string | number, frac: number): string {
  const n = typeof d === "number" ? d : Number(d);
  if (!isFinite(n)) return typeof d === "string" ? d : "—";
  return `$${n.toLocaleString("en-US", { maximumFractionDigits: frac })}`;
}

// ── Revoke modal ────────────────────────────────────────────────────────

function RevokeModal({ item, onClose }: { item: RevokeItem; onClose: () => void }) {
  const planMut = useMutation({
    mutationFn: async () => planRevokesLocal([item]),
  });
  if (planMut.isIdle) planMut.mutate();

  return (
    <Modal
      open
      onClose={onClose}
      title="ERC-20 approve 철회"
      width={560}
      footer={
        <>
          <button className="btn" onClick={onClose}>닫기</button>
          {planMut.data && (
            <button
              className="btn primary"
              onClick={() => {
                navigator.clipboard.writeText(JSON.stringify(planMut.data, null, 2));
              }}
            >
              JSON 복사
            </button>
          )}
        </>
      }
    >
      <p style={{ marginTop: 0, fontSize: 13, color: "var(--slate-600)" }}>
        아래는 <code>approve(spender, 0)</code> 호출에 필요한 calldata입니다.
        지갑(MetaMask 등)에서 직접 트랜잭션으로 보내야 실제로 철회됩니다.
        Pasu 백엔드는 절대 자동 전송하지 않습니다.
      </p>
      <div className="form-row">
        <label>대상</label>
        <div className="mono" style={{ fontSize: 12, color: "var(--slate-700)", padding: "8px 10px", background: "var(--fog-200)", borderRadius: "var(--r-sm)" }}>
          token: {item.token}
          <br />spender: {item.spender}
          <br />chain: {item.chain}
        </div>
      </div>
      {planMut.isPending && <div>calldata 빌드 중…</div>}
      {planMut.error && <div className="err">실패: {String(planMut.error)}</div>}
      {planMut.data && <CallPreview resp={planMut.data} />}
    </Modal>
  );
}

function CallPreview({ resp }: { resp: RevokePlanResp }) {
  const call = resp.calls[0];
  if (!call) return null;
  return (
    <div className="form-row">
      <label>calldata</label>
      <textarea
        readOnly
        rows={6}
        style={{
          width: "100%",
          fontFamily: "var(--ff-mono)",
          fontSize: 11,
          background: "var(--fog-100)",
          border: "1px solid var(--hairline)",
          borderRadius: "var(--r-sm)",
          padding: 10,
          color: "var(--slate-900)",
          wordBreak: "break-all",
        }}
        value={call.data}
      />
      <div className="hint">
        to: <code>{call.to}</code> · value: <code>{call.value}</code> · selector: <code>{call.selector}</code>
      </div>
    </div>
  );
}

function fmtApprovalAmount(raw: string): string {
  const n = Number(raw);
  if (!isFinite(n)) return raw.length > 12 ? `${raw.slice(0, 6)}…(${raw.length}d)` : raw;
  if (n === 0) return "0";
  if (n > 1e18) return n.toExponential(2);
  return n.toLocaleString("en-US", { maximumFractionDigits: 0 });
}

// ── helpers ─────────────────────────────────────────────────────────────

function shortAddr(addr: string): string {
  if (!addr || addr.length < 12) return addr;
  return `${addr.slice(0, 6)}···${addr.slice(-4)}`;
}
