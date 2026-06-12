import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { UNCATEGORIZED_PKG, type PackageDef } from "../../../server-api/policy-store";
import type { SaveScope } from "./save-def";

/** 지갑 전용 정책의 지갑별 **폴더**(템플릿 정리 축) 결정 — 기존 폴더 id,
 *  미분류 센티널, 또는 "이 이름의 폴더에 넣기"(find-or-create). 적용(패키지)은
 *  저장 후 지갑별 정책에서 drag&drop으로 한다. */
export type WalletFolderPick = { id: string } | { newName: string };

/** 미분류(폴더 없음) 센티널 — walletFolderId=undefined로 번역된다. */
export const WALLET_FOLDER_UNCAT = "__uncat__";

export interface SaveScopeChoice {
  /** 저장할 정책 이름 — 모달에서 확정한다 ("새 정책"인 채 저장되는 일이 없게). */
  name: string;
  scope: SaveScope;
  /** 라이브러리 경로의 폴더(또는 "__new__"). 지갑 경로에서는 무시. */
  packageId: string | "__new__";
  newPackageName?: string;
  /** 지갑 경로: 주소별 전용 폴더 결정(정리 축 — 적용 아님). */
  walletFolders?: Record<string, WalletFolderPick>;
  applyToNewWallets: boolean;
}

export interface ModalWallet {
  address: string;
  label?: string | undefined;
  /** 이 지갑이 이미 가진 전용 폴더들. */
  folders: { id: string; displayName: string }[];
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
  const { t } = useTranslation("editor");
  const [kind, setKind] = useState<Kind | null>(null);
  // 정책 이름 — 헤더 제목을 못 보고 지나친 채 "새 정책"으로 저장되는 일이
  // 잦아서, 첫 저장 모달이 명시적으로 묻는다.
  const [nameDraft, setNameDraft] = useState(policyName);
  useEffect(() => {
    if (open) setNameDraft(policyName);
  }, [open, policyName]);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  // 지갑별 폴더 선택 — 폴더는 지갑 소속이라 지갑마다 따로 고른다.
  const [walletPkg, setWalletPkg] = useState<Record<string, string>>({});
  const [walletNewName, setWalletNewName] = useState<Record<string, string>>({});
  // 일괄 모드: 선택한 지갑 모두에 같은 이름의 새 폴더를 만든다.
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

  // 일괄 이름이 이미 존재하는 지갑 — 그 지갑에서는 기존 폴더를 재사용한다.
  const bulkCollisions = useMemo(() => {
    const name = bulkName.trim();
    if (!bulk || !name) return [];
    return [...picked].filter((a) =>
      (walletByAddr.get(a)?.folders ?? []).some((f) => f.displayName === name),
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

  const pkgOf = (addr: string) => walletPkg[addr] ?? WALLET_FOLDER_UNCAT;

  const invalid =
    !nameDraft.trim() ||
    (kind === "wallet"
      ? picked.size === 0 ||
        (bulk
          ? !bulkName.trim()
          : [...picked].some((a) => pkgOf(a) === "__new__" && !(walletNewName[a] ?? "").trim()))
      : packageId === "__new__" && !newPackageName.trim());

  const confirm = () => {
    if (kind === "wallet") {
      const walletFolders: Record<string, WalletFolderPick> = {};
      for (const addr of picked) {
        if (bulk) {
          walletFolders[addr] = { newName: bulkName.trim() };
        } else {
          const sel = pkgOf(addr);
          walletFolders[addr] =
            sel === "__new__" ? { newName: (walletNewName[addr] ?? "").trim() } : { id: sel };
        }
      }
      onConfirm({
        name: nameDraft.trim(),
        scope: { kind: "wallets", addresses: [...picked] },
        packageId: UNCATEGORIZED_PKG,
        walletFolders,
        // 지갑 전용 정책은 새 지갑 자동 적용 개념이 없다.
        applyToNewWallets: false,
      });
      return;
    }
    onConfirm({
      name: nameDraft.trim(),
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
              <div className="ptm-t">{t("saveScope.title1")}</div>
              <div className="ptm-s">{t("saveScope.sub1")}</div>
            </div>
            <label className="ssm-name">
              <span>{t("saveScope.nameLabel")}</span>
              <input
                autoFocus
                value={nameDraft}
                onChange={(e) => setNameDraft(e.target.value)}
                placeholder={t("saveScope.namePlaceholder")}
                maxLength={120}
              />
            </label>
            <div className="ptm-opts">
              <button
                type="button"
                className="ptm-opt"
                disabled={wallets.length === 0}
                onClick={() => choose("wallet")}
              >
                <span className="ptm-opt-t">{t("saveScope.walletOptTitle")}</span>
                <span className="ptm-opt-d">
                  {t("saveScope.walletOptDesc")}
                  {wallets.length === 0 ? t("saveScope.noWalletsSuffix") : ""}
                </span>
              </button>
              <button type="button" className="ptm-opt" onClick={() => choose("library")}>
                <span className="ptm-opt-t">{t("saveScope.libOptTitle")}</span>
                <span className="ptm-opt-d">{t("saveScope.libOptDesc")}</span>
              </button>
              <div className="ptm-row">
                <button type="button" className="ev2-sec" onClick={onCancel} disabled={busy}>
                  {t("common:cancel")}
                </button>
              </div>
            </div>
          </>
        ) : (
          <>
            <div className="ptm-h">
              <div className="ptm-t">
                {kind === "wallet" ? t("saveScope.title2Wallet") : t("saveScope.title2Library")}
              </div>
              <div className="ptm-s">
                <b>{nameDraft.trim() || policyName}</b> —{" "}
                {kind === "wallet" ? t("saveScope.sub2Wallet") : t("saveScope.sub2Library")}
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
                            <span className="ssm-pkglabel">{t("saveScope.folderLabel")}</span>
                            <select
                              value={pkgOf(w.address)}
                              onChange={(e) =>
                                setWalletPkg((m) => ({ ...m, [w.address]: e.target.value }))
                              }
                            >
                              <option value={WALLET_FOLDER_UNCAT}>{t("uncategorized")}</option>
                              {w.folders.map((f) => (
                                <option key={f.id} value={f.id}>
                                  {f.displayName}
                                </option>
                              ))}
                              <option value="__new__">{t("saveScope.newFolderOption")}</option>
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
                                placeholder={t("saveScope.newFolderPlaceholder")}
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
                    {t("saveScope.bulkLabel")}
                  </label>
                  {bulk && (
                    <>
                      <label className="ptm-field">
                        <input
                          autoFocus
                          value={bulkName}
                          onChange={(e) => setBulkName(e.target.value)}
                          placeholder={t("saveScope.newFolderPlaceholder")}
                        />
                      </label>
                      {bulkCollisions.length > 0 && (
                        <div className="ssm-info">
                          {t("saveScope.bulkCollision", {
                            wallets: bulkCollisions.map(shortAddr).join(", "),
                          })}
                        </div>
                      )}
                    </>
                  )}
                </>
              )}

              {kind === "library" && (
                <>
                  <label className="ptm-field">
                    {t("saveScope.folder")}
                    <select
                      value={packageId}
                      onChange={(e) => setPackageId(e.target.value as string | "__new__")}
                    >
                      {packages.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.displayName}
                        </option>
                      ))}
                      <option value="__new__">{t("saveScope.newFolderOption")}</option>
                    </select>
                  </label>
                  {packageId === "__new__" && (
                    <label className="ptm-field">
                      <input
                        autoFocus
                        value={newPackageName}
                        onChange={(e) => setNewPackageName(e.target.value)}
                        placeholder={t("saveScope.newFolderPlaceholder")}
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
                    {t("saveScope.applyAllNow", { count: wallets.length })}
                  </label>
                  <label className="ptm-field">
                    <input
                      type="checkbox"
                      checked={applyToNewWallets}
                      onChange={(e) => setApplyToNewWallets(e.target.checked)}
                    />
                    {t("saveScope.applyToNew")}
                  </label>
                </>
              )}

              <div className="ptm-row">
                <button type="button" className="ev2-sec" onClick={() => setKind(null)} disabled={busy}>
                  {t("saveScope.back")}
                </button>
                <button type="button" className="ev2-pri" onClick={confirm} disabled={invalid || busy}>
                  {busy ? t("saving") : t("common:save")}
                </button>
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
