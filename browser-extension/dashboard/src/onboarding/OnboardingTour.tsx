import { useEffect, useState } from "react";
import { HELP_ENTRIES, TOUR_STEPS } from "./help-content";
import "./OnboardingTour.css";

const SEEN_KEY = "scopeball:dashboard:onboarding-seen:v1";

interface OnboardingTourProps {
  /** True if the modal should open at mount regardless of localStorage —
   *  used by the header `?` button to force-open the help dialog. */
  forceOpen?: boolean;
  /** Forced-open variant calls this when the user closes the modal. */
  onClose?: () => void;
}

export function OnboardingTour({ forceOpen, onClose }: OnboardingTourProps) {
  const [open, setOpen] = useState<boolean>(() => {
    if (forceOpen) return true;
    try {
      return localStorage.getItem(SEEN_KEY) === null;
    } catch {
      return false;
    }
  });
  const [stepIdx, setStepIdx] = useState(0);
  const [showHelp, setShowHelp] = useState(forceOpen ?? false);

  useEffect(() => {
    if (forceOpen) {
      setOpen(true);
      setShowHelp(true);
    }
  }, [forceOpen]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeAll();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const closeAll = () => {
    try {
      localStorage.setItem(SEEN_KEY, String(Date.now()));
    } catch {
      /* private mode */
    }
    setOpen(false);
    onClose?.();
  };

  if (!open) return null;

  if (showHelp) {
    // Reference card mode — invoked from the `?` button. Shows the
    // glossary list and lets the user re-watch the first-run tour from
    // the same modal.
    return (
      <div
        className="ob-backdrop"
        role="dialog"
        aria-modal="true"
        aria-labelledby="ob-title"
        onClick={closeAll}
      >
        <div className="ob-modal" onClick={(e) => e.stopPropagation()}>
          <h2 id="ob-title" className="ob-title">
            가이드
          </h2>
          <div className="ob-help-list">
            {HELP_ENTRIES.map((entry, i) => (
              <section key={i} className="ob-help-entry">
                <h3>{entry.title}</h3>
                <p>{entry.body}</p>
              </section>
            ))}
          </div>
          <footer className="ob-footer">
            <button
              type="button"
              className="ob-btn-secondary"
              onClick={() => {
                setShowHelp(false);
                setStepIdx(0);
              }}
            >
              튜토리얼 다시 보기
            </button>
            <button
              type="button"
              className="ob-btn-primary"
              onClick={closeAll}
            >
              닫기
            </button>
          </footer>
        </div>
      </div>
    );
  }

  const step = TOUR_STEPS[stepIdx];
  if (!step) {
    closeAll();
    return null;
  }
  const isLast = stepIdx === TOUR_STEPS.length - 1;

  return (
    <div
      className="ob-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="ob-title"
      onClick={closeAll}
    >
      <div className="ob-modal" onClick={(e) => e.stopPropagation()}>
        <div className="ob-stepper">
          {TOUR_STEPS.map((_, i) => (
            <span
              key={i}
              className={"ob-dot" + (i === stepIdx ? " active" : "")}
              aria-hidden
            />
          ))}
          <span className="ob-step-count">
            {stepIdx + 1} / {TOUR_STEPS.length}
          </span>
        </div>
        <h2 id="ob-title" className="ob-title">
          {step.title}
        </h2>
        <p className="ob-body">{step.body}</p>
        <footer className="ob-footer">
          <button
            type="button"
            className="ob-btn-secondary"
            onClick={closeAll}
          >
            건너뛰기
          </button>
          {stepIdx > 0 ? (
            <button
              type="button"
              className="ob-btn-secondary"
              onClick={() => setStepIdx((i) => i - 1)}
            >
              이전
            </button>
          ) : null}
          <button
            type="button"
            className="ob-btn-primary"
            onClick={() => {
              if (isLast) closeAll();
              else setStepIdx((i) => i + 1);
            }}
          >
            {isLast ? "시작하기" : "다음"}
          </button>
        </footer>
      </div>
    </div>
  );
}
