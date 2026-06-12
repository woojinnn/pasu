import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Trans, useTranslation } from "react-i18next";

import { deleteWallet, ServerError } from "../../server-api";
import { Modal } from "../../components/Modal";

interface Props {
  open: boolean;
  onClose: () => void;
  address: string;
  label: string | null;
}

/** In-page delete confirmation — replaces the prototype's window.confirm()
 * (the native dialog shows the extension's ugly origin string). */
export function DeleteWalletModal({ open, onClose, address, label }: Props) {
  const { t } = useTranslation("home");
  const qc = useQueryClient();
  const mut = useMutation({
    mutationFn: () => deleteWallet(address),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      onClose();
    },
  });

  return (
    <Modal
      open={open}
      onClose={() => !mut.isPending && onClose()}
      title={t("delete.title")}
      width={400}
      footer={
        <>
          <button className="btn" onClick={onClose} disabled={mut.isPending}>
            {t("common:cancel")}
          </button>
          <button className="btn danger" onClick={() => mut.mutate()} disabled={mut.isPending}>
            {mut.isPending ? t("delete.deleting") : t("common:delete")}
          </button>
        </>
      }
    >
      <p style={{ margin: "0 0 10px", fontSize: 14, color: "var(--slate-800)" }}>
        <Trans
          t={t}
          i18nKey="delete.confirm"
          values={{ name: label ?? address }}
          components={{ b: <b /> }}
        />
      </p>
      <div style={{ display: "flex", gap: 9, alignItems: "flex-start", background: "var(--fail-50)", border: "1px solid var(--fail-200)", borderRadius: 10, padding: "10px 12px", fontSize: 12.5, color: "var(--slate-600)", lineHeight: 1.5 }}>
        <svg viewBox="0 0 24 24" width={15} height={15} fill="none" stroke="var(--fail-600)" strokeWidth={1.9} strokeLinecap="round" strokeLinejoin="round" style={{ flexShrink: 0, marginTop: 1 }}>
          <path d="M3 6h18" />
          <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
          <path d="M10 11v6M14 11v6" />
        </svg>
        <span>
          <Trans t={t} i18nKey="delete.warning" components={{ b: <b /> }} />
        </span>
      </div>
      {mut.error && (
        <div className="err">
          {t("delete.failed")}&nbsp;
          {mut.error instanceof ServerError ? `${mut.error.status} ${String(mut.error.body)}` : String(mut.error)}
        </div>
      )}
    </Modal>
  );
}
