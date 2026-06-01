import type { ReactNode } from "react";

import { Modal } from "./Modal";

interface ConfirmDialogProps {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: string;
  description: ReactNode;
  confirmLabel?: string;
  tone?: "warn" | "fail";
  pending?: boolean;
}

/**
 * Two-button write-confirm gate. Matches the `confirmCenter` UX of the
 * original front mockup — opens when a destructive/state-changing action
 * is about to fire, requires an explicit click. No focus trap; ESC closes.
 */
export function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  description,
  confirmLabel,
  tone = "warn",
  pending,
}: ConfirmDialogProps) {
  return (
    <Modal
      open={open}
      onClose={() => !pending && onClose()}
      title={title}
      width={460}
      footer={
        <>
          <button className="btn" onClick={onClose} disabled={pending}>
            취소
          </button>
          <button
            className={tone === "fail" ? "btn danger" : "btn primary"}
            onClick={onConfirm}
            disabled={pending}
          >
            {pending ? "처리 중…" : (confirmLabel ?? "확정")}
          </button>
        </>
      }
    >
      <div style={{ fontSize: 13, color: "var(--slate-700)", lineHeight: 1.6 }}>
        {description}
      </div>
    </Modal>
  );
}
