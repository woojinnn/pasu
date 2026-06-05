/**
 * Policy on/off panel for the integrated simulator.
 *
 * Pulled out of the prior `WasmPolicyPanel`; the verdict-only slice now
 * lives in `VerdictPanel`. This panel owns the page-local `enabledIds`
 * mutation surface — toggles only affect the simulation, never the live
 * extension-storage `enabled` flag. The current verdict's `matched`
 * policies are highlighted inline so the user can see which rows triggered
 * at the cursor.
 */

import type { ManagedPolicy } from "../../server-api";
import type { EvaluateActionVerdict, MatchedPolicy } from "./sim-bridge";

export interface PolicyTogglePanelProps {
  policies: ReadonlyArray<ManagedPolicy>;
  /** Page-local on/off set (sim-only). Toggles do NOT touch storage. */
  enabledIds: Set<string>;
  toggle: (id: string) => void;
  enableAll: () => void;
  disableAll: () => void;
  /** Verdict at the current cursor. `matched.policy_id` rows light up
   *  inline so the toggle list doubles as a "blame" surface. */
  currentVerdict: EvaluateActionVerdict | undefined;
  /** A transient flash highlight requested from a sibling panel (e.g.
   *  clicking a token row to find policies that read it). `null` when no
   *  flash is active. */
  flashPolicyId: string | null;
}

export function PolicyTogglePanel(props: PolicyTogglePanelProps) {
  const {
    policies,
    enabledIds,
    toggle,
    enableAll,
    disableAll,
    currentVerdict,
    flashPolicyId,
  } = props;

  const matched: ReadonlyArray<MatchedPolicy> =
    currentVerdict && currentVerdict.kind !== "pass"
      ? currentVerdict.matched
      : [];

  return (
    <div className="sim-card policy-card">
      <div className="card-head">
        <h3>정책 선택</h3>
        <span className="meta">시뮬레이션 전용 on/off</span>
      </div>
      <div className="policy-toolbar">
        <span className="meta">
          {enabledIds.size} / {policies.length} 활성
        </span>
        <button className="btn xs" onClick={enableAll}>
          전체 ON
        </button>
        <button className="btn xs" onClick={disableAll}>
          전체 OFF
        </button>
      </div>

      {policies.length === 0 && (
        <div className="muted-line">설치된 정책이 없습니다.</div>
      )}
      <ul className="policy-list">
        {policies.map((p) => {
          const on = enabledIds.has(p.id);
          const involvedHere = matched.find((m) => m.policy_id === p.id);
          const flash = flashPolicyId === p.id;
          return (
            <li
              key={p.id}
              className={`policy-row ${flash ? "is-flash" : ""}`}
            >
              <label className="sw">
                <input
                  type="checkbox"
                  checked={on}
                  onChange={() => toggle(p.id)}
                />
                <span className="sw-slider" />
              </label>
              <div className="policy-body">
                <div className="policy-row-head">
                  <span className="pname">{p.displayName ?? p.id}</span>
                </div>
                {involvedHere && on && (
                  <div className="policy-outcome">
                    <span
                      className={`out-pill ${involvedHere.severity === "deny" ? "deny" : "warn"}`}
                    >
                      {involvedHere.severity === "deny" ? "DENY" : "WARN"}
                    </span>
                    {involvedHere.reason && (
                      <span className="matched-line">
                        {involvedHere.reason}
                      </span>
                    )}
                  </div>
                )}
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
