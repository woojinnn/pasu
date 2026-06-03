/**
 * End-to-end smoke probe for the dashboard → SW → wasm `simulate_step_json`
 * round-trip.
 *
 * Sends a hardcoded `(state, action, ctx)` triple (kept in lock-step with the
 * Rust integration test via the `emit_sim_step_sample` example) and renders
 * the raw `{ delta, next_state }` response. Intentionally bypasses the
 * Tx/State/Policy panel UI — those still consume the heuristic mock; this
 * probe proves the real reducer path is wired before the panel shape gets
 * refactored to consume `WalletState` directly.
 *
 * Failure modes surface as inline error messages:
 *   - "extension/bridge unavailable" → content-script not injected (run the
 *     dashboard from `http://127.0.0.1:5173` AND have the extension loaded).
 *   - `apply_failed: …` → reducer rejected the action (e.g. underflow).
 *   - `invalid_input: …` → wire-shape drift; regenerate `sim-step-sample.json`.
 *
 * Collapsible — hidden by default so it doesn't crowd the panel layout.
 */

import { useState } from "react";

import {
  isMissingBridge,
  simulateStepLocal,
  type SimulateStepOutput,
} from "./sim-bridge";
import { SAMPLE_ERC20_TRANSFER_PROBE } from "./wasm-probe-fixture";

type ProbeStatus =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "ok"; data: SimulateStepOutput; elapsedMs: number }
  | { kind: "err"; message: string; bridgeMissing: boolean };

export function WasmStepProbe() {
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<ProbeStatus>({ kind: "idle" });

  const run = async () => {
    setStatus({ kind: "running" });
    const t0 = performance.now();
    try {
      const data = await simulateStepLocal(SAMPLE_ERC20_TRANSFER_PROBE);
      setStatus({ kind: "ok", data, elapsedMs: performance.now() - t0 });
    } catch (err) {
      setStatus({
        kind: "err",
        message: err instanceof Error ? err.message : String(err),
        bridgeMissing: isMissingBridge(err),
      });
    }
  };

  return (
    <div className="sim-wasm-probe">
      <button
        type="button"
        className="probe-toggle"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        {open ? "▼" : "▶"} WASM step probe (debug)
      </button>
      {open && (
        <div className="probe-body">
          <p className="probe-help">
            하드코딩된 USDC transfer 샘플을 SW → wasm{" "}
            <code>simulate_step_json</code> 경로로 실제로 한 번 돌립니다. 패널
            UI는 아직 mock state를 사용 — 이 probe는 reducer 라운드트립이
            살아있는지만 검증합니다.
          </p>
          <div className="probe-controls">
            <button
              type="button"
              className="btn"
              onClick={run}
              disabled={status.kind === "running"}
            >
              {status.kind === "running" ? "실행 중…" : "1 step 실행"}
            </button>
            {status.kind === "ok" && (
              <span className="probe-meta ok">
                ✓ {status.elapsedMs.toFixed(1)} ms
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
              {JSON.stringify(status.data, null, 2)}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}
