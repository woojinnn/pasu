/**
 * Verdict-only panel for the integrated simulator.
 *
 * Pulled out of `WasmPolicyPanel` so the page-level grid can place the
 * verdict box (small, top-right) separately from the policy toggle list
 * (taller, bottom-right). Both still operate on the same simulator
 * outputs — this file owns the `EvaluateActionVerdict` slice; its sibling
 * `PolicyPanel` owns the on/off list.
 */

import type { EvaluateActionVerdict, MatchedPolicy } from "./sim-bridge";

export interface VerdictPanelProps {
  /** Verdict at the currently-selected step. `undefined` when the cursor
   *  is on the initial state (cursor 0) or no run has happened yet. */
  currentVerdict: EvaluateActionVerdict | undefined;
}

export function VerdictPanel(props: VerdictPanelProps) {
  const { currentVerdict } = props;
  const matched: ReadonlyArray<MatchedPolicy> =
    currentVerdict && currentVerdict.kind !== "pass"
      ? currentVerdict.matched
      : [];

  return (
    <div className="sim-card violations-card">
      <div className="card-head">
        <h3>판정</h3>
        {currentVerdict && (
          <span className={`vpill sm ${currentVerdict.kind}`}>
            {currentVerdict.kind.toUpperCase()}
          </span>
        )}
      </div>
      {!currentVerdict && (
        <div className="muted-line">
          시뮬레이션을 실행하면 각 TX 시점의 판정이 표시됩니다.
        </div>
      )}
      {currentVerdict && currentVerdict.kind === "pass" && (
        <div className="muted-line">
          이 step에서 차단/경고된 정책이 없습니다.
        </div>
      )}
      {matched.length > 0 && (
        <ul className="violation-list">
          {matched.map((m, i) => (
            <li
              key={`${m.policy_id}-${i}`}
              className={`vline sev-${m.severity}`}
              title={m.reason ?? undefined}
            >
              <div className="vline-head">
                <span className={`sev-tag ${m.severity}`}>
                  {m.severity === "deny" ? "FORBID" : "REVIEW"}
                </span>
                <span className="pname">
                  <code>{m.policy_id}</code>
                </span>
                {m.origin === "system" && (
                  <span className="meta">system</span>
                )}
              </div>
              {m.reason && <div className="vline-reason">{m.reason}</div>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
