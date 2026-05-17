import { useEffect, useRef } from "react";
import "./WarningModal.css";

interface WarningModalProps {
  open: boolean;
  title: string;
  body: string;
  confirmLabel: string;
  cancelLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function WarningModal({
  open,
  title,
  body,
  confirmLabel,
  cancelLabel,
  onConfirm,
  onCancel,
}: WarningModalProps) {
  const confirmRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (open) confirmRef.current?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onCancel]);

  if (!open) return null;

  return (
    <div
      className="warning-modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="warning-modal-title"
      onClick={onCancel}
    >
      <div className="warning-modal" onClick={(e) => e.stopPropagation()}>
        <h2 id="warning-modal-title" className="warning-modal-title">
          {title}
        </h2>
        <p className="warning-modal-body">{body}</p>
        <div className="warning-modal-actions">
          <button
            type="button"
            className="warning-modal-cancel"
            onClick={onCancel}
          >
            {cancelLabel}
          </button>
          <button
            ref={confirmRef}
            type="button"
            className="warning-modal-confirm"
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
