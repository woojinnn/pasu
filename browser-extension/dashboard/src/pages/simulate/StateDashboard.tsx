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
import { AnimatePresence, motion } from "framer-motion";
import { useTranslation } from "react-i18next";

import type { ApprovalView, PositionView, TokenHolding, WalletStateView } from "./types";

/** One dashboard widget (a category card). Adding a category = one entry in the
 *  widget list — the responsive grid + animations handle the rest. */
interface WidgetDef {
  key: string;
  title: string;
  total: number;
  /** Per-row: stable id, whether the policy filter hides it, and its element. */
  rows: { id: string; gone: boolean; el: ReactNode }[];
  /** Shown (muted) when the wallet has none of this category. */
  empty: string;
}

const ALLOC_COLORS = ["#0ea5e9", "#22c55e", "#a855f7", "#f59e0b", "#ec4899", "#64748b"];

/** Relevance predicates that drive the collapse/highlight in step 2. */
export interface StateFilter {
  active: boolean;
  /** Whole-widget relevance by category (tokens/positions/approvals). */
  isWidgetRelevant: (key: string) => boolean;
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
  const { t } = useTranslation("simulation");
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
        <Tile
          accent
          label={t("wizard.state.totalAssets")}
          value={s.portfolioUsd ?? `$${total.toLocaleString("en-US")}`}
          sub="Total value"
        />
        <Tile label={t("wizard.state.tokens")} value={String(s.tokens.length)} sub="Holdings" />
        <Tile label={t("wizard.state.positions")} value={String(s.positions.length)} sub="Open" />
        <Tile
          label={t("wizard.state.riskyApprovals")}
          value={String(highRiskCount)}
          sub="High risk"
          danger={highRiskCount > 0}
        />
      </div>

      {total > 0 && (
        <div className="sd-alloc" role="img" aria-label={t("wizard.state.allocAria")}>
          {s.tokens.map((t, i) => {
            const pct = ((t.usdNum ?? 0) / total) * 100;
            if (pct <= 0) return null;
            const gone = tokenRel(t.symbol) === "gone";
            return (
              <span
                key={`alloc-${i}`}
                className={`sd-alloc-seg${gone ? " sd-seg-gone" : ""}`}
                style={{ width: `${pct}%`, background: ALLOC_COLORS[i % ALLOC_COLORS.length] }}
                title={`${t.symbol} ${pct.toFixed(1)}%`}
              />
            );
          })}
        </div>
      )}

      {/* ── category widgets (tokens · positions · approvals · +future) ──
           Declared as data and flowed into a responsive grid, so a new category
           is one entry. Core widgets always render (muted empty state when the
           wallet has none) → every card is consistent. In step 2, framer-motion
           collapses filtered-out rows and reflows the survivors smoothly. */}
      <motion.div className="sd-grid" layout>
        <AnimatePresence initial={false}>
        {((): WidgetDef[] => [
          {
            key: "tokens",
            title: t("wizard.state.tokens"),
            total: s.tokens.length,
            empty: t("wizard.state.noTokens"),
            rows: s.tokens.map((t, i) => {
              // Row id unique even when addresses repeat/blank (native ETH).
              const id = `tok-${i}-${t.address}`;
              return {
                id,
                gone: tokenRel(t.symbol) === "gone",
                el: (
                  <TokenRow
                    t={t}
                    total={total}
                    color={ALLOC_COLORS[i % ALLOC_COLORS.length]}
                    rel={tokenRel(t.symbol)}
                    open={open.has(id)}
                    onToggle={() => toggle(id)}
                  />
                ),
              };
            }),
          },
          {
            key: "positions",
            title: t("wizard.state.positions"),
            total: s.positions.length,
            empty: t("wizard.state.noPositions"),
            rows: [
              ...perps.map((p) => ({
                id: p.id,
                gone: protoRel(p.protocol) === "gone",
                el: <PerpCard p={p} rel={protoRel(p.protocol)} open={open.has(p.id)} onToggle={() => toggle(p.id)} />,
              })),
              ...others.map((p) => ({
                id: p.id,
                gone: protoRel(p.protocol) === "gone",
                el: <LendingRow p={p} rel={protoRel(p.protocol)} open={open.has(p.id)} onToggle={() => toggle(p.id)} />,
              })),
            ],
          },
          {
            key: "approvals",
            title: t("wizard.state.approvals"),
            total: s.approvals.length,
            empty: t("wizard.state.noApprovals"),
            rows: s.approvals.map((a) => ({
              id: a.id,
              gone: tokenRel(a.token) === "gone",
              el: <ApprovalRow a={a} rel={tokenRel(a.token)} open={open.has(a.id)} onToggle={() => toggle(a.id)} />,
            })),
          },
        ])()
          .filter((w) => !active || filter!.isWidgetRelevant(w.key))
          .map((w) => {
          const vis = active ? w.rows.filter((r) => !r.gone) : w.rows;
          return (
            <motion.section
              layout
              key={w.key}
              className="sd-widget"
              initial={{ opacity: 0, scale: 0.97 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.97 }}
              transition={{ duration: 0.22, ease: [0.22, 0.8, 0.26, 1] }}
            >
              <div className="sd-section-head">
                <span className="sd-section-title">{w.title}</span>
                <span className="sd-section-count">
                  {active && vis.length !== w.total ? (
                    <>
                      <span className="sd-count-now">{vis.length}</span>
                      <span className="sd-count-of">/{w.total}</span>
                    </>
                  ) : (
                    w.total
                  )}
                </span>
              </div>
              {w.total === 0 ? (
                <div className="sd-empty-box">{w.empty}</div>
              ) : (
                <motion.div className="sd-widget-body" layout>
                  <AnimatePresence initial={false}>
                    {vis.map((r) => (
                      <motion.div
                        key={r.id}
                        layout
                        initial={{ opacity: 0, height: 0 }}
                        animate={{ opacity: 1, height: "auto" }}
                        exit={{ opacity: 0, height: 0 }}
                        transition={{ duration: 0.22, ease: [0.22, 0.8, 0.26, 1] }}
                        style={{ overflow: "hidden" }}
                      >
                        {r.el}
                      </motion.div>
                    ))}
                  </AnimatePresence>
                </motion.div>
              )}
            </motion.section>
          );
        })}
        </AnimatePresence>
      </motion.div>
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
  const { t: tr } = useTranslation("simulation");
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
            [tr("wizard.state.share"), `${pct.toFixed(1)}%`],
            [tr("wizard.state.price"), t.priceUsd ?? "—"],
            [
              tr("wizard.state.committed"),
              t.committed && t.committed !== "0.00" ? `${t.committed} ${t.symbol}` : tr("wizard.state.none"),
            ],
            [tr("wizard.state.contract"), <code key="c">{shortAddr(t.address)}</code>],
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
  const { t } = useTranslation("simulation");
  return (
    <div className={`sd-perp${relClass(rel)}${open ? " open" : ""}`}>
      <button type="button" className="sd-perp-top" onClick={onToggle} aria-expanded={open}>
        <b className="sd-perp-name">{p.label}</b>
        {p.side && (
          <span className={`sd-side ${p.side}`}>
            {p.side === "long" ? t("wizard.state.long") : t("wizard.state.short")}
          </span>
        )}
        {p.leverage && <span className="sd-lev">{p.leverage}</span>}
        {p.pnlUsd && <span className={`sd-pnl ${p.pnlSign ?? "up"}`}>{p.pnlUsd}</span>}
        <Chevron open={open} />
      </button>
      <div className="sd-perp-summary">
        {p.sizeUsd && <Metric label={t("wizard.state.size")} value={p.sizeUsd} />}
      </div>
      <div className="sd-detail">
        <DetailGrid
          rows={[
            [t("wizard.state.entryPrice"), p.entryPrice ?? "—"],
            [t("wizard.state.markPrice"), p.markPrice ?? "—"],
            [t("wizard.state.liqPrice"), <span key="l" className="sd-val-danger">{p.liqPrice ?? "—"}</span>],
            [t("wizard.state.margin"), p.marginUsd ?? "—"],
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
  const { t } = useTranslation("simulation");
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
            [t("wizard.state.collateral"), p.collateralUsd ?? "—"],
            [t("wizard.state.debt"), p.debtUsd ?? "—"],
            [t("wizard.state.netPosition"), p.sizeUsd ?? "—"],
            [
              t("wizard.state.healthFactor"),
              <span key="h" className={lowHealth ? "sd-val-danger" : "sd-val-good"}>{p.health ?? "—"}</span>,
            ],
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
  const { t } = useTranslation("simulation");
  const risk = a.risk ?? (a.unlimited ? "high" : "low");
  const riskLabel =
    risk === "high" ? t("wizard.state.riskHigh") : risk === "med" ? t("wizard.state.riskMed") : t("wizard.state.riskLow");
  return (
    <div className={`sd-appr risk-${risk}${relClass(rel)}${open ? " open" : ""}`}>
      <button type="button" className="sd-appr-main" onClick={onToggle} aria-expanded={open}>
        <span className="sd-appr-token">{a.token}</span>
        <span className="sd-appr-arrow">→</span>
        <span className="sd-appr-spender">
          {a.spender}
          {a.spenderAddress && <span className="sd-appr-addr">{shortAddr(a.spenderAddress)}</span>}
        </span>
        <span className={`sd-appr-amt${a.unlimited ? " unl" : ""}`}>
          {a.amount ?? (a.unlimited ? t("wizard.state.unlimited") : "")}
        </span>
        <span className={`sd-risk ${risk}`}>{riskLabel}</span>
        <Chevron open={open} />
      </button>
      <div className="sd-detail">
        {a.riskReason && <p className={`sd-appr-reason ${risk}`}>{a.riskReason}</p>}
        <DetailGrid
          rows={[
            [t("wizard.state.spender"), <code key="s">{a.spenderAddress ? shortAddr(a.spenderAddress) : a.spender}</code>],
            [t("wizard.state.allowance"), a.amount ?? (a.unlimited ? t("wizard.state.unlimited") : "—")],
            [t("wizard.state.scope"), a.scope ?? "ERC-20"],
            [t("wizard.state.token"), <code key="t">{a.tokenAddress ? shortAddr(a.tokenAddress) : a.token}</code>],
            [t("wizard.state.grantedAt"), a.grantedAt ?? "—"],
          ]}
        />
      </div>
    </div>
  );
}
