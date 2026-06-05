/**
 * State timeline panel.
 *
 * - Top: scrubber showing S₀ → S₁ → … → S_final. Each node is clickable
 *   and shows the post-step verdict pill.
 * - Body: the snapshot at the cursor position, with tokens / positions /
 *   NFTs. Rows that changed vs. the previous snapshot get a "Δ" highlight
 *   and an inline before→after micro-diff.
 * - Toggles: "변화만" and "정렬: 변화 우선" so the user can focus on
 *   what moved without losing the full snapshot.
 * - Highlight overlay: when a violation is focused on the right rail,
 *   referenced state rows light up in red with a "차단 원인" badge.
 */
import { useMemo } from "react";
import type { SequenceStepResult } from "../../cedar";
import type {
  SimSnapshot,
  SimState,
  SimTokenRow,
  SimPositionRow,
} from "./state-mock";
import { inferPolicyRefs } from "./policy-refs";

export interface StatePanelProps {
  /** All snapshots, snapshots[0] is S₀ (initial). length = steps.length + 1 */
  snapshots: SimSnapshot[];
  /** index into snapshots that the user is currently viewing */
  cursorIdx: number;
  setCursorIdx: (n: number) => void;
  /** verdict for the step that *produced* snapshots[cursorIdx] — i.e. step (cursorIdx - 1) */
  verdictForCursor: SequenceStepResult | undefined;
  /** all per-step verdicts so the scrubber pills can color themselves */
  verdictByStep: Map<number, SequenceStepResult>;

  /** Display options */
  changedOnly: boolean;
  setChangedOnly: (v: boolean) => void;
  changedFirst: boolean;
  setChangedFirst: (v: boolean) => void;

  /** Currently-focused violation (drives the red highlight overlay). */
  focusedViolation: { stepIdx: number; policyId: number } | null;
  /** Reverse-lookup: when user clicks a row, find policy(ies) implicating it. */
  onStateRowClick: (rowKey: string | null) => void;
  /** Currently-focused state row key (when user clicked a row). */
  focusedRowKey: string | null;
}

export function StatePanel(props: StatePanelProps) {
  const {
    snapshots,
    cursorIdx,
    setCursorIdx,
    verdictForCursor,
    verdictByStep,
    changedOnly,
    setChangedOnly,
    changedFirst,
    setChangedFirst,
    focusedViolation,
    onStateRowClick,
    focusedRowKey,
  } = props;

  const snap = snapshots[cursorIdx];
  const prev = cursorIdx > 0 ? snapshots[cursorIdx - 1] : null;
  const changed = snap?.changed ?? new Set<string>();

  // Resolve the violation's implicated state rows (highlight overlay).
  const blockedKeys = useMemo<{ rowKeys: Set<string>; buckets: Set<string> }>(() => {
    const rowKeys = new Set<string>();
    const buckets = new Set<string>();
    if (!focusedViolation || !verdictForCursor) return { rowKeys, buckets };
    const o = verdictForCursor.policy_results.find(
      (x) => x.policy_id === focusedViolation.policyId,
    );
    if (!o) return { rowKeys, buckets };
    const refs = inferPolicyRefs(o.policy_name, snap?.state ?? emptyState());
    refs.rowKeys.forEach((k) => rowKeys.add(k));
    refs.buckets.forEach((b) => buckets.add(b));
    return { rowKeys, buckets };
  }, [focusedViolation, verdictForCursor, snap]);

  if (!snap) return null;

  return (
    <div className="sim-card state-card">
      <div className="card-head">
        <h3>상태 (state)</h3>
        <div className="state-tools">
          <label className="checkline">
            <input
              type="checkbox"
              checked={changedOnly}
              onChange={(e) => setChangedOnly(e.target.checked)}
            />
            <span>변화만</span>
          </label>
          <label className="checkline">
            <input
              type="checkbox"
              checked={changedFirst}
              onChange={(e) => setChangedFirst(e.target.checked)}
            />
            <span>변화 우선 정렬</span>
          </label>
        </div>
      </div>

      <Scrubber
        snapshots={snapshots}
        cursorIdx={cursorIdx}
        setCursorIdx={setCursorIdx}
        verdictByStep={verdictByStep}
      />

      <div className="state-meta">
        <div>
          <span className="m-label">시점:</span>{" "}
          <strong>{cursorIdx === 0 ? "S₀ · 초기 상태" : `S${sub(cursorIdx)} · TX ${cursorIdx} 적용 후`}</strong>
        </div>
        <div>
          <span className="m-label">포트폴리오:</span>{" "}
          ${fmt(snap.state.portfolioUsd)}
          {prev && snap.state.portfolioUsd !== prev.state.portfolioUsd && (
            <span
              className={`delta ${snap.state.portfolioUsd > prev.state.portfolioUsd ? "up" : "down"}`}
            >
              {sign(snap.state.portfolioUsd - prev.state.portfolioUsd)}$
              {fmt(Math.abs(snap.state.portfolioUsd - prev.state.portfolioUsd))}
            </span>
          )}
        </div>
        <div>
          <span className="m-label">변경:</span>{" "}
          <strong>{changed.size}</strong> rows
        </div>
      </div>

      <TokenSection
        title="토큰 잔고"
        rows={snap.state.tokens}
        prev={prev?.state.tokens ?? []}
        changed={changed}
        changedOnly={changedOnly}
        changedFirst={changedFirst}
        blockedRowKeys={blockedKeys.rowKeys}
        bucketBlocked={blockedKeys.buckets.has("tokens")}
        focusedRowKey={focusedRowKey}
        onRowClick={onStateRowClick}
      />

      <PositionSection
        rows={snap.state.positions}
        prev={prev?.state.positions ?? []}
        changed={changed}
        changedOnly={changedOnly}
        changedFirst={changedFirst}
        blockedRowKeys={blockedKeys.rowKeys}
        bucketBlocked={blockedKeys.buckets.has("positions")}
        focusedRowKey={focusedRowKey}
        onRowClick={onStateRowClick}
      />

      {snap.state.nfts.length > 0 && (
        <NftSection rows={snap.state.nfts} />
      )}
    </div>
  );
}

// ── scrubber ──────────────────────────────────────────────────────────────

function Scrubber({
  snapshots,
  cursorIdx,
  setCursorIdx,
  verdictByStep,
}: {
  snapshots: SimSnapshot[];
  cursorIdx: number;
  setCursorIdx: (n: number) => void;
  verdictByStep: Map<number, SequenceStepResult>;
}) {
  return (
    <div className="scrubber">
      {snapshots.map((_, i) => {
        // node[0] = S₀ (no verdict). node[i>0] = state after step (i-1)
        const v = i === 0 ? undefined : verdictByStep.get(i - 1);
        const cls = v ? `verdict-${v.verdict}` : "neutral";
        const sel = i === cursorIdx ? "is-sel" : "";
        return (
          <button
            key={i}
            className={`scr-node ${cls} ${sel}`}
            onClick={() => setCursorIdx(i)}
            title={i === 0 ? "초기 상태" : `TX ${i} 적용 후`}
          >
            <span className="scr-label">S{sub(i)}</span>
            {v && <span className={`vpill xs ${v.verdict}`}>{v.verdict}</span>}
          </button>
        );
      })}
    </div>
  );
}

// ── tokens ────────────────────────────────────────────────────────────────

function TokenSection({
  title,
  rows,
  prev,
  changed,
  changedOnly,
  changedFirst,
  blockedRowKeys,
  bucketBlocked,
  focusedRowKey,
  onRowClick,
}: {
  title: string;
  rows: SimTokenRow[];
  prev: SimTokenRow[];
  changed: Set<string>;
  changedOnly: boolean;
  changedFirst: boolean;
  blockedRowKeys: Set<string>;
  bucketBlocked: boolean;
  focusedRowKey: string | null;
  onRowClick: (k: string | null) => void;
}) {
  const prevMap = new Map(prev.map((r) => [r.key, r]));
  let visible = rows.slice();
  if (changedOnly) visible = visible.filter((r) => changed.has(r.key));
  if (changedFirst) {
    visible.sort((a, b) => {
      const ca = changed.has(a.key) ? 0 : 1;
      const cb = changed.has(b.key) ? 0 : 1;
      return ca - cb;
    });
  }

  const totalUsd = rows.reduce((acc, r) => acc + (r.usd ?? 0), 0);

  return (
    <section className="st-section">
      <header className="st-head">
        <span className="st-title">{title}</span>
        <span className="st-count">{rows.length} 토큰</span>
        <span className="st-total">${fmt(totalUsd)}</span>
      </header>
      {visible.length === 0 && (
        <div className="muted-line sm">표시할 row 없음</div>
      )}
      <ul className="st-rows">
        {visible.map((r) => {
          const p = prevMap.get(r.key);
          const isChanged = changed.has(r.key);
          const isBlocked = blockedRowKeys.has(r.key) || bucketBlocked;
          const isFocused = focusedRowKey === r.key;
          const dAmt = p ? r.amount - p.amount : 0;
          const dUsd = p && r.usd != null && p.usd != null ? r.usd - p.usd : 0;
          return (
            <li
              key={r.key}
              className={`st-row token ${isChanged ? "changed" : ""} ${isBlocked ? "blocked" : ""} ${isFocused ? "is-focused" : ""}`}
              onClick={() => onRowClick(isFocused ? null : r.key)}
            >
              <div className="st-row-l">
                <span className="t-glyph">{r.symbol[0]}</span>
                <div className="t-id">
                  <div className="t-sym">
                    {r.symbol}
                    <span className="chain-pill">{r.chain}</span>
                    {r.stale && <span className="tag stale">STALE</span>}
                    {r.unknown && <span className="tag unknown">UNKNOWN</span>}
                  </div>
                  {r.note && <div className="t-note">{r.note}</div>}
                </div>
              </div>
              <div className="st-row-r">
                <div className="amt-line">
                  {isChanged && (
                    <span className={`delta ${dAmt >= 0 ? "up" : "down"}`}>
                      {sign(dAmt)}{fmt(Math.abs(dAmt))}
                    </span>
                  )}
                  <span className="amt">
                    {fmt(r.amount)} {r.symbol}
                  </span>
                </div>
                <div className="usd-line">
                  {r.usd == null ? (
                    <span className="muted">—</span>
                  ) : (
                    <>
                      <span>${fmt(r.usd)}</span>
                      {isChanged && dUsd !== 0 && (
                        <span className={`delta ${dUsd >= 0 ? "up" : "down"} sm`}>
                          {sign(dUsd)}${fmt(Math.abs(dUsd))}
                        </span>
                      )}
                    </>
                  )}
                </div>
              </div>
              {isBlocked && (
                <span className="blocked-pill">차단 원인</span>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

// ── positions ─────────────────────────────────────────────────────────────

function PositionSection({
  rows,
  prev,
  changed,
  changedOnly,
  changedFirst,
  blockedRowKeys,
  bucketBlocked,
  focusedRowKey,
  onRowClick,
}: {
  rows: SimPositionRow[];
  prev: SimPositionRow[];
  changed: Set<string>;
  changedOnly: boolean;
  changedFirst: boolean;
  blockedRowKeys: Set<string>;
  bucketBlocked: boolean;
  focusedRowKey: string | null;
  onRowClick: (k: string | null) => void;
}) {
  if (rows.length === 0) return null;
  const prevMap = new Map(prev.map((r) => [r.key, r]));

  let visible = rows.slice();
  if (changedOnly) visible = visible.filter((r) => changed.has(r.key));
  if (changedFirst) {
    visible.sort((a, b) => {
      const ca = changed.has(a.key) ? 0 : 1;
      const cb = changed.has(b.key) ? 0 : 1;
      return ca - cb;
    });
  }

  return (
    <section className="st-section">
      <header className="st-head">
        <span className="st-title">포지션</span>
        <span className="st-count">{rows.length} 프로토콜</span>
      </header>
      {visible.length === 0 && (
        <div className="muted-line sm">표시할 row 없음</div>
      )}
      <ul className="st-rows positions">
        {visible.map((r) => {
          const p = prevMap.get(r.key);
          const isChanged = changed.has(r.key);
          const isBlocked = blockedRowKeys.has(r.key) || bucketBlocked;
          const isFocused = focusedRowKey === r.key;
          const dColl = p ? r.collateralUsd - p.collateralUsd : 0;
          const dDebt = p ? r.debtUsd - p.debtUsd : 0;
          const dHf = p ? r.healthFactor - p.healthFactor : 0;
          const dLtv = p ? r.ltv - p.ltv : 0;
          const hfFloorViolated = r.hfFloor != null && r.healthFactor < r.hfFloor;
          const ltvMaxViolated = r.ltvMax != null && r.ltv > r.ltvMax;
          return (
            <li
              key={r.key}
              className={`st-row position ${isChanged ? "changed" : ""} ${isBlocked ? "blocked" : ""} ${isFocused ? "is-focused" : ""}`}
              onClick={() => onRowClick(isFocused ? null : r.key)}
            >
              <div className="pos-head">
                <strong>{r.protocol}</strong>
                <span className="pos-pair">· {r.pair}</span>
              </div>
              <div className="pos-grid">
                <Metric
                  label="담보"
                  value={`$${fmt(r.collateralUsd)}`}
                  delta={isChanged && dColl !== 0 ? `${sign(dColl)}$${fmt(Math.abs(dColl))}` : null}
                  up={dColl >= 0}
                />
                <Metric
                  label="부채"
                  value={`$${fmt(r.debtUsd)}`}
                  delta={isChanged && dDebt !== 0 ? `${sign(dDebt)}$${fmt(Math.abs(dDebt))}` : null}
                  up={dDebt <= 0}
                />
                <Metric
                  label="HF"
                  value={r.healthFactor.toFixed(2)}
                  delta={isChanged && Math.abs(dHf) > 0.005 ? `${sign(dHf)}${Math.abs(dHf).toFixed(2)}` : null}
                  up={dHf >= 0}
                  bad={hfFloorViolated}
                  floor={r.hfFloor != null ? `floor ${r.hfFloor.toFixed(2)}` : undefined}
                />
                <Metric
                  label="LTV"
                  value={`${Math.round(r.ltv * 100)}%`}
                  delta={isChanged && Math.abs(dLtv) > 0.001 ? `${sign(dLtv)}${Math.round(Math.abs(dLtv) * 100)}%` : null}
                  up={dLtv <= 0}
                  bad={ltvMaxViolated}
                  floor={r.ltvMax != null ? `max ${Math.round(r.ltvMax * 100)}%` : undefined}
                />
              </div>
              {isBlocked && (
                <span className="blocked-pill">차단 원인</span>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function Metric({
  label,
  value,
  delta,
  up,
  bad,
  floor,
}: {
  label: string;
  value: string;
  delta: string | null;
  up: boolean;
  bad?: boolean;
  floor?: string;
}) {
  return (
    <div className={`metric ${bad ? "bad" : ""}`}>
      <div className="m-label">{label}</div>
      <div className="m-val">
        {value}
        {delta && (
          <span className={`delta sm ${up ? "up" : "down"}`}>{delta}</span>
        )}
      </div>
      {floor && <div className="m-floor">{floor}</div>}
    </div>
  );
}

// ── NFTs ──────────────────────────────────────────────────────────────────

function NftSection({ rows }: { rows: SimSnapshot["state"]["nfts"] }) {
  return (
    <section className="st-section">
      <header className="st-head">
        <span className="st-title">컬렉티블 · NFT</span>
        <span className="st-count">{rows.length}</span>
      </header>
      <ul className="st-rows nfts">
        {rows.map((n) => (
          <li key={n.key} className="st-row nft">
            <div className="nft-thumb">🖼</div>
            <div className="nft-id">
              <div className="nft-name">{n.name}</div>
              <div className="nft-coll">
                {n.collection}
                <span className="chain-pill">{n.chain}</span>
              </div>
            </div>
            {n.floorEth != null && (
              <div className="nft-floor">{n.floorEth} ETH<br /><span>floor</span></div>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}

// ── helpers ───────────────────────────────────────────────────────────────

function fmt(n: number): string {
  if (Math.abs(n) >= 1000) return n.toLocaleString(undefined, { maximumFractionDigits: 2 });
  if (Math.abs(n) >= 1) return n.toLocaleString(undefined, { maximumFractionDigits: 4 });
  return n.toLocaleString(undefined, { maximumFractionDigits: 6 });
}
function sign(n: number): string {
  return n > 0 ? "+" : n < 0 ? "−" : "";
}
function sub(n: number): string {
  // unicode subscript digits
  return String(n)
    .split("")
    .map((d) => "₀₁₂₃₄₅₆₇₈₉"[Number(d)] ?? d)
    .join("");
}
function emptyState(): SimState {
  return { tokens: [], positions: [], nfts: [], portfolioUsd: 0 };
}
