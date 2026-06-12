import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

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
      title={mut.data ? "지갑 추가 결과" : "지갑 추가"}
      footer={
        mut.data ? (
          <button className="btn primary" onClick={onClose}>닫기</button>
        ) : (
          <>
            <button className="btn" type="button" onClick={onClose} disabled={mut.isPending}>
              취소
            </button>
            <button className="btn primary" type="submit" form="add-wallet-form" disabled={mut.isPending || !addressOk}>
              {mut.isPending ? "추가 중…" : "추가"}
            </button>
          </>
        )
      }
    >
      <form id="add-wallet-form" onSubmit={onSubmit}>
        <div className="form-row">
          <label htmlFor="aw-addr">주소 (0x…)</label>
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
            <div className="err">유효한 0x 주소 (40 hex)가 아닙니다</div>
          )}
        </div>

        <div className="form-row">
          <label htmlFor="aw-label">라벨 (선택)</label>
          <input
            id="aw-label"
            type="text"
            placeholder="예: 메인 지갑, Treasury, Hot"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
        </div>

        <div className="form-row">
          <label>체인 (선택 안 하면 서버 설정의 모든 체인)</label>
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
            전체 선택 안 하면: <code>dambi-sync.toml</code>에 RPC가 설정된 모든 체인을 추적합니다.
          </div>
        </div>

        {mut.error && (
          <div className="err" style={{ marginTop: 8 }}>
            추가 실패:&nbsp;
            {mut.error instanceof ServerError
              ? `${mut.error.status} ${String(mut.error.body)}`
              : String(mut.error)}
          </div>
        )}
      </form>
    </Modal>
  );
}
