import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { patchWallet, ServerError } from "@scopeball/api-client";

import { Modal } from "./Modal";

interface Props {
  open: boolean;
  onClose: () => void;
  address: string;
  initial: string | null;
}

export function RenameWalletModal({ open, onClose, address, initial }: Props) {
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
      title="지갑 이름 변경"
      footer={
        <>
          <button className="btn" onClick={onClose} disabled={mut.isPending}>취소</button>
          <button className="btn primary" onClick={() => mut.mutate()} disabled={mut.isPending}>
            {mut.isPending ? "저장 중…" : "저장"}
          </button>
        </>
      }
    >
      <div className="form-row">
        <label htmlFor="rn-label">라벨 (비우면 제거)</label>
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
          저장 실패:&nbsp;
          {mut.error instanceof ServerError
            ? `${mut.error.status} ${String(mut.error.body)}`
            : String(mut.error)}
        </div>
      )}
    </Modal>
  );
}
