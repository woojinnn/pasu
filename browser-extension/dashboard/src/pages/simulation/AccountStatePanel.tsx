/**
 * Account-level rollup of every selected wallet's state at the current
 * scrubber cursor.
 *
 * Rendering shape mirrors {@link WalletsStatePanel} (same scrubber, same
 * token-row layout) but the data is the aggregate produced by
 * `state-view.aggregateWalletStates` — token balances summed across the
 * selected wallets, position / pending / approval counts summed. Per-step
 * deltas don't make sense at this level (the simulator runs per-wallet);
 * the panel just shows the rolled-up snapshot at the cursor.
 */

import { useMemo } from "react";

import {
  aggregateWalletStates,
  filterAggregateByChain,
  formatBalance,
  parseWalletState,
  shortAddr,
  type AggregatedTokenRow,
} from "./state-view";
import type { OpaqueWalletState } from "./sim-bridge";
import { StepPicker } from "./StepPicker";

export interface AccountStatePanelProps {
  /** Map<walletAddress (lowercased), state-history>. `history[i]` is the
   *  wallet's state after step `i` (with `history[0] = initial`). The
   *  cursor index reads `history[cursorIdx]` across EVERY entry — the
   *  account view is intentionally independent of the wallet-selection
   *  UI: it always rolls up every registered wallet. */
  histories: Map<string, ReadonlyArray<OpaqueWalletState>>;
  cursorIdx: number;
  setCursorIdx: (next: number) => void;
  /** Step axis length — usually `simulator output count`. Derived from
   *  the longest history at the page level so the picker here matches the
   *  one in `WalletsStatePanel`. */
  totalSteps: number;
  /** CAIP-2 chain string — narrows the rendered token list to the
   *  page-active chain. */
  chain: string;
}

export function AccountStatePanel(props: AccountStatePanelProps) {
  const { histories, cursorIdx, setCursorIdx, totalSteps, chain } = props;

  const aggregate = useMemo(() => {
    const views = [];
    for (const hist of histories.values()) {
      const snap = hist[Math.min(cursorIdx, hist.length - 1)];
      if (!snap) continue;
      views.push(parseWalletState(snap));
    }
    const raw = aggregateWalletStates(views);
    return filterAggregateByChain(raw, chain);
  }, [histories, cursorIdx, chain]);

  return (
    <div className="sim-card account-state-card">
      <div className="card-head">
        <h3>계정 단위 state</h3>
        <span className="meta">{aggregate.walletCount} 지갑 합계</span>
      </div>

      <StepPicker
        totalSteps={totalSteps}
        cursorIdx={cursorIdx}
        setCursorIdx={setCursorIdx}
      />

      {aggregate.walletCount === 0 ? (
        <div className="muted-line">
          등록된 지갑이 없습니다.
        </div>
      ) : (
        <>
          <div className="state-section">
            <header className="state-section-head">
              <span>토큰 보유 (합계)</span>
              <span className="meta">{aggregate.tokens.length}</span>
            </header>
            {aggregate.tokens.length === 0 ? (
              <div className="muted-line">선택한 chain에 보유 토큰 없음</div>
            ) : (
              <ul className="state-token-list">
                {aggregate.tokens.map((t) => (
                  <AccountTokenRow key={tokenId(t)} t={t} />
                ))}
              </ul>
            )}
          </div>

          <div className="state-pills">
            <span className="state-pill">
              positions {aggregate.positionCount}
            </span>
            <span className="state-pill">
              pending {aggregate.pendingCount}
            </span>
            <span className="state-pill">
              erc20≈{aggregate.approvalCounts.erc20}
            </span>
            <span className="state-pill">
              op≈{aggregate.approvalCounts.setForAll}
            </span>
            <span className="state-pill">
              p2≈{aggregate.approvalCounts.permit2}
            </span>
          </div>
        </>
      )}
    </div>
  );
}

function AccountTokenRow({ t }: { t: AggregatedTokenRow }) {
  const display = formatBalance(t.totalBalance, t.decimals);
  return (
    <li className="state-token-row">
      <div className="state-token-head">
        <strong>{t.symbol || "?"}</strong>
        <code className="muted">{shortAddr(t.key.address)}</code>
        <span className="muted state-token-wcount">
          ({t.walletCount} 지갑)
        </span>
      </div>
      <div className="state-token-balance" title={t.totalBalance}>
        {display}{" "}
        <span className="muted state-token-symbol">{t.symbol || ""}</span>
      </div>
    </li>
  );
}

function tokenId(t: AggregatedTokenRow): string {
  return `${t.key.standard}:${t.key.chain}:${t.key.address}`;
}
