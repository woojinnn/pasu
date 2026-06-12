import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Trans, useTranslation } from "react-i18next";

import { addWallet, ServerError, type AddWalletResp } from "../server-api";

import { Modal } from "./Modal";

interface AddWalletModalProps {
  open: boolean;
  onClose: () => void;
  /** Called after a successful add. Parent can show a toast / refetch. */
  onAdded?: (resp: AddWalletResp) => void;
}

/** Default chain set shown as toggles. Empty selection → server tracks all configured chains. */
const CHAIN_OPTIONS: Array<{ id: string; label: string }> = [
  { id: "eip155:1", label: "Ethereum" },
  { id: "eip155:42161", label: "Arbitrum" },
  { id: "eip155:8453", label: "Base" },
  { id: "eip155:10", label: "Optimism" },
  { id: "eip155:137", label: "Polygon" },
];

const ADDR_RX = /^0x[0-9a-fA-F]{40}$/;

export function AddWalletModal({ open, onClose, onAdded }: AddWalletModalProps) {
  const { t } = useTranslation("common");
  const qc = useQueryClient();
  const [address, setAddress] = useState("");
  const [label, setLabel] = useState("");
  const [chains, setChains] = useState<Set<string>>(new Set());
  const [touched, setTouched] = useState(false);

  const reset = () => {
    setAddress("");
    setLabel("");
    setChains(new Set());
    setTouched(false);
  };

  const addressOk = ADDR_RX.test(address.trim());

  const mut = useMutation({
    mutationFn: () =>
      addWallet({
        address: address.trim().toLowerCase(),
        chains: chains.size === 0 ? undefined : Array.from(chains),
        label: label.trim() || undefined,
      }),
    onSuccess: (resp) => {
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      qc.invalidateQueries({ queryKey: ["wallets"] });
      onAdded?.(resp);
      // Keep modal open so user sees the sync result. They close it manually.
    },
  });

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setTouched(true);
    if (!addressOk) return;
    mut.mutate();
  };

  const toggleChain = (id: string) =>
    setChains((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <Modal
      open={open}
      onClose={() => {
        if (!mut.isPending) {
          reset();
          onClose();
        }
      }}
      title={mut.data ? t("wallet.addResultTitle") : t("wallet.addTitle")}
      footer={
        mut.data ? (
          <button className="btn primary" onClick={onClose}>{t("close")}</button>
        ) : (
          <>
            <button className="btn" type="button" onClick={onClose} disabled={mut.isPending}>
              {t("cancel")}
            </button>
            <button className="btn primary" type="submit" form="add-wallet-form" disabled={mut.isPending || !addressOk}>
              {mut.isPending ? t("wallet.adding") : t("add")}
            </button>
          </>
        )
      }
    >
      <form id="add-wallet-form" onSubmit={onSubmit}>
        <div className="form-row">
          <label htmlFor="aw-addr">{t("wallet.addressLabel")}</label>
          <input
            id="aw-addr"
            type="text"
            placeholder="0x0000000000000000000000000000000000000000"
            autoComplete="off"
            spellCheck={false}
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            onBlur={() => setTouched(true)}
            style={{ fontFamily: "var(--ff-mono)" }}
          />
          {touched && !addressOk && (
            <div className="err">{t("wallet.addressInvalid")}</div>
          )}
        </div>

        <div className="form-row">
          <label htmlFor="aw-label">{t("wallet.labelOptional")}</label>
          <input
            id="aw-label"
            type="text"
            placeholder={t("wallet.labelPlaceholder")}
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
        </div>

        <div className="form-row">
          <label>{t("wallet.chainsLabel")}</label>
          <div className="chain-grid">
            {CHAIN_OPTIONS.map((c) => (
              <label key={c.id} className={`chain-chip${chains.has(c.id) ? " checked" : ""}`}>
                <input
                  type="checkbox"
                  checked={chains.has(c.id)}
                  onChange={() => toggleChain(c.id)}
                />
                {c.label}
                <span style={{ marginLeft: "auto", fontFamily: "var(--ff-mono)", fontSize: 10.5, color: "var(--slate-400)" }}>
                  {c.id}
                </span>
              </label>
            ))}
          </div>
          <div className="hint">
            <Trans i18nKey="wallet.chainsHint" ns="common" components={{ code: <code /> }} />
          </div>
        </div>

        {mut.error && (
          <div className="err" style={{ marginTop: 8 }}>
            {t("wallet.addFailed")}&nbsp;
            {mut.error instanceof ServerError
              ? `${mut.error.status} ${String(mut.error.body)}`
              : String(mut.error)}
          </div>
        )}
      </form>
    </Modal>
  );
}
