/**
 * Verdict-only panel for the integrated simulator.
 *
 * Pulled out of `WasmPolicyPanel` so the page-level grid can place the
 * verdict box (small, top-right) separately from the policy toggle list
 * (taller, bottom-right). Both still operate on the same simulator
 * outputs — this file owns the `EvaluateActionVerdict` slice; its sibling
 * `PolicyPanel` owns the on/off list.
 */

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { textToBlocks } from "../../cedar";
import { PolicyDiagnosis } from "../../cedar/diagram/PolicyDiagnosis";

import type { EvaluateActionVerdict, MatchedPolicy } from "./sim-bridge";

export interface VerdictPanelProps {
  /** Verdict at the currently-selected step. `undefined` when the cursor
   *  is on the initial state (cursor 0) or no run has happened yet. */
  currentVerdict: EvaluateActionVerdict | undefined;
  /** Cedar `@id` → policy text, so a matched deny can render its structure
   *  diagram + on-demand "where it's blocked" diagnosis. */
  policyTextById?: Record<string, string>;
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
              {m.severity === "deny" && props.policyTextById?.[m.policy_id] && (
                <MatchedDiagnosis cedarText={props.policyTextById[m.policy_id]} />
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** Collapsible structure diagram + on-demand diagnosis for one matched deny.
 *  Parses the policy's Cedar to IR lazily (only when expanded). */
function MatchedDiagnosis({ cedarText }: { cedarText: string }) {
  const [open, setOpen] = useState(false);
  const q = useQuery({
    queryKey: ["verdict-diagram-ir", cedarText],
    queryFn: async () => (await textToBlocks(cedarText))[0] ?? null,
    enabled: open,
    retry: false,
  });
  return (
    <div className="vline-diagram">
      <button
        type="button"
        className="vline-diag-toggle"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
      >
        {open ? "구조 숨기기 ▲" : "구조 보기 ▼"}
      </button>
      {open && (
        <div className="vline-diag-body">
          <PolicyDiagnosis ir={q.data ?? null} compact />
        </div>
      )}
    </div>
  );
}
