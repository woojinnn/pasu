import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";

import {
  getExampleTransactions,
  listPolicies,
  type ExampleTransaction,
  type InstalledPolicy,
} from "../server-api";

import {
  simulateSequenceLocal,
  type SequenceResp,
  type SequenceStepInput,
  type SequenceStepResult,
} from "../cedar";
import { decodeTxLocal } from "../tools/tx-decode";
import { Topbar } from "../shell/Topbar";

import "./simulation.css";

/**
 * Simulation page — author a sequence of Cedar requests, run them all
 * at once via `/simulate/sequence`, see per-step verdicts + overall.
 *
 * For now this is a manual step builder + run; the front/scopeball-v3
 * design's provenance maps + counterfactuals + reducer-state diff are
 * deferred (they need real chain state, which means wiring the reducer
 * + simulation-state path — a bigger lift).
 *
 * Useful test loops it enables today:
 *   1. Load any example tx → derive a Cedar request → see which of
 *      your installed policies fire (allow/deny per policy).
 *   2. Chain N steps to ask "if I do this and then that, am I still
 *      compliant?" — the overall verdict rolls up worst-of-N.
 */
export function SimulationPage() {
  const [steps, setSteps] = useState<SequenceStepInput[]>([blankStep(0)]);
  const [policyFilter, setPolicyFilter] = useState<Set<number>>(new Set());
  const [result, setResult] = useState<SequenceResp | null>(null);
  const [pickedStep, setPickedStep] = useState<number>(0);

  const policiesQ = useQuery({ queryKey: ["policies"], queryFn: listPolicies });
  const examplesQ = useQuery({ queryKey: ["example-transactions"], queryFn: getExampleTransactions });

  const runMut = useMutation({
    mutationFn: () => {
      const all = policiesQ.data ?? [];
      const enabled = all.filter((p) => p.enabled);
      const chosen =
        policyFilter.size === 0 ? enabled : enabled.filter((p) => policyFilter.has(p.id));
      // Map InstalledPolicy → wasm PolicyInput shape (snake_case fields).
      const policies = chosen.map((p) => ({
        policy_id: p.id,
        policy_name: p.name,
        severity: p.severity,
        cedar_text: p.cedar_text,
      }));
      return simulateSequenceLocal(steps, policies);
    },
    onSuccess: (resp) => {
      setResult(resp);
      setPickedStep(0);
    },
  });

  const addStep = () => {
    setSteps((prev) => [...prev, blankStep(prev.length)]);
  };
  const removeStep = (idx: number) => {
    if (steps.length === 1) return;
    setSteps((prev) => prev.filter((_, i) => i !== idx));
    if (result) setResult(null);
  };
  const updateStep = (idx: number, patch: Partial<SequenceStepInput>) => {
    setSteps((prev) => prev.map((s, i) => (i === idx ? { ...s, ...patch } : s)));
  };
  const updateContext = (idx: number, raw: string) => {
    try {
      const parsed = raw.trim() === "" ? {} : (JSON.parse(raw) as Record<string, unknown>);
      updateStep(idx, { context: parsed });
    } catch {
      // Keep last-good context but flag visually via title attr (handled inline).
    }
  };

  const togglePolicy = (id: number) => {
    setPolicyFilter((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const loadExampleAsNewStep = async (ex: ExampleTransaction) => {
    const meta = (ex.meta as Record<string, unknown>) ?? {};
    const action = (ex.action as Record<string, unknown>) ?? {};
    const from = (meta.from as string) ?? "0x0000000000000000000000000000000000000000";
    const to = (meta.to as string) ?? "0x0000000000000000000000000000000000000000";
    const domain = String(action.domain ?? "Generic");
    const kind = String(action.kind ?? "Tx");
    const label = ex.label.ko || ex.label.en;
    // Best-effort: ask /tx/decode for a function-name suffix if there's calldata.
    let envelopeKind = `${cap(domain)}::${cap(kind)}`;
    const data = (meta.data as string) ?? "0x";
    if (data && data !== "0x") {
      try {
        const dec = decodeTxLocal({ chain: meta.chainId as string, to, data });
        if (dec.action_envelope) {
          envelopeKind = `${cap(dec.action_envelope.domain)}::${cap(dec.action_envelope.kind)}`;
        }
      } catch {
        /* keep best-effort envelopeKind */
      }
    }
    const ctx = { ...(ex.context ?? {}), ...((ex.enrichment as Record<string, unknown>) ?? {}) };
    const newStep: SequenceStepInput = {
      label,
      principal: `Wallet::"${from}"`,
      action: `Action::"${envelopeKind}"`,
      resource: `Protocol::"${to}"`,
      entities: [],
      context: ctx,
    };
    setSteps((prev) => [...prev, newStep]);
  };

  const stepCount = steps.length;
  const verdictByIdx = useMemo(() => {
    const map = new Map<number, SequenceStepResult>();
    result?.steps.forEach((r, i) => map.set(i, r));
    return map;
  }, [result]);

  return (
    <>
      <Topbar
        here="Simulation"
        subtitle={`${stepCount} step${stepCount === 1 ? "" : "s"}`}
      />
      <div className="sim-layout">
        {/* Left column — step builder + run trigger area */}
        <div className="sim-main">
          <div className="sim-card">
            <div className="tools-row">
              <h3 style={{ margin: 0, flex: 1 }}>Steps</h3>
              <select
                onChange={(e) => {
                  const ex = examplesQ.data?.find((x) => x.id === e.target.value);
                  if (ex) loadExampleAsNewStep(ex);
                  e.target.value = "";
                }}
                defaultValue=""
              >
                <option value="" disabled>🧪 예시에서 step 추가…</option>
                {examplesQ.data?.map((ex) => (
                  <option key={ex.id} value={ex.id}>{ex.label.ko || ex.label.en}</option>
                ))}
              </select>
            </div>

            <div className="step-list">
              {steps.map((s, idx) => {
                const rs = verdictByIdx.get(idx);
                const verdictClass = rs ? `verdict-${rs.verdict}` : "";
                return (
                  <div className={`step-row ${verdictClass}`} key={idx}>
                    <div className="step-n">{String(idx + 1).padStart(2, "0")}</div>
                    <div className="step-body">
                      <div className="step-label-row">
                        <input
                          type="text"
                          placeholder="step label"
                          value={s.label ?? ""}
                          onChange={(e) => updateStep(idx, { label: e.target.value })}
                        />
                      </div>
                      <div className="step-detail">
                        <input
                          type="text"
                          placeholder='principal — Wallet::"0x…"'
                          value={s.principal}
                          onChange={(e) => updateStep(idx, { principal: e.target.value })}
                        />
                        <input
                          type="text"
                          placeholder='action — Action::"Amm::Swap"'
                          value={s.action}
                          onChange={(e) => updateStep(idx, { action: e.target.value })}
                        />
                        <input
                          type="text"
                          placeholder='resource — Protocol::"0x…"'
                          value={s.resource}
                          onChange={(e) => updateStep(idx, { resource: e.target.value })}
                        />
                      </div>
                      <textarea
                        placeholder='context JSON — { "slippageBp": 30 }'
                        defaultValue={JSON.stringify(s.context ?? {}, null, 2)}
                        onBlur={(e) => updateContext(idx, e.target.value)}
                      />
                    </div>
                    <div className="step-actions">
                      <button
                        className="btn"
                        onClick={() => removeStep(idx)}
                        disabled={stepCount === 1}
                        title="이 step 제거"
                      >
                        ✕
                      </button>
                      {rs && (
                        <span className={`step-verdict-pill ${rs.verdict}`}>
                          {rs.verdict.toUpperCase()}
                        </span>
                      )}
                      {rs && (
                        <button
                          className="btn"
                          onClick={() => setPickedStep(idx)}
                          title="우측 패널에 상세"
                        >
                          상세
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>

            <div className="add-step-row">
              <button className="btn" onClick={addStep}>+ 빈 step 추가</button>
            </div>
          </div>
        </div>

        {/* Right rail — run + policy filter + overall + drilldown */}
        <div className="sim-side">
          <div className="sim-card run-card">
            <h3>실행</h3>
            <div className="run-controls">
              <button
                className="btn primary"
                onClick={() => runMut.mutate()}
                disabled={runMut.isPending || steps.length === 0}
              >
                {runMut.isPending ? "실행 중…" : `시뮬레이션 실행 (${steps.length} step)`}
              </button>
              <div className="meta">
                정책 필터:&nbsp;
                {policyFilter.size === 0 ? "활성화된 전체" : `${policyFilter.size}개 선택`}
              </div>
              {runMut.error && (
                <div className="err-banner">{String(runMut.error)}</div>
              )}
            </div>

            {result && (
              <div className={`overall ${result.overall}`}>
                <div className="v">{result.overall}</div>
                <div className="sub">
                  {result.steps.filter((s) => s.verdict === "pass").length} pass /&nbsp;
                  {result.steps.filter((s) => s.verdict === "warn").length} warn /&nbsp;
                  {result.steps.filter((s) => s.verdict === "fail").length} fail
                </div>
              </div>
            )}
          </div>

          <PolicyFilter
            policies={policiesQ.data ?? []}
            picked={policyFilter}
            toggle={togglePolicy}
          />

          {result && (
            <StepDetail
              idx={pickedStep}
              total={result.steps.length}
              step={result.steps[pickedStep]}
              setIdx={setPickedStep}
            />
          )}
        </div>
      </div>
    </>
  );
}

// ── helpers / sub-components ────────────────────────────────────────────

function blankStep(idx: number): SequenceStepInput {
  return {
    label: `step ${idx + 1}`,
    principal: 'Wallet::"0x0000000000000000000000000000000000000000"',
    action: 'Action::"Amm::Swap"',
    resource: 'Protocol::"0x0000000000000000000000000000000000000000"',
    entities: [],
    context: {},
  };
}

function cap(s: string): string {
  return s.length > 0 ? s[0].toUpperCase() + s.slice(1) : s;
}

function PolicyFilter({
  policies,
  picked,
  toggle,
}: {
  policies: InstalledPolicy[];
  picked: Set<number>;
  toggle: (id: number) => void;
}) {
  return (
    <div className="sim-card">
      <h3>정책 필터</h3>
      <div className="meta" style={{ fontSize: 12, color: "var(--slate-500)", marginBottom: 8 }}>
        체크 안 하면 활성화된 모든 정책이 평가됩니다.
      </div>
      {policies.length === 0 && (
        <div style={{ fontSize: 12, color: "var(--slate-400)" }}>
          설치된 정책이 없습니다. Editor에서 먼저 만드세요.
        </div>
      )}
      {policies.map((p) => (
        <label
          key={p.id}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            fontSize: 12.5,
            padding: "4px 0",
            color: "var(--slate-700)",
          }}
        >
          <input
            type="checkbox"
            checked={picked.has(p.id)}
            onChange={() => toggle(p.id)}
          />
          {p.name}{" "}
          <span
            style={{
              fontFamily: "var(--ff-mono)",
              fontSize: 10,
              padding: "1px 6px",
              borderRadius: 999,
              background: p.severity === "deny" ? "var(--fail-100)" : p.severity === "warn" ? "var(--warn-100)" : "var(--cyan-100)",
              color: p.severity === "deny" ? "var(--fail-800)" : p.severity === "warn" ? "var(--warn-800)" : "var(--cyan-800)",
            }}
          >
            {p.severity}
          </span>
        </label>
      ))}
    </div>
  );
}

function StepDetail({
  idx,
  total,
  step,
  setIdx,
}: {
  idx: number;
  total: number;
  step: SequenceStepResult | undefined;
  setIdx: (n: number) => void;
}) {
  if (!step) return null;
  return (
    <div className="sim-card outcomes">
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 }}>
        <button className="btn" onClick={() => setIdx(Math.max(0, idx - 1))} disabled={idx === 0}>‹</button>
        <h4 style={{ flex: 1, margin: 0 }}>
          Step {idx + 1} / {total}
          {step.label ? ` — ${step.label}` : ""}
        </h4>
        <button className="btn" onClick={() => setIdx(Math.min(total - 1, idx + 1))} disabled={idx >= total - 1}>›</button>
      </div>
      <span className={`step-verdict-pill ${step.verdict}`} style={{ display: "inline-block", marginBottom: 8 }}>
        {step.verdict.toUpperCase()}
      </span>
      {step.policy_results.length === 0 && (
        <div style={{ fontSize: 12, color: "var(--slate-400)" }}>
          평가된 정책 없음 (정책 필터를 다시 확인하세요)
        </div>
      )}
      {step.policy_results.map((o) => (
        <div key={o.policy_id} className="outcome-row">
          <span className={`sev ${o.severity}`}>{o.severity}</span>
          <span className="pname">{o.policy_name}</span>
          <span style={{ flex: 1 }} />
          <span className={o.decision}>{o.decision}</span>
        </div>
      ))}
    </div>
  );
}
