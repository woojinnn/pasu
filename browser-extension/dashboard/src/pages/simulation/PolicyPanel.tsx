/**
 * Right rail: simulation-only policy toggles + violation detail.
 *
 * The toggles here mutate the page-local `enabledIds` Set only — the
 * real `enabled` flag in extension storage is untouched, so a user can
 * trial "what if I turn this rule off?" without affecting their live
 * gate. Mention this to the user via the helper line under the title.
 */
import type { InstalledPolicy, PolicySeverity } from "../../server-api";
import type { PolicyOutcome, SequenceStepResult } from "../../cedar";

export interface PolicyPanelProps {
  policies: InstalledPolicy[];
  enabledIds: Set<number>;
  toggle: (id: number) => void;
  enableAll: () => void;
  disableAll: () => void;

  /** verdict result for the currently-selected step (timeline cursor) */
  currentStep: SequenceStepResult | undefined;
  /** which step is the cursor on (0 = S₀ pre-run, 1 = after TX1, …) */
  cursorIdx: number;
  /** currently-focused violation, drives the highlight in state panel */
  focusedViolation: { stepIdx: number; policyId: number } | null;
  setFocusedViolation: (v: { stepIdx: number; policyId: number } | null) => void;
  /** flash a specific policy row when its state ref is clicked from elsewhere */
  flashPolicyId: number | null;
}

export function PolicyPanel(props: PolicyPanelProps) {
  const {
    policies,
    enabledIds,
    toggle,
    enableAll,
    disableAll,
    currentStep,
    cursorIdx,
    focusedViolation,
    setFocusedViolation,
    flashPolicyId,
  } = props;

  const violations = (currentStep?.policy_results ?? []).filter(
    (o) => o.decision === "deny",
  );

  return (
    <div className="sim-side">
      <div className="sim-card violations-card">
        <div className="card-head">
          <h3>판정 · 위반</h3>
          {currentStep && (
            <span className={`vpill sm ${currentStep.verdict}`}>
              {currentStep.verdict.toUpperCase()}
            </span>
          )}
        </div>
        {!currentStep && (
          <div className="muted-line">
            시뮬레이션을 실행하면 각 TX 시점의 판정이 표시됩니다.
          </div>
        )}
        {currentStep && cursorIdx === 0 && (
          <div className="muted-line">S₀ (초기 상태) — 평가된 TX 없음</div>
        )}
        {currentStep && violations.length === 0 && cursorIdx > 0 && (
          <div className="muted-line">이 TX에서는 차단된 정책이 없습니다.</div>
        )}
        {currentStep && violations.length > 0 && (
          <ul className="violation-list">
            {violations.map((v) => {
              const focused = focusedViolation
                && focusedViolation.policyId === v.policy_id
                && focusedViolation.stepIdx === cursorIdx;
              return (
                <li
                  key={v.policy_id}
                  className={`vline sev-${v.severity} ${focused ? "is-focused" : ""}`}
                  onClick={() =>
                    setFocusedViolation(
                      focused ? null : { stepIdx: cursorIdx, policyId: v.policy_id },
                    )
                  }
                >
                  <div className="vline-head">
                    <span className={`sev-tag ${v.severity}`}>
                      {labelForSeverity(v.severity)}
                    </span>
                    <span className="pname">{v.policy_name}</span>
                  </div>
                  {v.matched && v.matched.length > 0 && (
                    <div className="vline-matched">
                      {v.matched.map((m, i) => (
                        <code key={i}>{m}</code>
                      ))}
                    </div>
                  )}
                  <div className="vline-hint">
                    {focused ? "클릭 해제" : "클릭 → 영향 state 강조"}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <div className="sim-card policy-card">
        <div className="card-head">
          <h3>정책</h3>
          <span className="meta">시뮬레이션 전용 on/off</span>
        </div>
        <div className="policy-toolbar">
          <span className="meta">
            {enabledIds.size} / {policies.length} 활성
          </span>
          <button className="btn xs" onClick={enableAll}>전체 ON</button>
          <button className="btn xs" onClick={disableAll}>전체 OFF</button>
        </div>

        {policies.length === 0 && (
          <div className="muted-line">설치된 정책이 없습니다.</div>
        )}
        <ul className="policy-list">
          {policies.map((p) => {
            const on = enabledIds.has(p.id);
            const outcome = currentStep?.policy_results.find(
              (o) => o.policy_id === p.id,
            );
            const flash = flashPolicyId === p.id;
            return (
              <li
                key={p.id}
                className={`policy-row sev-${p.severity} ${flash ? "is-flash" : ""}`}
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
                    <span className="pname">{p.name}</span>
                    <span className={`sev-tag ${p.severity}`}>{p.severity}</span>
                  </div>
                  {outcome && on && (
                    <div className="policy-outcome">
                      <span className={`out-pill ${outcome.decision}`}>
                        {outcome.decision === "deny" ? "DENY" : "PASS"}
                      </span>
                      {outcome.matched && outcome.matched.length > 0 && (
                        <span className="matched-line">
                          {outcome.matched.join(", ")}
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
    </div>
  );
}

// ── helpers ───────────────────────────────────────────────────────────────

function labelForSeverity(s: PolicySeverity): string {
  switch (s) {
    case "deny": return "FORBID";
    case "warn": return "REVIEW";
    case "info": return "INFO";
    default: return s;
  }
}

// avoid unused-imports lint without affecting bundle
export type { PolicyOutcome };
