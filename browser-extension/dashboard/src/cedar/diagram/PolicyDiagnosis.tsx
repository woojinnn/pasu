/**
 * PolicyDiagnosis — a {@link PolicyDiagram} plus an on-demand "진단" runner that
 * red-traces the clause(s) that would block a transaction.
 *
 * Shared by every surface that wants "show the policy + where it's blocked":
 * the editor's 다이어그램 tab and the simulation verdict panel both render this,
 * differing only in `compact` and where their `ir` comes from. The `ir` passed
 * in MUST be the same object the diagnosis is run against (object identity is
 * load-bearing — see cedar/diagnosis/README §4), which this component guarantees
 * by building probes from the very `ir` prop it renders.
 */
import { useEffect, useRef, useState } from "react";

import type { PolicyIR } from "../blocks/ir";
import {
  runDiagnosisProbes,
  type DiagnosisRequestDto,
} from "../../server-api/diagnosis";
import { SAMPLE_ACTIONS } from "../../editor-v9/sample-actions";
import { buildProbes, diagnoseFromResult } from "../diagnosis";
import { PolicyDiagram } from "./PolicyDiagram";

/** The diagnosis context minus the probes (the caller's policy supplies those). */
export type DiagnosisContext = Omit<DiagnosisRequestDto, "probes">;

export interface PolicyDiagnosisProps {
  ir: PolicyIR | null;
  /** Tighter layout for cramped surfaces (verdict panel, popup). */
  compact?: boolean;
  /** Real diagnosis context (action + materialized enrichment results) to run
   *  against — e.g. a live deny's captured context. When omitted, the on-demand
   *  run uses a built-in SAMPLE action keyed by the policy's action id. */
  request?: DiagnosisContext;
  /** Run the diagnosis automatically on mount (used when `request` is the real
   *  context of an already-blocked tx, so the culprit shows without a click). */
  autoRun?: boolean;
}

export function PolicyDiagnosis({
  ir,
  compact,
  request,
  autoRun,
}: PolicyDiagnosisProps) {
  const [diag, setDiag] = useState<{ culprits: string[]; errored: string[] } | null>(
    null,
  );
  const [msg, setMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // A diagnosis is bound to one policy; drop it when the policy changes.
  const autoRanFor = useRef<PolicyIR | null>(null);
  useEffect(() => {
    setDiag(null);
    setMsg(null);
    autoRanFor.current = null;
  }, [ir]);

  const run = async () => {
    if (!ir) return;
    setBusy(true);
    setMsg(null);
    setDiag(null);
    try {
      if (ir.effect !== "forbid") {
        setMsg("forbid(차단) 정책만 진단할 수 있어요");
        return;
      }
      const { probes, diagnosable } = buildProbes(ir);
      if (!diagnosable) {
        setMsg("hole/raw 블록이 있어 진단할 수 없어요");
        return;
      }
      // Real captured context when provided (the live deny); else a SAMPLE
      // action keyed by the policy's action id.
      let base: DiagnosisContext;
      if (request) {
        base = request;
      } else {
        const a = ir.scope.action;
        const actionId = a.kind === "scopeEq" ? a.entity.id : undefined;
        const sample = actionId ? SAMPLE_ACTIONS[actionId] : undefined;
        if (!sample) {
          setMsg(`이 액션(${actionId ?? "미지정"})의 샘플이 없어 진단할 수 없어요`);
          return;
        }
        base = sample();
      }
      const result = await runDiagnosisProbes({ ...base, probes });
      const d = diagnoseFromResult(
        ir,
        probes.map((p) => p.id),
        result,
      );
      setDiag({ culprits: d.culprits, errored: d.errored });
      setMsg(
        d.culprits.length > 0
          ? `차단 조건 ${d.culprits.length}개를 빨갛게 표시했어요`
          : request
            ? "이 거래에선 차단 조건이 없었어요"
            : "이 샘플 거래는 차단되지 않았어요 (빨간 조건 없음)",
      );
    } catch (e) {
      setMsg(`진단 실패: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  // Auto-run once for a real captured context (e.g. a history deny), so the
  // culprit shows on expand without a click.
  useEffect(() => {
    if (autoRun && request && ir && autoRanFor.current !== ir) {
      autoRanFor.current = ir;
      void run();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoRun, request, ir]);

  return (
    <div className="pdiagnosis">
      <div className="pdiagnosis-bar">
        <button
          type="button"
          className="pdiagnosis-run"
          onClick={run}
          disabled={busy || !ir}
          title="샘플 거래로 어느 조건이 차단하는지 진단합니다"
        >
          {busy ? "진단 중…" : "진단 실행"}
        </button>
        {msg && <span className="pdiagnosis-msg">{msg}</span>}
        {diag && (
          <button
            type="button"
            className="pdiagnosis-clear"
            onClick={() => {
              setDiag(null);
              setMsg(null);
            }}
          >
            지우기
          </button>
        )}
      </div>
      <PolicyDiagram
        ir={ir}
        highlightPaths={diag?.culprits}
        erroredPaths={diag?.errored}
        compact={compact}
      />
    </div>
  );
}
