/**
 * Discrete step picker — `[S₀] [S₁] [S₂] …`.
 *
 * Replaces the prior `<input type="range">` scrubber across the simulation
 * panels. The cursor lives at the page level so multiple panels render the
 * same buttons and tap into the same `cursorIdx` state. The label under
 * the row spells out what the cursor means at the current value.
 */

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
          ? "초기 상태"
          : `TX ${cursorIdx} 적용 후 (of ${totalSteps})`}
      </div>
    </>
  );
}
