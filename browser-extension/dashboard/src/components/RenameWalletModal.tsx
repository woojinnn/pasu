import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import { patchWallet, ServerError } from "../server-api";

import { Modal } from "./Modal";

interface Props {
  open: boolean;
  onClose: () => void;
  address: string;
  initial: string | null;
}

export function RenameWalletModal({ open, onClose, address, initial }: Props) {
  const { t } = useTranslation("common");
  const qc = useQueryClient();
  const [label, setLabel] = useState(initial ?? "");

  useEffect(() => {
    if (open) setLabel(initial ?? "");
  }, [open, initial]);

  const mut = useMutation({
    mutationFn: () =>
      patchWallet(address, { label: label.trim() === "" ? null : label.trim() }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      qc.invalidateQueries({ queryKey: ["wallets"] });
      onClose();
    },
  });

  return (
    <Modal
      open={open}
      onClose={() => !mut.isPending && onClose()}
      title={t("wallet.renameTitle")}
      footer={
        <>
          <button className="btn" onClick={onClose} disabled={mut.isPending}>{t("cancel")}</button>
          <button className="btn primary" onClick={() => mut.mutate()} disabled={mut.isPending}>
            {mut.isPending ? t("wallet.saving") : t("save")}
          </button>
        </>
      }
    >
      <div className="form-row">
        <label htmlFor="rn-label">{t("wallet.renameLabel")}</label>
        <input
          id="rn-label"
          type="text"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
          autoFocus
        />
        <div className="hint" style={{ fontFamily: "var(--ff-mono)" }}>{address}</div>
      </div>
      {mut.error && (
        <div className="err">
          {t("wallet.saveFailed")}&nbsp;
          {mut.error instanceof ServerError
            ? `${mut.error.status} ${String(mut.error.body)}`
            : String(mut.error)}
        </div>
      )}
    </Modal>
  );
}
