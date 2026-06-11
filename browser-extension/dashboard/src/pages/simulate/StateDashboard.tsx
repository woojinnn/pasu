/**
 * StateDashboard — the rich, visual rendering of one wallet's state, styled as a
 * card dashboard: a summary tile row up top, then token / position / approval
 * sections whose rows EXPAND on click to reveal detail.
 *
 * Used by step 1 (full, static, expandable) and step 2 (with a `filter`, where
 * toggling policies makes unreferenced items COLLAPSE OUT (빠짐) and referenced
 * ones stay/highlight (추가)). In filter mode items stay mounted; only the
 * `sd-gone` / `sd-keep` class flips, so CSS transitions animate both directions
 * without an animation library.
 */
import { useState } from "react";
import type { ReactNode } from "react";

import type { ApprovalView, PositionView, TokenHolding, WalletStateView } from "./types";

const ALLOC_COLORS = ["#0ea5e9", "#22c55e", "#a855f7", "#f59e0b", "#ec4899", "#64748b"];

/** Relevance predicates that drive the collapse/highlight in step 2. */
export interface StateFilter {
  active: boolean;
  isTokenRelevant: (symbol: string) => boolean;
  isProtocolRelevant: (protocol: string) => boolean;
}

function shortAddr(a: string): string {
  return a.length > 12 ? `${a.slice(0, 6)}…${a.slice(-4)}` : a;
}

/** "" (neutral) | "keep" (relevant, highlight) | "gone" (filtered out, collapse). */
type RelState = "" | "keep" | "gone";
function relClass(s: RelState): string {
  return s === "keep" ? " sd-keep" : s === "gone" ? " sd-gone" : "";
}

export function StateDashboard({
  s,
  filter,
  entrance = true,
}: {
  s: WalletStateView;
  filter?: StateFilter;
  /** Play the one-time mount entrance. Step 1 wants it; step 2 sets it false so
   *  the panel only moves in response to policy toggles. */
  entrance?: boolean;
}) {
  // Which rows are expanded (by item id). Independent per dashboard instance.
  const [open, setOpen] = useState<Set<string>>(() => new Set());
  const toggle = (id: string) =>
    setOpen((prev) => {
      const n = new Set(prev);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const total = s.tokens.reduce((acc, t) => acc + (t.usdNum ?? 0), 0);
  const perps = s.positions.filter((p) => p.kind === "perp");
  const others = s.positions.filter((p) => p.kind !== "perp");
  const active = filter?.active ?? false;
  const highRiskCount = s.approvals.filter((a) => (a.risk ?? (a.unlimited ? "high" : "low")) === "high").length;

  const tokenRel = (sym: string): RelState =>
    active ? (filter!.isTokenRelevant(sym) ? "keep" : "gone") : "";
  const protoRel = (proto: string): RelState =>
    active ? (filter!.isProtocolRelevant(proto) ? "keep" : "gone") : "";

  const shown = <T,>(items: T[], rel: (it: T) => RelState) =>
    active ? items.filter((it) => rel(it) !== "gone").length : items.length;

  return (
    <div className={`sd-card${active ? " sd-filterable" : ""}${entrance ? "" : " sd-static"}`}>
      {/* ── header: identity ── */}
      <div className="sd-head">
        <div className="sd-id">
          <b className="sd-name">{s.name}</b>
          <span className="sd-addr">{shortAddr(s.address)}</span>
        </div>
      </div>

      {/* ── summary tiles ── */}
      <div className="sd-tiles">
        <Tile accent label="총 자산" value={s.portfolioUsd ?? `$${total.toLocaleString("en-US")}`} sub="Total value" />
        <Tile label="토큰" value={String(s.tokens.length)} sub="Holdings" />
        <Tile label="포지션" value={String(s.positions.length)} sub="Open" />
        <Tile label="위험 승인" value={String(highRiskCount)} sub="High risk" danger={highRiskCount > 0} />
      </div>

      {total > 0 && (
        <div className="sd-alloc" role="img" aria-label="토큰 비중">
          {s.tokens.map((t, i) => {
            const pct = ((t.usdNum ?? 0) / total) * 100;
            if (pct <= 0) return null;
            const gone = tokenRel(t.symbol) === "gone";
            return (
              <span
                key={t.address}
                className={`sd-alloc-seg${gone ? " sd-seg-gone" : ""}`}
                style={{ width: `${pct}%`, background: ALLOC_COLORS[i % ALLOC_COLORS.length] }}
                title={`${t.symbol} ${pct.toFixed(1)}%`}
              />
            );
          })}
        </div>
      )}

      {/* ── tokens │ positions, side by side, each boxed ── */}
      <div className={`sd-cols2${s.positions.length > 0 ? "" : " one"}`}>
        <Section box title="토큰" count={shown(s.tokens, (t) => tokenRel(t.symbol))} total={s.tokens.length} active={active}>
          <div className="sd-tokens">
            {s.tokens.map((t, i) => (
              <TokenRow
                key={t.address}
                t={t}
                total={total}
                color={ALLOC_COLORS[i % ALLOC_COLORS.length]}
                rel={tokenRel(t.symbol)}
                open={open.has(t.address)}
                onToggle={() => toggle(t.address)}
              />
            ))}
          </div>
        </Section>

        {s.positions.length > 0 && (
          <Section
            box
            title="포지션"
            count={shown(s.positions, (p) => protoRel(p.protocol))}
            total={s.positions.length}
            active={active}
          >
            <div className="sd-perps">
              {perps.map((p) => (
                <PerpCard key={p.id} p={p} rel={protoRel(p.protocol)} open={open.has(p.id)} onToggle={() => toggle(p.id)} />
              ))}
            </div>
            {others.map((p) => (
              <LendingRow key={p.id} p={p} rel={protoRel(p.protocol)} open={open.has(p.id)} onToggle={() => toggle(p.id)} />
            ))}
          </Section>
        )}
      </div>

      {/* ── approvals (full width, boxed) ── */}
      {s.approvals.length > 0 && (
        <Section
          box
          title="승인"
          count={shown(s.approvals, (a) => tokenRel(a.token))}
          total={s.approvals.length}
          active={active}
        >
          <div className="sd-apprs">
            {s.approvals.map((a) => (
              <ApprovalRow key={a.id} a={a} rel={tokenRel(a.token)} open={open.has(a.id)} onToggle={() => toggle(a.id)} />
            ))}
          </div>
        </Section>
      )}
    </div>
  );
}

function Tile({
  label,
  value,
  sub,
  accent,
  danger,
}: {
  label: string;
  value: string;
  sub: string;
  accent?: boolean;
  danger?: boolean;
}) {
  return (
    <div className={`sd-tile${accent ? " accent" : ""}${danger ? " danger" : ""}`}>
      <span className="sd-tile-label">{label}</span>
      <span className="sd-tile-value">{value}</span>
      <span className="sd-tile-sub">{sub}</span>
    </div>
  );
}

function Section({
  title,
  count,
  total,
  active,
  box,
  children,
}: {
  title: string;
  count: number;
  total: number;
  active: boolean;
  box?: boolean;
  children: ReactNode;
}) {
  return (
    <div className={`sd-section${box ? " box" : ""}`}>
      <div className="sd-section-head">
        <span className="sd-section-title">{title}</span>
        <span className="sd-section-count">
          {active && count !== total ? (
            <>
              <span className="sd-count-now">{count}</span>
              <span className="sd-count-of">/{total}</span>
            </>
          ) : (
            total
          )}
        </span>
      </div>
      {children}
    </div>
  );
}

/** A small chevron that rotates when its row is open. */
function Chevron({ open }: { open: boolean }) {
  return <span className={`sd-chev${open ? " open" : ""}`} aria-hidden>›</span>;
}

function DetailGrid({ rows }: { rows: [string, ReactNode][] }) {
  return (
    <dl className="sd-detail-grid">
      {rows.map(([k, v]) => (
        <div key={k} className="sd-dl-row">
          <dt>{k}</dt>
          <dd>{v}</dd>
        </div>
      ))}
    </dl>
  );
}

function TokenRow({
  t,
  total,
  color,
  rel,
  open,
  onToggle,
}: {
  t: TokenHolding;
  total: number;
  color: string;
  rel: RelState;
  open: boolean;
  onToggle: () => void;
}) {
  const pct = total > 0 ? ((t.usdNum ?? 0) / total) * 100 : 0;
  return (
    <div className={`sd-token${relClass(rel)}${open ? " open" : ""}`}>
      <button type="button" className="sd-token-main" onClick={onToggle} aria-expanded={open}>
        <span className="sd-token-dot" style={{ background: color }} />
        <span className="sd-token-sym">{t.symbol}</span>
        <span className="sd-token-bal">{t.balance}</span>
        <span className="sd-token-bar">
          <span className="sd-token-bar-fill" style={{ width: `${pct}%`, background: color }} />
        </span>
        <span className="sd-token-usd">{t.usd ?? ""}</span>
        <Chevron open={open} />
      </button>
      <div className="sd-detail">
        <DetailGrid
          rows={[
            ["비중", `${pct.toFixed(1)}%`],
            ["단가", t.priceUsd ?? "—"],
            ["사용 대기", t.committed && t.committed !== "0.00" ? `${t.committed} ${t.symbol}` : "없음"],
            ["컨트랙트", <code key="c">{shortAddr(t.address)}</code>],
          ]}
        />
      </div>
    </div>
  );
}

function PerpCard({
  p,
  rel,
  open,
  onToggle,
}: {
  p: PositionView;
  rel: RelState;
  open: boolean;
  onToggle: () => void;
}) {
  return (
    <div className={`sd-perp${relClass(rel)}${open ? " open" : ""}`}>
      <button type="button" className="sd-perp-top" onClick={onToggle} aria-expanded={open}>
        <b className="sd-perp-name">{p.label}</b>
        {p.side && <span className={`sd-side ${p.side}`}>{p.side === "long" ? "롱" : "숏"}</span>}
        {p.leverage && <span className="sd-lev">{p.leverage}</span>}
        {p.pnlUsd && <span className={`sd-pnl ${p.pnlSign ?? "up"}`}>{p.pnlUsd}</span>}
        <Chevron open={open} />
      </button>
      <div className="sd-perp-summary">
        {p.sizeUsd && <Metric label="규모" value={p.sizeUsd} />}
      </div>
      <div className="sd-detail">
        <DetailGrid
          rows={[
            ["진입가", p.entryPrice ?? "—"],
            ["현재가", p.markPrice ?? "—"],
            ["청산가", <span key="l" className="sd-val-danger">{p.liqPrice ?? "—"}</span>],
            ["마진", p.marginUsd ?? "—"],
            ["ROE", <span key="r" className={p.pnlSign === "down" ? "sd-val-danger" : "sd-val-good"}>{p.roe ?? "—"}</span>],
          ]}
        />
      </div>
    </div>
  );
}

function LendingRow({
  p,
  rel,
  open,
  onToggle,
}: {
  p: PositionView;
  rel: RelState;
  open: boolean;
  onToggle: () => void;
}) {
  const lowHealth = p.health !== undefined && Number(p.health) < 1.5;
  return (
    <div className={`sd-lend${relClass(rel)}${open ? " open" : ""}`}>
      <button type="button" className="sd-lend-main" onClick={onToggle} aria-expanded={open}>
        <span className="sd-lend-proto">{p.protocol}</span>
        <span className="sd-lend-name">{p.label}</span>
        {p.sizeUsd && <span className="sd-lend-size">{p.sizeUsd}</span>}
        {p.health && <span className={`sd-health${lowHealth ? " low" : ""}`}>HF {p.health}</span>}
        <Chevron open={open} />
      </button>
      <div className="sd-detail">
        <DetailGrid
          rows={[
            ["담보", p.collateralUsd ?? "—"],
            ["부채", p.debtUsd ?? "—"],
            ["순포지션", p.sizeUsd ?? "—"],
            ["건전성(HF)", <span key="h" className={lowHealth ? "sd-val-danger" : "sd-val-good"}>{p.health ?? "—"}</span>],
          ]}
        />
      </div>
    </div>
  );
}

function Metric({ label, value, danger }: { label: string; value: string; danger?: boolean }) {
  return (
    <div className="sd-metric">
      <span className="sd-metric-lbl">{label}</span>
      <span className={`sd-metric-val${danger ? " danger" : ""}`}>{value}</span>
    </div>
  );
}

function ApprovalRow({
  a,
  rel,
  open,
  onToggle,
}: {
  a: ApprovalView;
  rel: RelState;
  open: boolean;
  onToggle: () => void;
}) {
  const risk = a.risk ?? (a.unlimited ? "high" : "low");
  return (
    <div className={`sd-appr risk-${risk}${relClass(rel)}${open ? " open" : ""}`}>
      <button type="button" className="sd-appr-main" onClick={onToggle} aria-expanded={open}>
        <span className="sd-appr-token">{a.token}</span>
        <span className="sd-appr-arrow">→</span>
        <span className="sd-appr-spender">
          {a.spender}
          {a.spenderAddress && <span className="sd-appr-addr">{shortAddr(a.spenderAddress)}</span>}
        </span>
        <span className={`sd-appr-amt${a.unlimited ? " unl" : ""}`}>{a.amount ?? (a.unlimited ? "무제한" : "")}</span>
        <span className={`sd-risk ${risk}`}>{risk === "high" ? "위험" : risk === "med" ? "주의" : "안전"}</span>
        <Chevron open={open} />
      </button>
      <div className="sd-detail">
        {a.riskReason && <p className={`sd-appr-reason ${risk}`}>{a.riskReason}</p>}
        <DetailGrid
          rows={[
            ["스펜더", <code key="s">{a.spenderAddress ? shortAddr(a.spenderAddress) : a.spender}</code>],
            ["한도", a.amount ?? (a.unlimited ? "무제한" : "—")],
            ["범위", a.scope ?? "ERC-20"],
            ["토큰", <code key="t">{a.tokenAddress ? shortAddr(a.tokenAddress) : a.token}</code>],
            ["승인일", a.grantedAt ?? "—"],
          ]}
        />
      </div>
    </div>
  );
}
