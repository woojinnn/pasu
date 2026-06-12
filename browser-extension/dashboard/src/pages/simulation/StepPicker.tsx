/**
 * Discrete step picker — `[S₀] [S₁] [S₂] …`.
 *
 * Replaces the prior `<input type="range">` scrubber across the simulation
 * panels. The cursor lives at the page level so multiple panels render the
 * same buttons and tap into the same `cursorIdx` state. The label under
 * the row spells out what the cursor means at the current value.
 */

import { useTranslation } from "react-i18next";

export interface StepPickerProps {
  totalSteps: number;
  cursorIdx: number;
  setCursorIdx: (next: number) => void;
}

export function StepPicker({
  totalSteps,
  cursorIdx,
  setCursorIdx,
}: StepPickerProps) {
  const { t } = useTranslation("simulation");
  return (
    <>
      <div className="state-step-buttons">
        {Array.from({ length: totalSteps + 1 }, (_, i) => (
          <button
            key={i}
            type="button"
            className={`state-step-btn ${i === cursorIdx ? "is-active" : ""}`}
            onClick={() => setCursorIdx(i)}
          >
            {i === 0 ? "S₀" : `S${i}`}
          </button>
        ))}
      </div>
      <div className="state-step-label">
        {cursorIdx === 0
          ? t("historic.stepPicker.initialState")
          : t("historic.stepPicker.afterTx", { n: cursorIdx, total: totalSteps })}
      </div>
    </>
  );
}
