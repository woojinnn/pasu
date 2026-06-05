/**
 * End-to-end smoke probe for the full `tx queue → decode → simulate` pipeline.
 *
 * Sister to `WasmStepProbe`: that one ships a pinned Action and proves the
 * `simulate_step_json` round-trip. This one is the real user-flow shape —
 * a queue of raw EVM txs (each `{to, calldata, value}` row), threaded through:
 *   (1) `decodeCalldataLocal` → `declarative_route_request_v3_json`
 *       → typed `Action[]` per row,
 *   (2) for each decoded `Action`, `simulateStepLocal` threading
 *       `next_state` forward across the whole queue (NOT just within a row),
 *   (3) per-row + per-step `{ delta, post_state }` rendered as a flat list.
 *
 * Base state comes from either the pinned ERC20 fixture or
 * `GET /wallets/:addr/state` — toggled at the top of the panel.
 *
 * Failure surfaces:
 *   - decode timeout / `sim_decode_failed` — calldata didn't match any
 *     installed manifest, or wire payload malformed.
 *   - per-step `sim_step_failed` — the reducer rejected a decoded Action
 *     against the running temp state.
 * On error, every row's previously-successful steps are still rendered so
 * the user can see how far the queue got before the offending tx.
 */

import { useState } from "react";

import { getWalletState } from "../../server-api";
import {
  decodeCalldataLocal,
  isMissingBridge,
  simulateStepLocal,
  type DecodeCalldataInput,
  type OpaqueAction,
  type OpaqueStateDelta,
  type OpaqueWalletState,
} from "./sim-bridge";
import { SAMPLE_ERC20_TRANSFER_PROBE } from "./wasm-probe-fixture";

// ── base state ────────────────────────────────────────────────────────────

type StateSource =
  | { kind: "fixture" }
  | { kind: "backend"; address: string; loadedAt: number };

type StateLoadStatus =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "err"; message: string };

// ── tx row + steps ────────────────────────────────────────────────────────

interface TxRow {
  id: string;
  to: string;
  calldata: string;
  value: string;
}

interface StepRecord {
  rowIndex: number;
  /** Order within the row's decoded `Action[]`. */
  withinRowIndex: number;
  action: OpaqueAction;
  delta: OpaqueStateDelta;
  post_state: OpaqueWalletState;
}

type ProbeStatus =
  | { kind: "idle" }
  | { kind: "running"; rowIndex: number; phase: "decode" | "simulate" }
  | { kind: "ok"; steps: StepRecord[]; elapsedMs: number }
  | {
      kind: "err";
      message: string;
      bridgeMissing: boolean;
      stepsBeforeError: StepRecord[];
    };

const FROM_DEFAULT = "0x000000000000000000000000000000000000a01c";
const CHAIN_DEFAULT = 1;

function newRowId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function blankRow(): TxRow {
  return { id: newRowId(), to: "", calldata: "0x", value: "0" };
}

function selectorOf(calldata: string): string {
  const cd = calldata.trim();
  if (cd.startsWith("0x") && cd.length >= 10) {
    return cd.slice(0, 10).toLowerCase();
  }
  return "0x00000000";
}

// ── component ─────────────────────────────────────────────────────────────

export function CalldataProbe() {
  const [open, setOpen] = useState(false);

  // Base state — fixture or backend-loaded.
  const [baseState, setBaseState] = useState<OpaqueWalletState>(
    SAMPLE_ERC20_TRANSFER_PROBE.state,
  );
  const [stateSource, setStateSource] = useState<StateSource>({
    kind: "fixture",
  });
  const [stateAddress, setStateAddress] = useState<string>(FROM_DEFAULT);
  const [stateLoad, setStateLoad] = useState<StateLoadStatus>({ kind: "idle" });

  // Routing fields shared by every row in the queue.
  const [chainId, setChainId] = useState<number>(CHAIN_DEFAULT);
  const [submitter, setSubmitter] = useState<string>(FROM_DEFAULT);

  // The tx queue itself — start with one blank row.
  const [rows, setRows] = useState<TxRow[]>([blankRow()]);
  const [status, setStatus] = useState<ProbeStatus>({ kind: "idle" });

  // ── base state actions ─────────────────────────────────────────────────
  const loadBackendState = async () => {
    setStateLoad({ kind: "loading" });
    try {
      const live = (await getWalletState(
        stateAddress.trim(),
      )) as unknown as OpaqueWalletState;
      setBaseState(live);
      setStateSource({
        kind: "backend",
        address: stateAddress.trim(),
        loadedAt: Date.now(),
      });
      setStateLoad({ kind: "idle" });
    } catch (err) {
      setStateLoad({
        kind: "err",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  };

  const resetToFixture = () => {
    setBaseState(SAMPLE_ERC20_TRANSFER_PROBE.state);
    setStateSource({ kind: "fixture" });
    setStateLoad({ kind: "idle" });
  };

  // ── row mutators ────────────────────────────────────────────────────────
  const addRow = () => setRows((r) => [...r, blankRow()]);
  const removeRow = (id: string) =>
    setRows((r) => (r.length <= 1 ? r : r.filter((x) => x.id !== id)));
  const updateRow = (id: string, patch: Partial<TxRow>) =>
    setRows((r) =>
      r.map((x) => {
        if (x.id !== id) return x;
        const next = { ...x, ...patch };
        if (patch.calldata !== undefined) next.calldata = patch.calldata.trim();
        return next;
      }),
    );

  // ── run the queue ───────────────────────────────────────────────────────
  const run = async () => {
    const t0 = performance.now();
    const submittedAt = Math.floor(Date.now() / 1000);
    const allSteps: StepRecord[] = [];
    let state = baseState;
    let nonce = 0;

    for (let r = 0; r < rows.length; r++) {
      const row = rows[r]!;
      setStatus({ kind: "running", rowIndex: r, phase: "decode" });
      try {
        const decodeInput: DecodeCalldataInput = {
          chain_id: chainId,
          to: row.to.trim(),
          selector: selectorOf(row.calldata),
          calldata: row.calldata.trim(),
          value: row.value.trim() || "0",
          submitter: submitter.trim(),
          submitted_at: submittedAt + r,
          nonce: nonce++,
        };
        const decoded = await decodeCalldataLocal(decodeInput);

        setStatus({ kind: "running", rowIndex: r, phase: "simulate" });
        for (let a = 0; a < decoded.actions.length; a++) {
          const action = decoded.actions[a]!;
          const out = await simulateStepLocal({
            state,
            action,
            ctx: SAMPLE_ERC20_TRANSFER_PROBE.ctx,
          });
          allSteps.push({
            rowIndex: r,
            withinRowIndex: a,
            action,
            delta: out.delta,
            post_state: out.next_state,
          });
          state = out.next_state;
        }
      } catch (err) {
        setStatus({
          kind: "err",
          message: err instanceof Error ? err.message : String(err),
          bridgeMissing: isMissingBridge(err),
          stepsBeforeError: allSteps,
        });
        return;
      }
    }

    setStatus({
      kind: "ok",
      steps: allSteps,
      elapsedMs: performance.now() - t0,
    });
  };

  // ── render ──────────────────────────────────────────────────────────────
  const isRunning = status.kind === "running";

  return (
    <div className="sim-wasm-probe">
      <button
        type="button"
        className="probe-toggle"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        {open ? "▼" : "▶"} Tx queue → decode → simulate probe (debug)
      </button>
      {open && (
        <div className="probe-body">
          <p className="probe-help">
            여러 tx를 큐로 넣고 순차 시뮬레이션. 각 row →{" "}
            <code>declarative_route_request_v3_json</code>로 디코드 → 나온{" "}
            <code>Action[]</code>을 차례대로{" "}
            <code>simulate_step_json</code>에 넣고{" "}
            <code>next_state</code>를 다음 호출의 state로 던집니다. Base
            state는 fixture 또는 backend{" "}
            <code>GET /wallets/:addr/state</code> 중 택1.
          </p>

          <div className="probe-base-state">
            <span className="probe-base-label">Base state:</span>
            <input
              type="text"
              placeholder="0x… wallet address"
              value={stateAddress}
              onChange={(e) => setStateAddress(e.target.value)}
              className="probe-base-input"
            />
            <button
              type="button"
              className="btn"
              onClick={loadBackendState}
              disabled={
                stateLoad.kind === "loading" || stateAddress.trim() === ""
              }
            >
              {stateLoad.kind === "loading" ? "fetching…" : "load from backend"}
            </button>
            <button
              type="button"
              className="btn ghost"
              onClick={resetToFixture}
              disabled={stateSource.kind === "fixture"}
            >
              reset to fixture
            </button>
            <span className="probe-meta">
              {stateSource.kind === "backend" ? (
                <>
                  source:{" "}
                  <code>
                    backend({stateSource.address.slice(0, 10)}…)
                  </code>
                </>
              ) : (
                <>
                  source: <code>fixture</code>
                </>
              )}
            </span>
            {stateLoad.kind === "err" && (
              <span className="probe-meta err">✗ {stateLoad.message}</span>
            )}
          </div>

          <div className="probe-grid">
            <label>
              chain_id
              <input
                type="number"
                value={chainId}
                onChange={(e) =>
                  setChainId(Number(e.target.value) || CHAIN_DEFAULT)
                }
              />
            </label>
            <label>
              from (submitter, applies to every row)
              <input
                type="text"
                value={submitter}
                onChange={(e) => setSubmitter(e.target.value)}
              />
            </label>
          </div>

          <div className="probe-rows">
            {rows.map((row, idx) => (
              <div className="probe-row" key={row.id}>
                <div className="probe-row-head">
                  <span className="probe-row-tag">tx {idx + 1}</span>
                  <button
                    type="button"
                    className="btn ghost"
                    onClick={() => removeRow(row.id)}
                    disabled={rows.length <= 1 || isRunning}
                  >
                    remove
                  </button>
                </div>
                <div className="probe-grid">
                  <label>
                    to
                    <input
                      type="text"
                      value={row.to}
                      onChange={(e) =>
                        updateRow(row.id, { to: e.target.value })
                      }
                    />
                  </label>
                  <label>
                    value (wei, decimal)
                    <input
                      type="text"
                      value={row.value}
                      onChange={(e) =>
                        updateRow(row.id, { value: e.target.value })
                      }
                    />
                  </label>
                  <label className="probe-wide">
                    calldata (0x…)
                    <textarea
                      rows={2}
                      value={row.calldata}
                      onChange={(e) =>
                        updateRow(row.id, { calldata: e.target.value })
                      }
                    />
                    <span className="probe-meta">
                      selector: <code>{selectorOf(row.calldata)}</code>
                    </span>
                  </label>
                </div>
              </div>
            ))}
            <button
              type="button"
              className="btn ghost"
              onClick={addRow}
              disabled={isRunning}
            >
              + add tx
            </button>
          </div>

          <div className="probe-controls">
            <button
              type="button"
              className="btn"
              onClick={run}
              disabled={isRunning}
            >
              {isRunning
                ? `실행 중 (row ${status.rowIndex + 1}/${rows.length}, ${status.phase})…`
                : `decode + simulate (${rows.length} tx)`}
            </button>
            {status.kind === "ok" && (
              <span className="probe-meta ok">
                ✓ {status.steps.length} step
                {status.steps.length === 1 ? "" : "s"} ·{" "}
                {status.elapsedMs.toFixed(1)} ms
              </span>
            )}
            {status.kind === "err" && (
              <span className="probe-meta err">
                ✗ {status.bridgeMissing ? "bridge 없음 — " : ""}
                {status.message}
              </span>
            )}
          </div>

          {status.kind === "ok" && (
            <pre className="probe-output">
              {JSON.stringify({ steps: status.steps }, null, 2)}
            </pre>
          )}
          {status.kind === "err" && status.stepsBeforeError.length > 0 && (
            <>
              <p className="probe-help">
                에러 이전까지 {status.stepsBeforeError.length} step 진행됨:
              </p>
              <pre className="probe-output">
                {JSON.stringify(
                  { steps_before_error: status.stepsBeforeError },
                  null,
                  2,
                )}
              </pre>
            </>
          )}
        </div>
      )}
    </div>
  );
}
