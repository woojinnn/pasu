/** The 4-step progress header. Click a completed/earlier step to jump back. */
import { STEP_LABELS, type WizardStep } from "./types";

const STEPS: WizardStep[] = [1, 2, 3, 4];

export function StepNav({ step, goTo }: { step: WizardStep; goTo: (s: WizardStep) => void }) {
  return (
    <ol className="sw-nav">
      {STEPS.map((s) => {
        const state = s === step ? "active" : s < step ? "done" : "todo";
        return (
          <li key={s} className={`sw-nav-item ${state}`}>
            <button type="button" disabled={s > step} onClick={() => goTo(s)}>
              <span className="sw-nav-num">{s < step ? "✓" : s}</span>
              <span className="sw-nav-label">{STEP_LABELS[s]}</span>
            </button>
            {s < 4 && <span className="sw-nav-bar" />}
          </li>
        );
      })}
    </ol>
  );
}
