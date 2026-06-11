import { useMemo, useState } from "react";

import { UNCATEGORIZED_PKG, type PackageDef } from "../../../server-api/policy-store";
import type { SaveScope } from "./save-def";

/** 지갑 전용 정책의 지갑별 패키지 결정 — 기존 패키지 id 또는 "이 이름의 패키지에
 *  넣기"(find-or-create: 같은 이름이 이미 있으면 재사용, 없으면 생성). */
export type WalletPkgPick = { id: string } | { newName: string };

export interface SaveScopeChoice {
  scope: SaveScope;
  /** 라이브러리 경로의 폴더(또는 "__new__"). 지갑 경로에서는 무시. */
  packageId: string | "__new__";
  newPackageName?: string;
  /** 지갑 경로: 주소별 패키지 결정. */
  walletPackages?: Record<string, WalletPkgPick>;
  applyToNewWallets: boolean;
}

export interface ModalWallet {
  address: string;
  label?: string | undefined;
  /** 이 지갑이 이미 가진 패키지(지갑 소속 객체). */
  packages: { id: string; displayName: string }[];
}

type Kind = "wallet" | "library";

function shortAddr(a: string): string {
  return a.length > 12 ? `${a.slice(0, 6)}…${a.slice(-4)}` : a;
}

/** 신규 정책 첫 저장 모달 — 2단계. ① 지갑 전용 정책 vs 라이브러리 정책,
 *  ② 세부 설정(지갑별 패키지 / 폴더+기본 적용). ps2 호출은 호출측이 수행. */
export function SaveScopeModal(props: {
  open: boolean;
  policyName: string;
  /** 알려진 지갑 전체(서버 목록 ∪ ps2 지갑, 소문자 주소). */
  wallets: ModalWallet[];
  /** 라이브러리 폴더 목록. */
  packages: PackageDef[];
  busy: boolean;
  onCancel: () => void;
  onConfirm: (choice: SaveScopeChoice) => void;
}) {
  const { open, policyName, wallets, packages, busy, onCancel, onConfirm } = props;
  const [kind, setKind] = useState<Kind | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  // 지갑별 패키지 선택 — 패키지는 지갑 소속이라 지갑마다 따로 고른다.
  const [walletPkg, setWalletPkg] = useState<Record<string, string>>({});
  const [walletNewName, setWalletNewName] = useState<Record<string, string>>({});
  // 일괄 모드: 선택한 지갑 모두에 같은 이름의 새 패키지를 만들어 넣는다.
  const [bulk, setBulk] = useState(false);
  const [bulkName, setBulkName] = useState("");
  // 라이브러리 경로.
  const [packageId, setPackageId] = useState<string | "__new__">(UNCATEGORIZED_PKG);
  const [newPackageName, setNewPackageName] = useState("");
  const [applyToNewWallets, setApplyToNewWallets] = useState(true);
  const [applyToAllNow, setApplyToAllNow] = useState(false);

  const allAddresses = useMemo(() => wallets.map((w) => w.address), [wallets]);
  const walletByAddr = useMemo(
    () => new Map(wallets.map((w) => [w.address, w])),
    [wallets],
  );

  // 일괄 이름이 이미 존재하는 지갑 — 그 지갑에서는 기존 패키지를 재사용한다.
  const bulkCollisions = useMemo(() => {
    const name = bulkName.trim();
    if (!bulk || !name) return [];
    return [...picked].filter((a) =>
      (walletByAddr.get(a)?.packages ?? []).some((p) => p.displayName === name),
    );
  }, [bulk, bulkName, picked, walletByAddr]);

  if (!open) return null;

  const choose = (k: Kind) => {
    setKind(k);
    setPackageId(UNCATEGORIZED_PKG);
    setNewPackageName("");
  };

  const togglePick = (addr: string) =>
    setPicked((prev) => {
      const n = new Set(prev);
      if (n.has(addr)) n.delete(addr);
      else n.add(addr);
      return n;
    });

  const pkgOf = (addr: string) => walletPkg[addr] ?? UNCATEGORIZED_PKG;

  const invalid =
    kind === "wallet"
      ? picked.size === 0 ||
        (bulk
          ? !bulkName.trim()
          : [...picked].some((a) => pkgOf(a) === "__new__" && !(walletNewName[a] ?? "").trim()))
      : packageId === "__new__" && !newPackageName.trim();

  const confirm = () => {
    if (kind === "wallet") {
      const walletPackages: Record<string, WalletPkgPick> = {};
      for (const addr of picked) {
        if (bulk) {
          walletPackages[addr] = { newName: bulkName.trim() };
        } else {
          const sel = pkgOf(addr);
          walletPackages[addr] =
            sel === "__new__" ? { newName: (walletNewName[addr] ?? "").trim() } : { id: sel };
        }
      }
      onConfirm({
        scope: { kind: "wallets", addresses: [...picked] },
        packageId: UNCATEGORIZED_PKG,
        walletPackages,
        // 지갑 전용 정책은 새 지갑 자동 적용 개념이 없다.
        applyToNewWallets: false,
      });
      return;
    }
    onConfirm({
      scope: applyToAllNow
        ? { kind: "all-wallets", addresses: allAddresses }
        : { kind: "library-only" },
      packageId,
      ...(packageId === "__new__" ? { newPackageName: newPackageName.trim() } : {}),
      applyToNewWallets,
    });
  };

  return (
    <div className="ptm-bd" role="dialog" aria-modal onClick={busy ? undefined : onCancel}>
      <div className="ptm" onClick={(e) => e.stopPropagation()}>
        {kind === null ? (
          <>
            <div className="ptm-h">
              <div className="ptm-t">어떤 정책으로 저장할까요?</div>
              <div className="ptm-s">
                <b>{policyName}</b> — 처음 저장하는 정책이에요.
              </div>
            </div>
            <div className="ptm-opts">
              <button
                type="button"
                className="ptm-opt"
                disabled={wallets.length === 0}
                onClick={() => choose("wallet")}
              >
                <span className="ptm-opt-t">지갑 전용 정책</span>
                <span className="ptm-opt-d">
                  선택한 지갑에만 존재해요 — 라이브러리에는 보이지 않아요.
                  {wallets.length === 0 ? " (등록된 지갑이 없어요)" : ""}
                </span>
              </button>
              <button type="button" className="ptm-opt" onClick={() => choose("library")}>
                <span className="ptm-opt-t">라이브러리 정책</span>
                <span className="ptm-opt-d">
                  지갑 간 공유되는 템플릿으로 저장돼요 — 지갑별 정책에서 언제든 적용할 수 있어요.
                </span>
              </button>
              <div className="ptm-row">
                <button type="button" className="ev2-sec" onClick={onCancel} disabled={busy}>
                  취소
                </button>
              </div>
            </div>
          </>
        ) : (
          <>
            <div className="ptm-h">
              <div className="ptm-t">
                {kind === "wallet" ? "어느 지갑에 적용할까요?" : "라이브러리 설정"}
              </div>
              <div className="ptm-s">
                <b>{policyName}</b> —{" "}
                {kind === "wallet"
                  ? "선택한 지갑에만 저장돼요. 패키지는 지갑마다 따로 골라요."
                  : "라이브러리에 템플릿으로 저장돼요."}
              </div>
            </div>
            <div className="ptm-opts">
              {kind === "wallet" && (
                <>
                  <div className="ssm-wallets">
                    {wallets.map((w) => (
                      <div key={w.address}>
                        <label className="ptm-field">
                          <input
                            type="checkbox"
                            checked={picked.has(w.address)}
                            onChange={() => togglePick(w.address)}
                          />
                          <span className="ssm-addr">{w.label ?? w.address}</span>
                        </label>
                        {picked.has(w.address) && !bulk && (
                          <div className="ssm-pkgrow">
                            <span className="ssm-pkglabel">패키지</span>
                            <select
                              value={pkgOf(w.address)}
                              onChange={(e) =>
                                setWalletPkg((m) => ({ ...m, [w.address]: e.target.value }))
                              }
                            >
                              <option value={UNCATEGORIZED_PKG}>미분류</option>
                              {w.packages.map((p) => (
                                <option key={p.id} value={p.id}>
                                  {p.displayName}
                                </option>
                              ))}
                              <option value="__new__">+ 새 패키지…</option>
                            </select>
                            {pkgOf(w.address) === "__new__" && (
                              <input
                                value={walletNewName[w.address] ?? ""}
                                onChange={(e) =>
                                  setWalletNewName((m) => ({
                                    ...m,
                                    [w.address]: e.target.value,
                                  }))
                                }
                                placeholder="새 패키지 이름"
                              />
                            )}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>

                  <label className="ptm-field">
                    <input
                      type="checkbox"
                      checked={bulk}
                      onChange={(e) => {
                        setBulk(e.target.checked);
                        // 일괄 모드를 켜면 모든 지갑을 선택해 준다(편의 기능).
                        if (e.target.checked) setPicked(new Set(allAddresses));
                      }}
                    />
                    모든 지갑에 새 패키지를 만들어 넣기
                  </label>
                  {bulk && (
                    <>
                      <label className="ptm-field">
                        <input
                          autoFocus
                          value={bulkName}
                          onChange={(e) => setBulkName(e.target.value)}
                          placeholder="새 패키지 이름"
                        />
                      </label>
                      {bulkCollisions.length > 0 && (
                        <div className="ssm-info">
                          같은 이름의 패키지가 이미 있는 지갑은 그 패키지에 넣어요:{" "}
                          {bulkCollisions.map(shortAddr).join(", ")}
                        </div>
                      )}
                    </>
                  )}
                </>
              )}

              {kind === "library" && (
                <>
                  <label className="ptm-field">
                    폴더
                    <select
                      value={packageId}
                      onChange={(e) => setPackageId(e.target.value as string | "__new__")}
                    >
                      {packages.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.displayName}
                        </option>
                      ))}
                      <option value="__new__">+ 새 폴더…</option>
                    </select>
                  </label>
                  {packageId === "__new__" && (
                    <label className="ptm-field">
                      <input
                        autoFocus
                        value={newPackageName}
                        onChange={(e) => setNewPackageName(e.target.value)}
                        placeholder="새 폴더 이름"
                      />
                    </label>
                  )}
                  <label className="ptm-field">
                    <input
                      type="checkbox"
                      checked={applyToAllNow}
                      disabled={wallets.length === 0}
                      onChange={(e) => setApplyToAllNow(e.target.checked)}
                    />
                    지금 모든 지갑에 적용 ({wallets.length}개)
                  </label>
                  <label className="ptm-field">
                    <input
                      type="checkbox"
                      checked={applyToNewWallets}
                      onChange={(e) => setApplyToNewWallets(e.target.checked)}
                    />
                    앞으로 추가되는 지갑에도 기본 적용
                  </label>
                </>
              )}

              <div className="ptm-row">
                <button type="button" className="ev2-sec" onClick={() => setKind(null)} disabled={busy}>
                  ← 이전
                </button>
                <button type="button" className="ev2-pri" onClick={confirm} disabled={invalid || busy}>
                  {busy ? "저장 중…" : "저장"}
                </button>
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
