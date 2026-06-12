/**
 * Per-wallet state panel for the integrated simulator (replaces the prior
 * single-wallet `WasmStatePanel`).
 *
 * Renders one collapsible section per selected wallet. Each section shows
 * the wallet's state at the current scrubber cursor plus the delta that
 * produced it (when the cursor is past 0 AND this wallet was the one that
 * ran the step). The scrubber itself is page-shared and lives here as
 * the primary state-time control.
 *
 * Inputs:
 *   - `histories`: Map<walletAddr, OpaqueWalletState[]>. `history[i]` is
 *     the wallet's state after step `i`. `history[0] = initial`.
 *   - `deltas`: per-step `OpaqueStateDelta` AND the address that step
 *     belonged to; only the matching wallet shows the delta.
 *   - `cursorIdx`: page-shared cursor into the step axis. `0 = initial`,
 *     `i > 0 = after step i`.
 */

import { useMemo } from "react";
import { useTranslation } from "react-i18next";

import {
  aggregateWalletStates,
  filterAggregateByChain,
  formatBalance,
  formatSignedDelta,
  isStateDeltaEmpty,
  parseStateDelta,
  parseWalletState,
  shortAddr,
  tokenKeyId,
  tokenKeysTouchedBy,
  filterViewByChain,
  type AggregatedTokenRow,
  type StateDeltaView,
  type TokenHoldingRow,
} from "./state-view";
import type { OpaqueStateDelta, OpaqueWalletState } from "./sim-bridge";
import { StepPicker } from "./StepPicker";

/** Per-step delta record. `walletAddr` is the wallet that ran the step
 *  (lowercase); only that wallet renders the delta payload. */
export interface SimStepDelta {
  walletAddr: string;
  delta: OpaqueStateDelta;
}

export interface WalletsStatePanelProps {
  /** Lowercase addresses of selected wallets, in render order. */
  selected: ReadonlyArray<string>;
  histories: Map<string, ReadonlyArray<OpaqueWalletState>>;
  /** One entry per simulated step; index `i` is the step that produced the
   *  `i+1`-th cursor position. `null` when no run has happened. */
  deltas: ReadonlyArray<SimStepDelta>;
  cursorIdx: number;
  setCursorIdx: (next: number) => void;
  /** Hide unchanged-token rows for the wallet that ran this step. */
  changedOnly: boolean;
  setChangedOnly: (next: boolean) => void;
  /** CAIP-2 chain filter. */
  chain: string;
}

export function WalletsStatePanel(props: WalletsStatePanelProps) {
  const { t } = useTranslation("simulation");
  const {
    selected,
    histories,
    deltas,
    cursorIdx,
    setCursorIdx,
    changedOnly,
    setChangedOnly,
    chain,
  } = props;

  const totalSteps = deltas.length;

  // The delta for this cursor (cursor 0 has no delta).
  const currentDelta = cursorIdx === 0 ? null : deltas[cursorIdx - 1] ?? null;
  const currentDeltaView = useMemo(
    () => (currentDelta ? parseStateDelta(currentDelta.delta) : null),
    [currentDelta],
  );
  const touched = useMemo(
    () => (currentDeltaView ? tokenKeysTouchedBy(currentDeltaView) : new Set<string>()),
    [currentDeltaView],
  );

  return (
    <div className="sim-card wallets-state-card">
      <div className="card-head">
        <h3>{t("historic.walletsState.title")}</h3>
        <span className="meta">{t("historic.walletsState.walletCount", { count: selected.length })}</span>
      </div>

      <StepPicker
        totalSteps={totalSteps}
        cursorIdx={cursorIdx}
        setCursorIdx={setCursorIdx}
      />

      {/* Active-step summary — which wallet's delta drove this cursor. */}
      {currentDelta && currentDeltaView && (
        <div className="state-delta-summary">
          <span className="meta">{t("historic.walletsState.stepOwner")}</span>
          <code>{shortAddr(currentDelta.walletAddr)}</code>
          <span className="meta">{t("historic.walletsState.changes")}</span>
          {isStateDeltaEmpty(currentDeltaView) ? (
            <span className="muted">no state change</span>
          ) : (
            <>
              <span>token×{currentDeltaView.tokenChanges.length}</span>
              {currentDeltaView.positionChanges.length > 0 && (
                <span> · pos×{currentDeltaView.positionChanges.length}</span>
              )}
              {currentDeltaView.gasPaid && <span> · gas</span>}
            </>
          )}
        </div>
      )}

      <label className="state-filter">
        <input
          type="checkbox"
          checked={changedOnly}
          onChange={(e) => setChangedOnly(e.target.checked)}
          disabled={touched.size === 0}
        />
        <span>{t("historic.walletsState.changedOnly")}</span>
      </label>

      {selected.length === 0 ? (
        <div className="muted-line">{t("historic.walletsState.noSelection")}</div>
      ) : (
        <ul className="wallets-state-list">
          {/* Selected-wallets aggregate row. Only renders for >1 wallets
              (with a single wallet selected the per-wallet section already
              shows the same numbers). Re-uses the account aggregator but
              fed only the selected histories. */}
          {selected.length > 1 && (
            <li className="wallets-state-section wallets-state-section-sum">
              <SelectedSumSection
                selected={selected}
                histories={histories}
                cursorIdx={cursorIdx}
                chain={chain}
                deltaView={currentDeltaView}
                stepOwnerInSelection={
                  currentDelta
                    ? selected.includes(currentDelta.walletAddr)
                    : false
                }
              />
            </li>
          )}
          {selected.map((addr) => {
            const history = histories.get(addr);
            const snap = history?.[Math.min(cursorIdx, (history?.length ?? 1) - 1)];
            const isStepOwner = currentDelta?.walletAddr === addr;
            return (
              <li
                key={addr}
                className={`wallets-state-section ${isStepOwner ? "is-step-owner" : ""}`}
              >
                <WalletSection
                  addr={addr}
                  snap={snap ?? null}
                  chain={chain}
                  touched={isStepOwner ? touched : new Set<string>()}
                  changedOnly={changedOnly && isStepOwner}
                  deltaView={isStepOwner ? currentDeltaView : null}
                />
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

interface WalletSectionProps {
  addr: string;
  snap: OpaqueWalletState | null;
  chain: string;
  touched: Set<string>;
  changedOnly: boolean;
  /** This wallet's delta at the current cursor — `null` when the cursor
   *  is on initial state or this wallet didn't run the current step. The
   *  section uses it to render `+/-` change rows under touched balances. */
  deltaView: StateDeltaView | null;
}

function WalletSection({
  addr,
  snap,
  chain,
  touched,
  changedOnly,
  deltaView,
}: WalletSectionProps) {
  const { t } = useTranslation("simulation");
  /** Map tokenKeyId(key) → signed-decimal delta string. Built once per
   *  delta so the per-row render is O(1). */
  const balanceDeltaByKey = useMemo(() => {
    const m = new Map<string, string>();
    if (!deltaView) return m;
    for (const c of deltaView.tokenChanges) {
      if (c.kind === "balance_delta") {
        m.set(tokenKeyId(c.key), c.delta);
      }
    }
    return m;
  }, [deltaView]);
  const view = useMemo(() => {
    if (!snap) return null;
    return filterViewByChain(parseWalletState(snap), chain);
  }, [snap, chain]);

  const tokensToShow = useMemo<TokenHoldingRow[]>(() => {
    if (!view) return [];
    if (!changedOnly || touched.size === 0) return view.tokens;
    return view.tokens.filter((t) => touched.has(tokenKeyId(t.key)));
  }, [view, changedOnly, touched]);

  return (
    <>
      <header className="wallets-state-head">
        <code>{shortAddr(addr)}</code>
        {view?.walletAddress && view.walletAddress.toLowerCase() !== addr && (
          <span className="muted">→ {shortAddr(view.walletAddress)}</span>
        )}
        <span className="meta">
          {tokensToShow.length}
          {changedOnly && view ? ` / ${view.tokens.length}` : ""}
        </span>
      </header>
      {!view ? (
        <div className="muted-line">no snapshot</div>
      ) : tokensToShow.length === 0 ? (
        <div className="muted-line">
          {changedOnly ? t("historic.walletsState.noChange") : t("historic.walletsState.noTokens")}
        </div>
      ) : (
        <ul className="state-token-list">
          {tokensToShow.map((t) => {
            const id = tokenKeyId(t.key);
            const isTouched = touched.has(id);
            const rawDelta = balanceDeltaByKey.get(id);
            const deltaStr = rawDelta
              ? formatSignedDelta(rawDelta, t.decimals)
              : null;
            const deltaSign = rawDelta?.startsWith("-") ? "neg" : "pos";
            return (
              <li
                key={id}
                className={`state-token-row ${isTouched ? "is-changed" : ""}`}
              >
                <div className="state-token-head">
                  <strong>{t.symbol || "?"}</strong>
                  <code className="muted">{shortAddr(t.key.address)}</code>
                </div>
                <div className="state-token-balance" title={t.balance}>
                  {formatBalance(t.balance, t.decimals)}{" "}
                  <span className="muted state-token-symbol">
                    {t.symbol || ""}
                  </span>
                  {deltaStr && (
                    <span
                      className={`state-token-delta state-token-delta-${deltaSign}`}
                      title={rawDelta ?? undefined}
                    >
                      {deltaStr}
                    </span>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </>
  );
}

interface SelectedSumSectionProps {
  selected: ReadonlyArray<string>;
  histories: Map<string, ReadonlyArray<OpaqueWalletState>>;
  cursorIdx: number;
  chain: string;
  /** Active step's delta — used to render the same +/- pill on the sum
   *  row that the per-wallet section shows. Only meaningful when the
   *  step's owner is one of the selected wallets. */
  deltaView: StateDeltaView | null;
  stepOwnerInSelection: boolean;
}

function SelectedSumSection({
  selected,
  histories,
  cursorIdx,
  chain,
  deltaView,
  stepOwnerInSelection,
}: SelectedSumSectionProps) {
  const { t } = useTranslation("simulation");
  // Alias for use inside the token map below, where `t` is shadowed by the row.
  const tr = t;
  const aggregate = useMemo(() => {
    const views = [];
    for (const addr of selected) {
      const hist = histories.get(addr);
      if (!hist) continue;
      const snap = hist[Math.min(cursorIdx, hist.length - 1)];
      if (!snap) continue;
      views.push(parseWalletState(snap));
    }
    return filterAggregateByChain(aggregateWalletStates(views), chain);
  }, [selected, histories, cursorIdx, chain]);

  /** Same delta map as `WalletSection`, but only meaningful when the
   *  step's owner is in the selection — otherwise the aggregated sum
   *  didn't change at this step. */
  const balanceDeltaByKey = useMemo(() => {
    const m = new Map<string, string>();
    if (!deltaView || !stepOwnerInSelection) return m;
    for (const c of deltaView.tokenChanges) {
      if (c.kind === "balance_delta") {
        m.set(tokenKeyId(c.key), c.delta);
      }
    }
    return m;
  }, [deltaView, stepOwnerInSelection]);

  return (
    <>
      <header className="wallets-state-head">
        <strong>{t("historic.walletsState.selectedSum")}</strong>
        <span className="meta">{t("historic.walletsState.walletCount", { count: aggregate.walletCount })}</span>
      </header>
      {aggregate.tokens.length === 0 ? (
        <div className="muted-line">{t("historic.noTokensOnChain")}</div>
      ) : (
        <ul className="state-token-list">
          {aggregate.tokens.map((t: AggregatedTokenRow) => {
            const id = tokenKeyId(t.key);
            const rawDelta = balanceDeltaByKey.get(id);
            const deltaStr = rawDelta
              ? formatSignedDelta(rawDelta, t.decimals)
              : null;
            const deltaSign = rawDelta?.startsWith("-") ? "neg" : "pos";
            return (
              <li
                key={id}
                className={`state-token-row ${rawDelta ? "is-changed" : ""}`}
              >
                <div className="state-token-head">
                  <strong>{t.symbol || "?"}</strong>
                  <code className="muted">{shortAddr(t.key.address)}</code>
                  <span className="muted state-token-wcount">
                    {tr("historic.walletCountParen", { count: t.walletCount })}
                  </span>
                </div>
                <div
                  className="state-token-balance"
                  title={t.totalBalance}
                >
                  {formatBalance(t.totalBalance, t.decimals)}{" "}
                  <span className="muted state-token-symbol">
                    {t.symbol || ""}
                  </span>
                  {deltaStr && (
                    <span
                      className={`state-token-delta state-token-delta-${deltaSign}`}
                      title={rawDelta ?? undefined}
                    >
                      {deltaStr}
                    </span>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </>
  );
}
