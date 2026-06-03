/**
 * Simulation page — author up to N TXs, evaluate them as a sequence,
 * visualize per-TX state evolution and which policies block where.
 *
 * Layout (3 columns at >= ~1100px, stacks on narrower screens):
 *   ┌──────────────┬─────────────────────────┬──────────────┐
 *   │ TX builder   │ State timeline + diff   │ Violations + │
 *   │ (max 5)      │ (S₀ → Sₙ scrubber)      │ policy on/off│
 *   └──────────────┴─────────────────────────┴──────────────┘
 *
 * Cross-panel linking:
 *   - Click a violation (right) → red overlay on referenced state rows (center).
 *   - Click a state row (center) → flashes the implicated policies (right).
 *   - Click a TX card (left)   → moves the state scrubber to that step's post-state.
 *
 * The page-local `enabledIds` Set is what drives which policies get sent
 * to the simulator. The `enabled` flag in extension storage is left alone
 * — the help line under the policy header tells the user this is "what-if"
 * only.
 *
 * State snapshots are produced by the heuristic applier in `state-mock.ts`.
 * Swap that for live reducer output when the wasm bridge exposes a
 * `policy-state:reduce-step` op.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";

import {
  getEnabledPolicyIds,
  getExampleTransactions,
  listManagedPolicies,
  type InstalledPolicy,
  type ManagedPolicy,
} from "../server-api";
import {
  simulateSequenceLocal,
  type PolicyInput,
  type SequenceResp,
  type SequenceStepInput,
  type SequenceStepResult,
} from "../cedar";
import { Topbar } from "../shell/Topbar";

import { TxBuilder, blankStep, MAX_STEPS } from "./simulation/TxBuilder";
import { StatePanel } from "./simulation/StatePanel";
import { PolicyPanel } from "./simulation/PolicyPanel";
import { inferPolicyRefs } from "./simulation/policy-refs";
import { nameFromPolicy, severityFromCedar } from "./editor/policy-meta";
import { DEFAULT_INITIAL_STATE, buildTimeline } from "./simulation/state-mock";
import { WasmStepProbe } from "./simulation/WasmStepProbe";
import { CalldataProbe } from "./simulation/CalldataProbe";

import "./simulation.css";

export function SimulationPage() {
  // ── TX sequence ────────────────────────────────────────────────────────
  const [steps, setSteps] = useState<SequenceStepInput[]>([blankStep(0)]);
  const safeSetSteps = (next: SequenceStepInput[]) => {
    setSteps(next.slice(0, MAX_STEPS));
  };

  // ── extension-local policies ────────────────────────────────────────────
  // Server `listPolicies` was retired; policies now live in the SW's
  // `chrome.storage.local`. We fetch the same two sources EditorListPage
  // uses so this page stays in sync with the editor / popup.
  const managedQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
  });
  const liveEnabledQ = useQuery({
    queryKey: ["enabled-policy-ids"],
    queryFn: getEnabledPolicyIds,
  });
  const examplesQ = useQuery({
    queryKey: ["example-transactions"],
    queryFn: getExampleTransactions,
  });

  // Adapter: ManagedPolicy → InstalledPolicy-shaped row (which is what
  // PolicyPanel already consumes). We assign a deterministic numeric id
  // per render via a stable string-id↔number map so the simulator's
  // `PolicyInput.policy_id: number` matches the verdict result's id.
  const { adaptedPolicies, idMap } = useMemo(() => {
    const list = managedQ.data ?? [];
    const idMap = new Map<number, string>(); // numeric → original string id
    const adapted: InstalledPolicy[] = list.map((p: ManagedPolicy, idx) => {
      idMap.set(idx, p.id);
      return {
        id: idx,
        name: nameFromPolicy(p),
        description: null,
        cedar_text: p.text,
        severity: severityFromCedar(p.text),
        enabled: true, // sim treats live enabled as a seed; per-row toggle below
        created_at: Math.floor(p.updatedAtMs / 1000),
        updated_at: Math.floor(p.updatedAtMs / 1000),
      };
    });
    return { adaptedPolicies: adapted, idMap };
  }, [managedQ.data]);

  // ── policy on/off (simulation-only) ─────────────────────────────────────
  const [enabledIds, setEnabledIds] = useState<Set<number>>(new Set());
  // Seed once: every policy whose live (extension-storage) `enabled` flag
  // is on becomes the sim's starting allowlist. Toggles after that are
  // page-local — the live enabled set is never written back from here.
  const seededRef = useRef(false);
  useEffect(() => {
    if (seededRef.current) return;
    if (adaptedPolicies.length === 0) return;
    if (liveEnabledQ.data == null) return; // wait for both
    seededRef.current = true;
    const liveSet = new Set(liveEnabledQ.data);
    setEnabledIds(
      new Set(
        adaptedPolicies
          .filter((p) => liveSet.has(idMap.get(p.id)!))
          .map((p) => p.id),
      ),
    );
  }, [adaptedPolicies, liveEnabledQ.data, idMap]);
  const togglePolicy = (id: number) =>
    setEnabledIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const enableAll = () =>
    setEnabledIds(new Set(adaptedPolicies.map((p) => p.id)));
  const disableAll = () => setEnabledIds(new Set());

  // ── run ────────────────────────────────────────────────────────────────
  const [result, setResult] = useState<SequenceResp | null>(null);
  const runMut = useMutation({
    mutationFn: () => {
      const chosen = adaptedPolicies.filter((p) => enabledIds.has(p.id));
      const policies: PolicyInput[] = chosen.map((p) => ({
        policy_id: p.id,
        policy_name: p.name,
        severity: p.severity,
        cedar_text: p.cedar_text,
      }));
      return simulateSequenceLocal(steps, policies);
    },
    onSuccess: (resp) => {
      setResult(resp);
      // Park the scrubber on the first non-pass step if any, else S₀.
      const firstBad = resp.steps.findIndex((s) => s.verdict !== "pass");
      setCursorIdx(firstBad === -1 ? 0 : firstBad + 1);
    },
  });

  // Stale-result invalidation: any change to steps or enabledIds wipes the
  // last result so the UI doesn't show verdicts that no longer correspond
  // to the inputs.
  const lastInputsSig = useRef("");
  useEffect(() => {
    const sig = JSON.stringify({ steps, enabled: [...enabledIds].sort() });
    if (lastInputsSig.current === "") {
      lastInputsSig.current = sig;
      return;
    }
    if (sig !== lastInputsSig.current) {
      lastInputsSig.current = sig;
      if (result) setResult(null);
    }
  }, [steps, enabledIds, result]);

  // ── state timeline (mocked locally) ─────────────────────────────────────
  const snapshots = useMemo(
    () => buildTimeline(DEFAULT_INITIAL_STATE, steps),
    [steps],
  );
  const [cursorIdx, setCursorIdx] = useState(0);
  // Clamp cursor when steps shrink.
  useEffect(() => {
    if (cursorIdx > snapshots.length - 1) setCursorIdx(snapshots.length - 1);
  }, [snapshots.length, cursorIdx]);

  // ── verdict lookup ──────────────────────────────────────────────────────
  const verdictByStep = useMemo(() => {
    const m = new Map<number, SequenceStepResult>();
    result?.steps.forEach((r, i) => m.set(i, r));
    return m;
  }, [result]);
  // Right rail shows the verdict for the step that produced the current snapshot.
  const verdictForCursor = cursorIdx === 0 ? undefined : verdictByStep.get(cursorIdx - 1);

  // ── display toggles for state panel ─────────────────────────────────────
  const [changedOnly, setChangedOnly] = useState(false);
  const [changedFirst, setChangedFirst] = useState(true);

  // ── cross-panel selection ───────────────────────────────────────────────
  const [focusedViolation, setFocusedViolation] =
    useState<{ stepIdx: number; policyId: number } | null>(null);
  const [focusedRowKey, setFocusedRowKey] = useState<string | null>(null);
  const [flashPolicyId, setFlashPolicyId] = useState<number | null>(null);
  const [flashStepIdx, setFlashStepIdx] = useState<number | null>(null);

  // When the cursor moves away, clear violation focus (it was scoped to a step).
  useEffect(() => {
    if (focusedViolation && focusedViolation.stepIdx !== cursorIdx) {
      setFocusedViolation(null);
    }
  }, [cursorIdx, focusedViolation]);

  // When a violation is focused, briefly flash the source TX in the left rail.
  useEffect(() => {
    if (!focusedViolation) return;
    setFlashStepIdx(focusedViolation.stepIdx - 1);
    const id = setTimeout(() => setFlashStepIdx(null), 900);
    return () => clearTimeout(id);
  }, [focusedViolation]);

  // When a state row is clicked, find policies that reference it and flash.
  const onStateRowClick = (rowKey: string | null) => {
    setFocusedRowKey(rowKey);
    if (!rowKey || !verdictForCursor) {
      setFlashPolicyId(null);
      return;
    }
    const snap = snapshots[cursorIdx];
    if (!snap) return;
    for (const o of verdictForCursor.policy_results) {
      const refs = inferPolicyRefs(o.policy_name, snap.state);
      const hit = refs.rowKeys.includes(rowKey)
        || (refs.buckets.includes("tokens") && snap.state.tokens.some((t) => t.key === rowKey))
        || (refs.buckets.includes("positions") && snap.state.positions.some((p) => p.key === rowKey));
      if (hit) {
        setFlashPolicyId(o.policy_id);
        setTimeout(() => setFlashPolicyId(null), 1200);
        return;
      }
    }
    setFlashPolicyId(null);
  };

  // ── derived ─────────────────────────────────────────────────────────────
  const stepCount = steps.length;
  const blockedSteps =
    result?.steps.filter((s) => s.verdict === "fail").length ?? 0;

  return (
    <>
      <Topbar
        here="Simulation"
        subtitle={`${stepCount} / ${MAX_STEPS} TX`}
      />

      {/* Run strip — always visible above the columns */}
      <div className="sim-runstrip">
        <button
          className="btn primary"
          onClick={() => runMut.mutate()}
          disabled={runMut.isPending || steps.length === 0}
        >
          {runMut.isPending ? "실행 중…" : `시뮬레이션 실행 (${steps.length})`}
        </button>
        <div className="rs-meta">
          정책: <strong>{enabledIds.size}</strong> 활성
          <span className="sep">·</span>
          TX: <strong>{stepCount}</strong>
        </div>
        {result && (
          <div className={`rs-overall ${result.overall}`}>
            <span className="ov-pill">{result.overall.toUpperCase()}</span>
            <span className="ov-sub">
              {result.steps.filter((s) => s.verdict === "pass").length} pass ·{" "}
              {result.steps.filter((s) => s.verdict === "warn").length} warn ·{" "}
              {blockedSteps} fail
            </span>
          </div>
        )}
        {runMut.error && (
          <div className="rs-err">{String(runMut.error)}</div>
        )}
      </div>

      <div className="sim-layout">
        <TxBuilder
          steps={steps}
          setSteps={safeSetSteps}
          examples={examplesQ.data ?? []}
          verdictByIdx={verdictByStep}
          selectedIdx={Math.max(0, cursorIdx - 1)}
          onSelect={(idx) => setCursorIdx(idx + 1)}
          flashStepIdx={flashStepIdx}
        />

        <StatePanel
          snapshots={snapshots}
          cursorIdx={cursorIdx}
          setCursorIdx={setCursorIdx}
          verdictForCursor={verdictForCursor}
          verdictByStep={verdictByStep}
          changedOnly={changedOnly}
          setChangedOnly={setChangedOnly}
          changedFirst={changedFirst}
          setChangedFirst={setChangedFirst}
          focusedViolation={focusedViolation}
          onStateRowClick={onStateRowClick}
          focusedRowKey={focusedRowKey}
        />

        <PolicyPanel
          policies={adaptedPolicies}
          enabledIds={enabledIds}
          toggle={togglePolicy}
          enableAll={enableAll}
          disableAll={disableAll}
          currentStep={verdictForCursor}
          cursorIdx={cursorIdx}
          focusedViolation={focusedViolation}
          setFocusedViolation={setFocusedViolation}
          flashPolicyId={flashPolicyId}
        />
      </div>

      <WasmStepProbe />
      <CalldataProbe />
    </>
  );
}
