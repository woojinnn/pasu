import { useMemo, useState } from "react";

import { UNCATEGORIZED_PKG, type PackageDef } from "../../../server-api/policy-store";
import type { SaveScope } from "./save-def";

export interface SaveScopeChoice {
  scope: SaveScope;
  packageId: string | "__new__";
  newPackageName?: string;
  applyToNewWallets: boolean;
}

/** 신규 정책 첫 저장의 적용 범위 모달. ps2 호출은 호출측이 수행 — 여기는 입력만 수집. */
export function SaveScopeModal(props: {
  open: boolean;
  policyName: string;
  /** 알려진 지갑 전체(서버 목록 ∪ ps2 지갑, 소문자 주소). */
  wallets: { address: string; label?: string | undefined }[];
  packages: PackageDef[];
  busy: boolean;
  onCancel: () => void;
  onConfirm: (choice: SaveScopeChoice) => void;
}) {
  const { open, policyName, wallets, packages, busy, onCancel, onConfirm } = props;
  // 지갑 범위 = 지갑 전용 정책 — 패키지도 지갑 소속이라 라이브러리 폴더 목록을
  // 보여주지 않는다(미분류 또는 새 지갑 패키지).
  const [mode, setModeRaw] = useState<"wallets" | "all-wallets" | "library-only">(
    wallets.length > 0 ? "wallets" : "library-only",
  );
  const setMode = (m: "wallets" | "all-wallets" | "library-only") => {
    setModeRaw(m);
    setPackageId(UNCATEGORIZED_PKG);
  };
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [packageId, setPackageId] = useState<string | "__new__">(UNCATEGORIZED_PKG);
  const [newPackageName, setNewPackageName] = useState("");
  const [applyToNewWallets, setApplyToNewWallets] = useState(true);

  const allAddresses = useMemo(() => wallets.map((w) => w.address), [wallets]);

  if (!open) return null;

  const togglePick = (addr: string) =>
    setPicked((prev) => {
      const n = new Set(prev);
      if (n.has(addr)) n.delete(addr);
      else n.add(addr);
      return n;
    });

  const invalid =
    (mode === "wallets" && picked.size === 0) ||
    (packageId === "__new__" && !newPackageName.trim());

  const confirm = () => {
    const scope: SaveScope =
      mode === "library-only"
        ? { kind: "library-only" }
        : mode === "all-wallets"
          ? { kind: "all-wallets", addresses: allAddresses }
          : { kind: "wallets", addresses: [...picked] };
    onConfirm({
      scope,
      packageId,
      ...(packageId === "__new__" ? { newPackageName: newPackageName.trim() } : {}),
      applyToNewWallets,
    });
  };

  return (
    <div className="ptm-bd" role="dialog" aria-modal onClick={busy ? undefined : onCancel}>
      <div className="ptm" onClick={(e) => e.stopPropagation()}>
        <div className="ptm-h">
          <div className="ptm-t">어디에 적용할까요?</div>
          <div className="ptm-s">
            <b>{policyName}</b> — 처음 저장하는 정책이에요. 적용 범위를 골라주세요.
          </div>
        </div>
        <div className="ptm-opts">
          <label className="ptm-field">
            <input
              type="radio"
              name="ssm-scope"
              checked={mode === "wallets"}
              disabled={wallets.length === 0}
              onChange={() => setMode("wallets")}
            />
            선택한 지갑에 적용
          </label>
          {mode === "wallets" && (
            <div className="ssm-wallets">
              {wallets.map((w) => (
                <label key={w.address} className="ptm-field">
                  <input
                    type="checkbox"
                    checked={picked.has(w.address)}
                    onChange={() => togglePick(w.address)}
                  />
                  <span className="ssm-addr">{w.label ?? w.address}</span>
                </label>
              ))}
              {wallets.length === 0 && <div className="ssm-none">등록된 지갑이 없어요</div>}
            </div>
          )}
          <label className="ptm-field">
            <input
              type="radio"
              name="ssm-scope"
              checked={mode === "all-wallets"}
              disabled={wallets.length === 0}
              onChange={() => setMode("all-wallets")}
            />
            모든 지갑에 적용 ({wallets.length}개)
          </label>
          <label className="ptm-field">
            <input
              type="radio"
              name="ssm-scope"
              checked={mode === "library-only"}
              onChange={() => setMode("library-only")}
            />
            라이브러리에만 저장 (나중에 적용)
          </label>

          <label className="ptm-field">
            패키지
            <select
              value={packageId}
              onChange={(e) => setPackageId(e.target.value as string | "__new__")}
            >
              {mode === "wallets" ? (
                // 지갑 전용 정책의 패키지는 지갑 소속 — 라이브러리 폴더와 무관.
                <option value={UNCATEGORIZED_PKG}>미분류</option>
              ) : (
                packages.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.displayName}
                  </option>
                ))
              )}
              <option value="__new__">+ 새 패키지…</option>
            </select>
          </label>
          {packageId === "__new__" && (
            <label className="ptm-field">
              <input
                autoFocus
                value={newPackageName}
                onChange={(e) => setNewPackageName(e.target.value)}
                placeholder="새 패키지 이름"
              />
            </label>
          )}

          <label className="ptm-field">
            <input
              type="checkbox"
              checked={applyToNewWallets}
              onChange={(e) => setApplyToNewWallets(e.target.checked)}
            />
            앞으로 추가되는 지갑에도 기본 적용
          </label>

          <div className="ptm-row">
            <button type="button" className="ev2-sec" onClick={onCancel} disabled={busy}>
              취소
            </button>
            <button type="button" className="ev2-pri" onClick={confirm} disabled={invalid || busy}>
              {busy ? "저장 중…" : "저장"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
