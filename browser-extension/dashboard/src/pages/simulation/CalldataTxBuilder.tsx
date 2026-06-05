/**
 * Calldata-shape TX builder for the integrated simulator.
 *
 * Replaces the Cedar-shaped `TxBuilder` (`{principal, action, resource}`)
 * with raw EVM `{to, calldata, value}` rows that the SW decode + simulate
 * + evaluate pipeline (`sim-bridge.simulateSequenceWithVerdicts`) consumes
 * directly. Per-row optional `label` mirrors the prior Cedar builder for
 * cross-panel cursor labelling.
 *
 * No "examples" dropdown yet — the prior Cedar-mode dropdown loaded
 * `getExampleTransactions()` rows whose shape was Cedar, not calldata.
 * A v2 dropdown sourced from `ExampleTransaction.meta.data` is plausible
 * future work but out of scope here.
 */

import type { EvaluateActionVerdict } from "./sim-bridge";

export const MAX_TX = 5;

export interface CalldataTxRow {
  /** Stable per-row id (UUID or fallback). Used as React key + as the
   *  cross-panel cursor reference. */
  id: string;
  /** Human-readable label. Free-form. */
  label?: string;
  /** Lowercase 0x address of the wallet this TX is signed from. Drives
   *  which per-wallet state the simulator threads through. Empty when
   *  no wallets are selected yet (the builder surfaces a hint). */
  fromWallet: string;
  /** "0x" + 40 hex. Case-insensitive on the engine side. */
  to: string;
  /** Raw "0x"-prefixed calldata. Empty / `"0x"` = bare native transfer. */
  calldata: string;
  /** `msg.value` as a base-10 decimal string. */
  value: string;
}

export interface CalldataTxBuilderProps {
  rows: CalldataTxRow[];
  setRows: (next: CalldataTxRow[]) => void;
  /** Per-row verdict, keyed by row id. Set after a simulation run. */
  verdictByRowId: Map<string, EvaluateActionVerdict>;
  /** Per-row engine error (already translated to Korean by
   *  `explainEngineError`). When set, the row renders a red banner
   *  instead of (or in addition to) a verdict pill. */
  errorByRowId: Map<string, string>;
  /** Currently-focused row (for cross-panel highlighting). */
  selectedId: string | null;
  onSelect: (id: string) => void;
  /** When the user is mid-simulation, the builder grays out edits. */
  isRunning: boolean;
  /** Lowercase addresses of the user's selected wallets. The "from"
   *  dropdown only offers these. */
  availableWallets: ReadonlyArray<string>;
}

export function CalldataTxBuilder(props: CalldataTxBuilderProps) {
  const {
    rows,
    setRows,
    verdictByRowId,
    errorByRowId,
    selectedId,
    onSelect,
    isRunning,
    availableWallets,
  } = props;
  const atCap = rows.length >= MAX_TX;

  const updateRow = (id: string, patch: Partial<CalldataTxRow>) => {
    setRows(rows.map((r) => (r.id === id ? { ...r, ...patch } : r)));
  };
  const move = (id: string, dir: -1 | 1) => {
    const idx = rows.findIndex((r) => r.id === id);
    if (idx < 0) return;
    const j = idx + dir;
    if (j < 0 || j >= rows.length) return;
    const next = rows.slice();
    [next[idx], next[j]] = [next[j], next[idx]];
    setRows(next);
  };
  const remove = (id: string) => {
    if (rows.length === 1) return;
    setRows(rows.filter((r) => r.id !== id));
  };
  const add = () => {
    if (atCap) return;
    setRows([
      ...rows,
      blankCalldataRow(rows.length, availableWallets[0] ?? ""),
    ]);
  };

  return (
    <div className="sim-card tx-builder">
      <div className="card-head">
        <h3>트랜잭션 큐</h3>
        <span className="cap-pill" data-at-cap={atCap || undefined}>
          {rows.length} / {MAX_TX}
        </span>
      </div>

      <div className="tools-row">
        <select
          defaultValue=""
          onChange={(e) => {
            const id = e.target.value;
            if (!id) return;
            const preset = CALLDATA_PRESETS.find((p) => p.id === id);
            if (preset && !atCap) {
              setRows([
                ...rows,
                {
                  id: newRowId(),
                  label: preset.label,
                  fromWallet: availableWallets[0] ?? "",
                  to: preset.to,
                  calldata: preset.calldata,
                  value: preset.value,
                },
              ]);
            }
            e.target.value = "";
          }}
          disabled={atCap || isRunning}
        >
          <option value="" disabled>
            🧪 예시에서 TX 추가…
          </option>
          {CALLDATA_PRESETS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
        <button className="btn" onClick={add} disabled={atCap || isRunning}>
          + 빈 TX
        </button>
      </div>

      <div className="tx-cards">
        {rows.map((r, idx) => {
          const verdict = verdictByRowId.get(r.id);
          const error = errorByRowId.get(r.id);
          // Error wins the visual treatment — engine couldn't even run
          // the action, so the verdict (if any) is stale.
          const verdictClass = error
            ? "verdict-error"
            : verdict
              ? verdict.kind === "pass"
                ? "verdict-pass"
                : verdict.kind === "warn"
                  ? "verdict-warn"
                  : "verdict-fail"
              : "";
          const isSel = selectedId === r.id;
          return (
            <div
              key={r.id}
              className={`tx-card ${verdictClass} ${isSel ? "is-sel" : ""}`}
              onClick={() => onSelect(r.id)}
            >
              <div className="tx-card-head">
                <div className="ord">{String(idx + 1).padStart(2, "0")}</div>
                <input
                  className="tx-label"
                  type="text"
                  placeholder="라벨 (예: USDC approve)"
                  value={r.label ?? ""}
                  onChange={(e) => updateRow(r.id, { label: e.target.value })}
                  onClick={(e) => e.stopPropagation()}
                  disabled={isRunning}
                />
                {error ? (
                  <span className="vpill error">ERROR</span>
                ) : verdict ? (
                  <span className={`vpill ${verdict.kind}`}>
                    {verdict.kind.toUpperCase()}
                  </span>
                ) : null}
                <div className="reorder">
                  <button
                    className="btn icon"
                    onClick={(e) => {
                      e.stopPropagation();
                      move(r.id, -1);
                    }}
                    disabled={idx === 0 || isRunning}
                    title="위로"
                  >
                    ▲
                  </button>
                  <button
                    className="btn icon"
                    onClick={(e) => {
                      e.stopPropagation();
                      move(r.id, 1);
                    }}
                    disabled={idx === rows.length - 1 || isRunning}
                    title="아래로"
                  >
                    ▼
                  </button>
                  <button
                    className="btn icon danger"
                    onClick={(e) => {
                      e.stopPropagation();
                      remove(r.id);
                    }}
                    disabled={rows.length === 1 || isRunning}
                    title="제거"
                  >
                    ✕
                  </button>
                </div>
              </div>

              <div className="tx-grid">
                <label
                  className="tx-grid-wide"
                  title="이 TX를 어느 지갑에서 보낼지. 등록된 지갑 중 선택하거나, 임의 주소 직접 입력."
                >
                  <span>from (wallet)</span>
                  {/* Free-text input + datalist. The dropdown options
                      surface the user's registered wallets, but any
                      0x-address can be typed in directly — the engine's
                      principal entity is synthesized from this string
                      regardless of registration status. */}
                  <input
                    type="text"
                    list={`from-wallets-${r.id}`}
                    value={r.fromWallet}
                    onChange={(e) =>
                      updateRow(r.id, {
                        fromWallet: e.target.value.toLowerCase(),
                      })
                    }
                    onClick={(e) => e.stopPropagation()}
                    disabled={isRunning}
                    placeholder="0x… (등록 지갑 또는 임의 주소)"
                    autoComplete="off"
                    spellCheck={false}
                  />
                  <datalist id={`from-wallets-${r.id}`}>
                    {availableWallets.map((addr) => (
                      <option key={addr} value={addr}>
                        {shortAddrForOption(addr)} (등록)
                      </option>
                    ))}
                  </datalist>
                </label>
                <label title="컨트랙트 주소 (0x + 40 hex). USDC/USDT/WETH 메인넷 주소는 v3 번들에 매칭됨.">
                  <span>to (contract)</span>
                  <input
                    type="text"
                    value={r.to}
                    onChange={(e) => updateRow(r.id, { to: e.target.value })}
                    onClick={(e) => e.stopPropagation()}
                    disabled={isRunning}
                    placeholder="0xa0B8…eB48 (USDC)"
                  />
                </label>
                <label title="msg.value (wei 10진수). ETH 함께 보낼 때만 0 아닌 값.">
                  <span>value (wei)</span>
                  <input
                    type="text"
                    value={r.value}
                    onChange={(e) => updateRow(r.id, { value: e.target.value })}
                    onClick={(e) => e.stopPropagation()}
                    disabled={isRunning}
                    placeholder="0"
                  />
                </label>
                <label
                  className="ctx"
                  title="0xSELECTOR + ABI 인코딩 인자. 위 드롭다운에서 예시 골라 시작하는 것 추천."
                >
                  <span>calldata</span>
                  <textarea
                    value={r.calldata}
                    onChange={(e) =>
                      updateRow(r.id, { calldata: e.target.value })
                    }
                    onClick={(e) => e.stopPropagation()}
                    disabled={isRunning}
                    placeholder="0xa9059cbb… (transfer) 또는 0x095ea7b3… (approve)"
                    rows={3}
                  />
                </label>
              </div>

              {/* Engine-error banner. Surfaces issues like "this wallet
                  doesn't track USDT" so the user gets actionable guidance
                  instead of a raw `apply_failed: token not found` from
                  the WASM bridge. Verdict pills are hidden in this state
                  (verdict is stale when the step never ran). */}
              {error && (
                <div className="tx-card-error">
                  <span className="tx-card-error-icon">⚠</span>
                  <span className="tx-card-error-msg">{error}</span>
                </div>
              )}

              {/* Verdict + matched-policies summary. We render the matched
                  list inline (instead of a separate detail panel) since the
                  WASM simulator's verdict shape is per-row and self-contained.
                  The right-rail PolicyPanel can still flash these on click. */}
              {!error && verdict && verdict.kind !== "pass" && (
                <div className="tx-card-foot">
                  {verdict.matched.map((m) => (
                    <span
                      key={m.policy_id}
                      className={`block-tag sev-${m.severity}`}
                      title={m.reason ?? undefined}
                    >
                      {m.severity === "deny" ? "⛔" : "⚠"} {m.policy_id}
                    </span>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── helpers ───────────────────────────────────────────────────────────────

function newRowId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function blankCalldataRow(
  idx: number,
  defaultFromWallet: string = "",
): CalldataTxRow {
  return {
    id: newRowId(),
    label: `TX ${idx + 1}`,
    fromWallet: defaultFromWallet,
    to: "",
    calldata: "0x",
    value: "0",
  };
}

/** Same compact-address render the wallet selector uses, inlined here so
 *  the dropdown options match visually. */
function shortAddrForOption(addr: string): string {
  if (!addr || addr.length < 10) return addr;
  return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
}

/** Curated calldata presets surfacing real txs against the shipped v3
 *  bundles (`browser-extension/public/default-v3-bundles/bundles-v1.json`).
 *  Each entry is a complete, paste-ready row — the user picks one from the
 *  dropdown and gets a working tx without having to hand-encode anything.
 *
 *  `value` is wei as a decimal string; `to`/`calldata` are 0x-hex. Token
 *  addresses are the canonical mainnet contracts the shipped bundles match
 *  on (USDC, USDT, WETH). Recipient `0xbeef…` is a deliberately fake
 *  address so the simulator's "recipient not self" policies trigger
 *  predictably for demos. */
const CALLDATA_PRESETS: ReadonlyArray<{
  id: string;
  label: string;
  to: string;
  calldata: string;
  value: string;
}> = [
  {
    id: "usdc-transfer-1",
    label: "USDC transfer 1.0 USDC → 0xbeef…",
    to: "0xa0B86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
    calldata:
      "0xa9059cbb000000000000000000000000000000000000000000000000000000000000beef00000000000000000000000000000000000000000000000000000000000f4240",
    value: "0",
  },
  {
    id: "usdc-approve-uniswap-max",
    label: "USDC approve unlimited → 0xdead…",
    to: "0xa0B86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
    calldata:
      "0x095ea7b3000000000000000000000000000000000000000000000000000000000000deadffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    value: "0",
  },
  {
    id: "usdt-transfer-100",
    label: "USDT transfer 100 USDT → 0xbeef…",
    to: "0xdAC17F958D2ee523a2206206994597C13D831ec7",
    calldata:
      "0xa9059cbb000000000000000000000000000000000000000000000000000000000000beef0000000000000000000000000000000000000000000000000000000005f5e100",
    value: "0",
  },
  {
    id: "weth-approve-1eth",
    label: "WETH approve 1.0 WETH → 0xdead…",
    to: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
    calldata:
      "0x095ea7b3000000000000000000000000000000000000000000000000000000000000dead0000000000000000000000000000000000000000000000000de0b6b3a7640000",
    value: "0",
  },
];
