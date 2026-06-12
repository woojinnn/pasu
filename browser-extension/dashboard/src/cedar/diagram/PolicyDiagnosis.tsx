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
import { useTranslation } from "react-i18next";

import type { PolicyIR } from "../blocks/ir";
import {
  runDiagnosisProbes,
  type DiagnosisRequestDto,
} from "../../server-api/diagnosis";
import { SAMPLE_ACTIONS } from "../../editor-v9/sample-actions";
import { buildProbes, diagnoseFromResult } from "../diagnosis";
import { enumeratePaths } from "../diagnosis/path";
import { useAddressBook, shortAddress } from "../../hooks/useAddressBook";
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
  const { t } = useTranslation("diagnosis");
  const [diag, setDiag] = useState<{
    culprits: string[];
    errored: string[];
    /** 진단이 평가한 materialize된 context — 다이어그램의 실제 값 표시용. */
    context?: unknown;
  } | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Resolve 0x addresses in diagram labels to friendly names (my wallet / token).
  const book = useAddressBook();
  const humanizeLabel = (text: string): string =>
    text.replace(/0x[0-9a-fA-F]{40}/g, (m) => {
      const e = book.lookup(m);
      return e ? `${e.name}(${shortAddress(m)})` : m;
    });

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
        setMsg(t("forbidOnly"));
        return;
      }
      const { probes, diagnosable } = buildProbes(ir);
      if (!diagnosable) {
        setMsg(t("notDiagnosable"));
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
          setMsg(t("noSample", { action: actionId ?? t("unspecified") }));
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
      // has-가드(필드 존재 확인)는 다이어그램이 그리지 않는 스캐폴딩 —
      // 표시 개수/하이라이트에서 빼야 메시지의 N과 빨간 박스 수가 일치한다.
      const byPath = new Map(enumeratePaths(ir).map(({ path, node }) => [path, node]));
      const visible = (p: string) => byPath.get(p)?.kind !== "has";
      const culprits = d.culprits.filter(visible);
      const errored = d.errored.filter(visible);
      setDiag({ culprits, errored, context: result.context });
      if (culprits.length > 0) {
        setMsg(t("culpritsShown", { n: culprits.length }));
      } else if (d.culprits.length > 0) {
        // 가드만 발화한 비정형 케이스 — 박스 없이 N개라고 말하지 않는다.
        setMsg(t("guardOnly"));
      } else if (errored.length > 0) {
        // Every probe touching an absent field errored — typical when an
        // enrichment (context.custom.*) policy is run on a SAMPLE without those
        // results. Not "passed"; just unevaluable here.
        setMsg(t("unevaluable", { n: errored.length }));
      } else {
        setMsg(request ? t("noBlockReal") : t("noBlockSample"));
      }
    } catch (e) {
      setMsg(t("failed", { message: e instanceof Error ? e.message : String(e) }));
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
          title={t("runTitle")}
        >
          {busy ? t("running") : t("run")}
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
            {t("clear")}
          </button>
        )}
      </div>
      <PolicyDiagram
        ir={ir}
        highlightPaths={diag?.culprits}
        erroredPaths={diag?.errored}
        compact={compact}
        humanizeLabel={humanizeLabel}
        actualContext={diag?.context}
      />
    </div>
  );
}
