import { useEffect, useMemo, useState } from "react";
import {
  runPolicyTest,
  type PlanDto,
} from "../policy/engine-wasm";
import {
  SAMPLE_TRANSACTIONS,
  findSample,
  type SampleTransaction,
} from "../policy/sample-transactions";
import type { VerdictDto } from "../policy/types";
import {
  loadPreferences,
  subscribePreferences,
} from "../settings/preferences";
import "./PolicyTestPanel.css";

interface PolicyTestPanelProps {
  policyId: string;
  cedarText: string;
  /** Called when user clicks a matched policy row — Editor wires this
   *  to its own state to jump back into edit mode. */
  onOpenMatched?: (policyId: string) => void;
}

interface FormState {
  method: string;
  chainId: string;
  to: string;
  value: string;
  data: string;
  actor: string;
}

function formFromPreferences(): FormState {
  const p = loadPreferences();
  return {
    method: "eth_sendTransaction",
    chainId: String(p.policyTestChainId),
    to: p.policyTestTo,
    value: "0x0",
    data: "0x",
    actor: p.policyTestActor,
  };
}

function applySample(form: FormState, sample: SampleTransaction): FormState {
  return {
    ...form,
    method: sample.method,
    to: sample.to,
    value: sample.value,
    data: sample.data,
  };
}

export function PolicyTestPanel({
  policyId,
  cedarText,
  onOpenMatched,
}: PolicyTestPanelProps) {
  const [form, setForm] = useState<FormState>(() => formFromPreferences());
  const [sampleId, setSampleId] = useState<string>("custom");
  const [resetTick, setResetTick] = useState(0);
  const [verdict, setVerdict] = useState<VerdictDto | null>(null);
  const [plan, setPlan] = useState<PlanDto | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<{
    stage?: string;
    message: string;
  } | null>(null);

  useEffect(() => {
    return subscribePreferences(() => setResetTick((n) => n + 1));
  }, []);

  useEffect(() => {
    if (resetTick === 0) return;
    const seeded = formFromPreferences();
    setForm((f) => ({
      ...f,
      chainId: seeded.chainId,
      to: seeded.to,
      actor: seeded.actor,
    }));
  }, [resetTick]);

  const handle = <K extends keyof FormState>(key: K, value: FormState[K]) => {
    setForm((f) => ({ ...f, [key]: value }));
    // Manual edits drop the user out of "preset" mode so the dropdown
    // matches what's actually in the form.
    setSampleId("custom");
  };

  const handleSampleChange = (nextId: string) => {
    setSampleId(nextId);
    const sample = findSample(nextId);
    if (sample) setForm((f) => applySample(f, sample));
  };

  const canRun =
    policyId.startsWith("dashboard::") &&
    policyId.length > "dashboard::".length &&
    cedarText.trim().length > 0;

  const planSummary = useMemo(() => summarizePlan(plan), [plan]);

  const handleRun = async () => {
    setRunning(true);
    setError(null);
    setVerdict(null);
    setPlan(null);
    try {
      const chainId = Number.parseInt(form.chainId, 10);
      if (!Number.isFinite(chainId) || chainId <= 0) {
        throw new Error("chainId must be a positive integer");
      }
      const outcome = await runPolicyTest({
        policyId,
        cedarText,
        rawRequest: {
          method: form.method,
          chainId,
          params: [
            {
              from: form.actor,
              to: form.to,
              value: form.value,
              data: form.data,
            },
          ],
        },
      });
      if (outcome.plan) setPlan(outcome.plan);
      if (outcome.verdict) {
        setVerdict(outcome.verdict);
      } else if (outcome.error) {
        setError({
          stage: outcome.stage,
          message: outcome.error.message ?? outcome.error.kind ?? "engine error",
        });
      }
    } catch (e) {
      setError({
        message: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="policy-test-panel card">
      <header className="ptp-header">
        <h2>Policy Test</h2>
        <span className="ptp-badge">WASM in-process</span>
      </header>
      <p className="ptp-sub">
        가상 트랜잭션으로 현재 정책의 verdict를 미리 확인합니다. EVM
        시뮬레이션이 아닌 정책 평가 미리보기입니다.
      </p>

      <label className="ptp-field">
        <span>샘플 프리셋</span>
        <select
          value={sampleId}
          onChange={(e) => handleSampleChange(e.target.value)}
        >
          {SAMPLE_TRANSACTIONS.map((s) => (
            <option key={s.id} value={s.id}>
              {s.label}
            </option>
          ))}
        </select>
        <span className="ptp-field-hint">
          {findSample(sampleId)?.description ?? ""}
        </span>
      </label>

      <div className="ptp-form">
        <Field label="Method">
          <input
            value={form.method}
            onChange={(e) => handle("method", e.target.value)}
          />
        </Field>
        <Field label="Chain ID">
          <input
            value={form.chainId}
            onChange={(e) => handle("chainId", e.target.value)}
            inputMode="numeric"
          />
        </Field>
        <Field label="From (actor)">
          <input
            value={form.actor}
            onChange={(e) => handle("actor", e.target.value)}
            placeholder="0x…"
          />
        </Field>
        <Field label="To">
          <input
            value={form.to}
            onChange={(e) => handle("to", e.target.value)}
            placeholder="0x…"
          />
        </Field>
        <Field label="Value (hex)">
          <input
            value={form.value}
            onChange={(e) => handle("value", e.target.value)}
            placeholder="0x0"
          />
        </Field>
        <Field label="Calldata">
          <textarea
            value={form.data}
            onChange={(e) => handle("data", e.target.value)}
            placeholder="0x…"
            rows={2}
          />
        </Field>
      </div>

      <button
        type="button"
        className="ptp-run"
        onClick={handleRun}
        disabled={running || !canRun}
        title={canRun ? "" : "Editor에서 정책 컴파일 후 실행 가능"}
      >
        {running ? "평가 중..." : "Run Policy Test"}
      </button>

      {planSummary ? (
        <div className="ptp-plan">
          <div className="ptp-plan-head">
            <span className="ptp-plan-title">Plan</span>
            <span className="ptp-plan-meta">
              {planSummary.envelopeCount} envelope · {planSummary.callCount} call
            </span>
          </div>
          {planSummary.actions.length > 0 ? (
            <div className="ptp-plan-actions">
              {planSummary.actions.map((a, i) => (
                <span className="ptp-plan-action" key={`${a}-${i}`}>
                  {a}
                </span>
              ))}
            </div>
          ) : (
            <div className="ptp-plan-empty">매칭된 adapter envelope 없음</div>
          )}
          {planSummary.diagnostics.length > 0 ? (
            <ul className="ptp-plan-diag">
              {planSummary.diagnostics.map((d, i) => (
                <li key={i}>{d}</li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}

      {verdict ? (
        <div className={`ptp-result kind-${verdict.kind}`}>
          <div className="ptp-result-head">
            <div className={`verdict-chip kind-${verdict.kind}`}>
              {verdict.kind}
            </div>
            <div className="ptp-result-sub">
              {verdict.matched?.length ?? 0} matched
            </div>
          </div>
          {verdict.matched && verdict.matched.length > 0 ? (
            <ul className="ptp-matched">
              {verdict.matched.map((m, idx) => (
                <li
                  key={`${m.policy_id}-${idx}`}
                  className={onOpenMatched ? "is-clickable" : ""}
                  onClick={() => onOpenMatched?.(m.policy_id)}
                  role={onOpenMatched ? "button" : undefined}
                  tabIndex={onOpenMatched ? 0 : undefined}
                >
                  <div className="ptp-policy-id">{m.policy_id}</div>
                  {m.reason ? (
                    <div className="ptp-reason">{m.reason}</div>
                  ) : null}
                  <div className="ptp-policy-meta">
                    {m.severity} · {m.origin}
                  </div>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}

      {error ? (
        <div className="ptp-error">
          {error.stage ? <strong>{error.stage}: </strong> : null}
          {error.message}
        </div>
      ) : null}
    </div>
  );
}

interface PlanSummary {
  envelopeCount: number;
  callCount: number;
  actions: string[];
  diagnostics: string[];
}

function summarizePlan(plan: PlanDto | null): PlanSummary | null {
  if (!plan) return null;
  // PlanDto is opaque on purpose — but the wire shape includes
  // envelopes/calls/diagnostics for display. Cast through unknown so TS
  // doesn't complain about the structural diff.
  const planAny = plan as unknown as {
    envelopes?: unknown;
    calls?: unknown;
    diagnostics?: unknown;
  };
  const envelopes = Array.isArray(planAny.envelopes) ? planAny.envelopes : [];
  const calls = Array.isArray(planAny.calls) ? planAny.calls : [];
  const diagnostics = Array.isArray(planAny.diagnostics)
    ? planAny.diagnostics.filter((d): d is string => typeof d === "string")
    : [];
  const actions: string[] = [];
  for (const env of envelopes) {
    const action = (env as { action?: unknown }).action;
    if (typeof action === "string") actions.push(action);
  }
  return {
    envelopeCount: envelopes.length,
    callCount: calls.length,
    actions,
    diagnostics,
  };
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="ptp-field">
      <span>{label}</span>
      {children}
    </label>
  );
}
