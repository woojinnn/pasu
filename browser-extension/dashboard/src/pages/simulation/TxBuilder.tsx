/**
 * Transaction sequence builder.
 *
 * - Hard cap of MAX_STEPS items (UI gates +Add, and the page enforces on import too).
 * - Reorder via ▲▼ buttons (kept dep-free; can be upgraded to native HTML5 DnD later).
 * - Each card surfaces principal/action/resource/context — same Cedar shape the
 *   simulator already accepts, just laid out as a card instead of an inline grid.
 * - When a sequence has been evaluated, the card shows its verdict pill and a
 *   "blocked-by" badge that's clickable into the right-rail violation panel.
 */
import type { SequenceStepInput, SequenceStepResult } from "../../cedar";
import type { ExampleTransaction } from "../../server-api";
import { decodeTxLocal } from "../../tools/tx-decode";

export const MAX_STEPS = 5;

export interface TxBuilderProps {
  steps: SequenceStepInput[];
  setSteps: (next: SequenceStepInput[]) => void;
  examples: ExampleTransaction[];
  /** map of step idx → verdict result (only after a run) */
  verdictByIdx: Map<number, SequenceStepResult>;
  selectedIdx: number;
  onSelect: (idx: number) => void;
  /** when a violation is selected on the right rail, we flash its source step */
  flashStepIdx: number | null;
}

export function TxBuilder(props: TxBuilderProps) {
  const { steps, setSteps, examples, verdictByIdx, selectedIdx, onSelect, flashStepIdx } = props;
  const atCap = steps.length >= MAX_STEPS;

  const updateStep = (idx: number, patch: Partial<SequenceStepInput>) => {
    setSteps(steps.map((s, i) => (i === idx ? { ...s, ...patch } : s)));
  };
  const updateContext = (idx: number, raw: string) => {
    try {
      const parsed = raw.trim() === "" ? {} : (JSON.parse(raw) as Record<string, unknown>);
      updateStep(idx, { context: parsed });
    } catch {
      /* keep last-good */
    }
  };
  const move = (idx: number, dir: -1 | 1) => {
    const j = idx + dir;
    if (j < 0 || j >= steps.length) return;
    const next = steps.slice();
    [next[idx], next[j]] = [next[j], next[idx]];
    setSteps(next);
    if (selectedIdx === idx) onSelect(j);
    else if (selectedIdx === j) onSelect(idx);
  };
  const remove = (idx: number) => {
    if (steps.length === 1) return;
    setSteps(steps.filter((_, i) => i !== idx));
    if (selectedIdx >= steps.length - 1) onSelect(Math.max(0, steps.length - 2));
  };
  const add = () => {
    if (atCap) return;
    setSteps([...steps, blankStep(steps.length)]);
  };

  const loadExample = async (ex: ExampleTransaction) => {
    if (atCap) return;
    const meta = (ex.meta as Record<string, unknown>) ?? {};
    const action = (ex.action as Record<string, unknown>) ?? {};
    const from = (meta.from as string) ?? "0x0000000000000000000000000000000000000000";
    const to = (meta.to as string) ?? "0x0000000000000000000000000000000000000000";
    const domain = String(action.domain ?? "Generic");
    const kind = String(action.kind ?? "Tx");
    const label = ex.label.ko || ex.label.en;
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
    setSteps([
      ...steps,
      {
        label,
        principal: `Wallet::"${from}"`,
        action: `Action::"${envelopeKind}"`,
        resource: `Protocol::"${to}"`,
        entities: [],
        context: ctx,
      },
    ]);
  };

  return (
    <div className="sim-card tx-builder">
      <div className="card-head">
        <h3>트랜잭션 시퀀스</h3>
        <span className="cap-pill" data-at-cap={atCap || undefined}>
          {steps.length} / {MAX_STEPS}
        </span>
      </div>

      <div className="tools-row">
        <select
          onChange={(e) => {
            const ex = examples.find((x) => x.id === e.target.value);
            if (ex) loadExample(ex);
            e.target.value = "";
          }}
          defaultValue=""
          disabled={atCap}
        >
          <option value="" disabled>
            🧪 예시에서 TX 추가…
          </option>
          {examples.map((ex) => (
            <option key={ex.id} value={ex.id}>
              {ex.label.ko || ex.label.en}
            </option>
          ))}
        </select>
        <button className="btn" onClick={add} disabled={atCap}>
          + 빈 TX
        </button>
      </div>

      <div className="tx-cards">
        {steps.map((s, idx) => {
          const rs = verdictByIdx.get(idx);
          const verdictClass = rs ? `verdict-${rs.verdict}` : "";
          const isSel = selectedIdx === idx;
          const flash = flashStepIdx === idx;
          return (
            <div
              key={idx}
              className={`tx-card ${verdictClass} ${isSel ? "is-sel" : ""} ${flash ? "is-flash" : ""}`}
              onClick={() => onSelect(idx)}
            >
              <div className="tx-card-head">
                <div className="ord">{String(idx + 1).padStart(2, "0")}</div>
                <input
                  className="tx-label"
                  type="text"
                  placeholder="라벨 (예: 스왑)"
                  value={s.label ?? ""}
                  onChange={(e) => updateStep(idx, { label: e.target.value })}
                  onClick={(e) => e.stopPropagation()}
                />
                {rs && (
                  <span className={`vpill ${rs.verdict}`}>
                    {rs.verdict.toUpperCase()}
                  </span>
                )}
                <div className="reorder">
                  <button
                    className="btn icon"
                    onClick={(e) => { e.stopPropagation(); move(idx, -1); }}
                    disabled={idx === 0}
                    title="위로"
                  >▲</button>
                  <button
                    className="btn icon"
                    onClick={(e) => { e.stopPropagation(); move(idx, 1); }}
                    disabled={idx === steps.length - 1}
                    title="아래로"
                  >▼</button>
                  <button
                    className="btn icon danger"
                    onClick={(e) => { e.stopPropagation(); remove(idx); }}
                    disabled={steps.length === 1}
                    title="제거"
                  >✕</button>
                </div>
              </div>

              <div className="tx-grid">
                <label>
                  <span>principal</span>
                  <input
                    type="text"
                    value={s.principal}
                    onChange={(e) => updateStep(idx, { principal: e.target.value })}
                    onClick={(e) => e.stopPropagation()}
                  />
                </label>
                <label>
                  <span>action</span>
                  <input
                    type="text"
                    value={s.action}
                    onChange={(e) => updateStep(idx, { action: e.target.value })}
                    onClick={(e) => e.stopPropagation()}
                  />
                </label>
                <label>
                  <span>resource</span>
                  <input
                    type="text"
                    value={s.resource}
                    onChange={(e) => updateStep(idx, { resource: e.target.value })}
                    onClick={(e) => e.stopPropagation()}
                  />
                </label>
                <label className="ctx">
                  <span>context</span>
                  <textarea
                    defaultValue={JSON.stringify(s.context ?? {}, null, 2)}
                    onBlur={(e) => updateContext(idx, e.target.value)}
                    onClick={(e) => e.stopPropagation()}
                  />
                </label>
              </div>

              {rs && rs.verdict !== "pass" && (
                <div className="tx-card-foot">
                  {rs.policy_results
                    .filter((o) => o.decision === "deny")
                    .map((o) => (
                      <span key={o.policy_id} className={`block-tag sev-${o.severity}`}>
                        ⛔ {o.policy_name}
                      </span>
                    ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── helpers ───────────────────────────────────────────────────────────────

export function blankStep(idx: number): SequenceStepInput {
  return {
    label: `TX ${idx + 1}`,
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
