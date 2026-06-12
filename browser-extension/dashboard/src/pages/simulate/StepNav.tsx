/** The 4-step progress header. Click a completed/earlier step to jump back. */
import { useTranslation } from "react-i18next";

import type { WizardStep } from "./types";

const STEPS: WizardStep[] = [1, 2, 3, 4];

export function StepNav({ step, goTo }: { step: WizardStep; goTo: (s: WizardStep) => void }) {
  const { t } = useTranslation("simulation");
  return (
    <ol className="sw-nav">
      {STEPS.map((s) => {
        const state = s === step ? "active" : s < step ? "done" : "todo";
        return (
          <li key={s} className={`sw-nav-item ${state}`}>
            <button type="button" disabled={s > step} onClick={() => goTo(s)}>
              <span className="sw-nav-num">{s < step ? "✓" : s}</span>
              <span className="sw-nav-label">{t(`wizard.steps.${s}`)}</span>
            </button>
            {s < 4 && <span className="sw-nav-bar" />}
          </li>
        );
      })}
    </ol>
  );
}
