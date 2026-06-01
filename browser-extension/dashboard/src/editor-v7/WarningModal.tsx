/**
 * Small modal for destructive-confirm flows in the editor — used when
 * the user is about to discard either the builder tree (Code → Builder
 * after manual edits) or their hand-written Cedar (Builder edit while
 * Code has unsaved changes).
 *
 * Kept dependency-free (no portal lib) since we already have a similar
 * Modal component in `components/modal.tsx`, but we want a different
 * tone (warning vs. confirm) here without coupling.
 */

import { useEffect } from "react";

export interface WarningModalProps {
  open: boolean;
  title: string;
  body: React.ReactNode;
  confirmLabel: string;
  confirmTone?: "danger" | "primary";
  cancelLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function WarningModal({
  open,
  title,
  body,
  confirmLabel,
  confirmTone = "danger",
  cancelLabel = "취소",
  onConfirm,
  onCancel,
}: WarningModalProps) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onCancel]);

  if (!open) return null;
  return (
    <div className="v7-modal-backdrop" onClick={onCancel}>
      <div className="v7-modal" onClick={(e) => e.stopPropagation()}>
        <h3>{title}</h3>
        <div className="v7-modal-body">{body}</div>
        <div className="v7-modal-actions">
          <button className="btn-secondary" onClick={onCancel}>{cancelLabel}</button>
          <button
            className={confirmTone === "danger" ? "btn-danger" : "btn-primary"}
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
