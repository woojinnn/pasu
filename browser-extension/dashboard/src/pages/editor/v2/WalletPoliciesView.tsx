import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import {
  bindDef,
  deletePackage as deletePackageApi,
  getOverview,
  isEffectiveOn,
  provisionWallets,
  putPackage,
  removeBinding,
  setPackageEnabled,
  updateBinding,
  UNCATEGORIZED_PKG,
  type Binding,
  type PolicyDef,
  type StoreSnapshot,
  type WalletPolicyState,
} from "../../../server-api/policy-store";
import { listWallets } from "../../../server-api/wallets";
import { deriveWalletRows, packageDisplayOn } from "./wallet-policies-derive";
import { DRAG_DEF_MIME } from "./LibraryDirectory";
import { catKey, catLabel, catStyle } from "./categories";
import { CaretRightIcon, CopyIcon, FolderIcon, PencilIcon, PlusIcon, TrashIcon } from "./icons";

/** 지갑별 정책 — 좌: 이 지갑의 패키지(추가/이름변경/토글/드롭), 우: 라이브러리
 *  디렉토리 모양의 정책 트리. 각 정책 아래에 "이 지갑에서 들어가 있는 패키지"가
 *  바인딩 줄(on/off)로 쌓인다 — 왼쪽 패키지에 추가될 때마다 한 줄씩. */
export function WalletPoliciesView(props: { onToast: (text: string) => void }) {
  const { onToast } = props;
  const qc = useQueryClient();

  const walletsQ = useQuery({ queryKey: ["wallets"], queryFn: listWallets });
  const overviewQ = useQuery({ queryKey: ["ps2-overview"], queryFn: getOverview });
  const invalidate = () => void qc.invalidateQueries({ queryKey: ["ps2-overview"] });

  // 서버 지갑이 ps2 스토어에 아직 없으면 프로비저닝(멱등).
  const provisioned = useRef(false);
  useEffect(() => {
    if (provisioned.current || !walletsQ.data || !overviewQ.data) return;
    const known = overviewQ.data.wallets.byAddress;
    const missing = walletsQ.data.map((w) => w.address.toLowerCase()).filter((a) => !known[a]);
    provisioned.current = true;
    if (missing.length === 0) return;
    void provisionWallets(missing)
      .then(invalidate)
      .catch((err) => console.warn("[v2 apply] provisioning failed:", err));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [walletsQ.data, overviewQ.data]);

  const snap = overviewQ.data ?? null;
  const rows = useMemo(
    () =>
      snap
        ? deriveWalletRows(
            snap,
            (walletsQ.data ?? []).map((w) => ({ address: w.address })),
          )
        : null,
    [snap, walletsQ.data],
  );

  const [addr, setAddr] = useState<string | null>(null);
  const activeAddr = addr ?? rows?.[0]?.address ?? null;

  if (overviewQ.isLoading || !rows || !snap) {
    return <div className="ev2-status">불러오는 중…</div>;
  }
  if (rows.length === 0) {
    return (
      <div className="ev2-empty">
        <div className="big">등록된 지갑이 없습니다</div>
        <div className="sm">확장 popup에서 지갑을 추가하면 여기에서 정책을 적용할 수 있어요.</div>
      </div>
    );
  }

  return (
    <div className="wd-wrap">
      <div className="wd-modes">
        {activeAddr && (
          <select className="wd-walletsel" value={activeAddr} onChange={(e) => setAddr(e.target.value)}>
            {rows.map((r) => (
              <option key={r.address} value={r.address}>
                {r.label ? `${r.label} (${shortAddr(r.address)})` : shortAddr(r.address)}
              </option>
            ))}
          </select>
        )}
      </div>

      {activeAddr && (
        <WalletWorkspace snap={snap} address={activeAddr} onToast={onToast} invalidate={invalidate} />
      )}
    </div>
  );
}

function shortAddr(a: string): string {
  return a.length > 12 ? `${a.slice(0, 6)}…${a.slice(-4)}` : a;
}

/* ─────────────── 지갑별 워크스페이스 ─────────────── */

function WalletWorkspace(props: {
  snap: StoreSnapshot;
  address: string;
  onToast: (text: string) => void;
  invalidate: () => void;
}) {
  const { snap, address, onToast, invalidate } = props;
  const navigate = useNavigate();
  const wallet: WalletPolicyState = snap.wallets.byAddress[address] ?? {
    bindings: {},
    packageEnabled: {},
  };

  const [scope, setScope] = useState<string | "all">("all");
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [draftName, setDraftName] = useState("");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const run = async (label: string, fn: () => Promise<unknown>): Promise<boolean> => {
    try {
      await fn();
      invalidate();
      return true;
    } catch (err) {
      console.error(`[v2 apply] ${label} failed:`, err);
      onToast(`${label}에 실패했어요`);
      return false;
    }
  };

  // 지갑 패키지별 멤버 바인딩.
  const membersByPkg = useMemo(() => {
    const m = new Map<string, Binding[]>();
    for (const b of Object.values(wallet.bindings)) {
      const arr = m.get(b.packageId) ?? [];
      arr.push(b);
      m.set(b.packageId, arr);
    }
    return m;
  }, [wallet]);

  // def별 이 지갑의 바인딩(우측 트리의 sub-row들).
  const bindingsByDef = useMemo(() => {
    const m = new Map<string, Binding[]>();
    for (const b of Object.values(wallet.bindings)) {
      const arr = m.get(b.defId) ?? [];
      arr.push(b);
      m.set(b.defId, arr);
    }
    for (const arr of m.values()) {
      arr.sort((a, b) =>
        (snap.library.packages[a.packageId]?.displayName ?? "").localeCompare(
          snap.library.packages[b.packageId]?.displayName ?? "",
          "ko",
        ),
      );
    }
    return m;
  }, [wallet, snap]);

  const packages = useMemo(
    () =>
      Object.values(snap.library.packages).sort((a, b) =>
        a.id === UNCATEGORIZED_PKG ? -1 : b.id === UNCATEGORIZED_PKG ? 1 : a.id.localeCompare(b.id),
      ),
    [snap],
  );

  // 우측 트리: 라이브러리 디렉토리 구조(폴더 멤버십 = defaults.packageId).
  const defsByFolder = useMemo(() => {
    const m = new Map<string, PolicyDef[]>();
    for (const d of Object.values(snap.library.defs)) {
      const key = d.defaults.packageId ?? UNCATEGORIZED_PKG;
      const arr = m.get(key) ?? [];
      arr.push(d);
      m.set(key, arr);
    }
    for (const arr of m.values()) arr.sort((a, b) => a.displayName.localeCompare(b.displayName, "ko"));
    return m;
  }, [snap]);

  /** 하이브리드 토글: 켜기 = 게이트 on + (전부 꺼져 있으면) 멤버 일괄 on;
   *  끄기 = 게이트 off(부분 상태 보존). */
  const togglePackage = (pkgId: string, members: Binding[], displayedOn: boolean) =>
    void run("패키지 토글", async () => {
      if (displayedOn) {
        await setPackageEnabled({ address, packageId: pkgId, enabled: false });
        return;
      }
      await setPackageEnabled({ address, packageId: pkgId, enabled: true });
      if (members.length > 0 && !members.some((b) => b.enabled)) {
        for (const b of members) {
          await updateBinding({ address, bindingId: b.id, patch: { enabled: true } });
        }
      }
    });

  /** def를 지갑 패키지에 추가(드롭/+) = 바인딩 한 줄 추가. */
  const addDefToPackage = (defId: string, pkgId: string) => {
    const def = snap.library.defs[defId];
    if (!def) return;
    if ((bindingsByDef.get(defId) ?? []).some((b) => b.packageId === pkgId)) {
      onToast("이미 이 패키지에 들어 있어요");
      return;
    }
    void run("정책 적용", () =>
      bindDef({
        defId,
        packageId: pkgId,
        addresses: [address],
        ...(Object.keys(def.defaults.params).length ? { params: def.defaults.params } : {}),
      }),
    ).then(
      (ok) =>
        ok && onToast(`${def.displayName} → ${snap.library.packages[pkgId]?.displayName ?? pkgId}`),
    );
  };

  // 지갑 화면에서의 패키지 추가/이름변경/삭제 — 패키지는 계정 라이브러리 객체지만
  // "이 지갑에 존재"는 바인딩이 만든다(추가 직후엔 빈 폴더로 흐리게 보임).
  const createPackage = () =>
    void run("패키지 생성", () =>
      putPackage({
        id: `pkg::${crypto.randomUUID()}`,
        displayName: "새 패키지",
        source: "mine",
        updatedAtMs: Date.now(),
      }),
    ).then((ok) => ok && onToast("패키지를 만들었어요 — 이름을 바꿔보세요"));

  const renamePackage = (pkgId: string) => {
    const pkg = snap.library.packages[pkgId];
    const name = draftName.trim();
    setRenaming(null);
    if (!pkg || !name || name === pkg.displayName) return;
    void run("이름 변경", () => putPackage({ ...pkg, displayName: name, updatedAtMs: Date.now() }));
  };

  const removePackage = (pkgId: string) => {
    const pkg = snap.library.packages[pkgId];
    if (!pkg) return;
    if (
      !window.confirm(
        `패키지 "${pkg.displayName}"를 삭제할까요?\n(계정 전체에서 삭제되고, 안의 정책 인스턴스는 '미분류'로 이동해요)`,
      )
    )
      return;
    void run("패키지 삭제", () => deletePackageApi(pkgId)).then(
      (ok) => ok && onToast("패키지를 삭제했어요"),
    );
  };

  const toggleFolder = (id: string) =>
    setCollapsed((prev) => {
      const n = new Set(prev);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const totalActive = Object.values(wallet.bindings).filter((b) => isEffectiveOn(wallet, b)).length;
  const scopePkgId = scope === "all" ? UNCATEGORIZED_PKG : scope;

  return (
    <div className="ev2-2col">
      <aside className="ev2-left">
        <div className="ev2-leftsec">
          <div className="ev2-lefthead">
            <span>이 지갑의 패키지</span>
            <button type="button" className="ev2-iconbtn" title="새 패키지" onClick={createPackage}>
              <PlusIcon />
            </button>
          </div>
          <div className="ev2-pkglist">
            <button
              type="button"
              className={`ev2-pkgrow wd-scope${scope === "all" ? " on" : ""}`}
              onClick={() => setScope("all")}
            >
              <span className="nm">전체 정책</span>
              <span className="cnt">
                {totalActive}/{Object.keys(wallet.bindings).length}
              </span>
            </button>
            {packages.map((pkg) => {
              const members = membersByPkg.get(pkg.id) ?? [];
              const active = members.filter((b) => isEffectiveOn(wallet, b)).length;
              const displayedOn = packageDisplayOn(
                wallet.packageEnabled[pkg.id] ?? true,
                members.filter((b) => b.enabled).length,
              );
              const empty = members.length === 0;
              const locked = pkg.id === UNCATEGORIZED_PKG;
              return (
                <div
                  key={pkg.id}
                  className={`ev2-pkgrow wd-scope${scope === pkg.id ? " on" : ""}${empty ? " dim" : ""}${dropTarget === pkg.id ? " droptarget" : ""}`}
                  onClick={() => setScope(pkg.id)}
                  onDragOver={(e) => {
                    if (e.dataTransfer.types.includes(DRAG_DEF_MIME)) {
                      e.preventDefault();
                      setDropTarget(pkg.id);
                    }
                  }}
                  onDragLeave={() => setDropTarget((t) => (t === pkg.id ? null : t))}
                  onDrop={(e) => {
                    e.preventDefault();
                    setDropTarget(null);
                    const defId = e.dataTransfer.getData(DRAG_DEF_MIME);
                    if (defId) addDefToPackage(defId, pkg.id);
                  }}
                >
                  <FolderIcon />
                  {renaming === pkg.id ? (
                    <input
                      autoFocus
                      value={draftName}
                      onClick={(e) => e.stopPropagation()}
                      onChange={(e) => setDraftName(e.target.value)}
                      onBlur={() => renamePackage(pkg.id)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                        if (e.key === "Escape") setRenaming(null);
                      }}
                    />
                  ) : (
                    <span className="nm">{pkg.displayName}</span>
                  )}
                  <span className="cnt">{empty ? "–" : `${active}/${members.length}`}</span>
                  {!locked && (
                    <span className="acts" onClick={(e) => e.stopPropagation()}>
                      <button
                        type="button"
                        className="ev2-iconbtn"
                        title="이름 변경"
                        onClick={() => {
                          setRenaming(pkg.id);
                          setDraftName(pkg.displayName);
                        }}
                      >
                        <PencilIcon />
                      </button>
                      <button
                        type="button"
                        className="ev2-iconbtn danger"
                        title="삭제"
                        onClick={() => removePackage(pkg.id)}
                      >
                        <TrashIcon />
                      </button>
                    </span>
                  )}
                  {!empty && (
                    <label
                      className="pm-switch sm"
                      title="패키지 정책 전체 켜기/끄기"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <input
                        type="checkbox"
                        checked={displayedOn}
                        onChange={() => togglePackage(pkg.id, members, displayedOn)}
                      />
                      <span className="trk" />
                    </label>
                  )}
                </div>
              );
            })}
          </div>
          <div className="ev2-lefthint">
            오른쪽 정책을 끌어다 패키지에 놓으면 이 지갑에 적용돼요 — 정책 아래에 패키지별
            줄이 하나씩 쌓여요.
          </div>
        </div>
      </aside>

      <section className="ev2-right">
        <div className="ev2-ctrl">
          <span className="wd-scopelabel">
            {scope === "all" ? "전체 정책" : (snap.library.packages[scope]?.displayName ?? scope)}
          </span>
        </div>

        <div className="ev2-scroll">
          <div className="ld">
            {packages
              .slice()
              .sort((a, b) =>
                a.id === UNCATEGORIZED_PKG ? 1 : b.id === UNCATEGORIZED_PKG ? -1 : a.id.localeCompare(b.id),
              )
              .map((folder) => {
                let defs = defsByFolder.get(folder.id) ?? [];
                // 스코프(지갑 패키지) 선택 시: 그 패키지에 바인딩된 정책만.
                if (scope !== "all") {
                  defs = defs.filter((d) =>
                    (bindingsByDef.get(d.id) ?? []).some((b) => b.packageId === scope),
                  );
                }
                if (defs.length === 0) return null;
                const open = !collapsed.has(folder.id);
                return (
                  <div key={folder.id} className="ld-folder">
                    <div className="ld-folderhead" onClick={() => toggleFolder(folder.id)}>
                      <span className={`ld-caret${open ? " open" : ""}`}>
                        <CaretRightIcon />
                      </span>
                      <FolderIcon />
                      <span className="nm">{folder.displayName}</span>
                      <span className="cnt">{defs.length}</span>
                    </div>
                    {open && (
                      <div className="ld-defs">
                        {defs.map((d) => {
                          const cat = catKey(d.cat);
                          const rows = (bindingsByDef.get(d.id) ?? []).filter(
                            (b) => scope === "all" || b.packageId === scope,
                          );
                          return (
                            <div key={d.id} className="wt-def">
                              <div
                                className="ld-def"
                                draggable
                                onDragStart={(e) => {
                                  e.dataTransfer.setData(DRAG_DEF_MIME, d.id);
                                  e.dataTransfer.effectAllowed = "copy";
                                }}
                              >
                                <span
                                  className="ld-cat"
                                  style={{ background: catStyle(cat).hex }}
                                  title={catLabel(cat)}
                                />
                                <span className={`nm${rows.length === 0 ? " dim" : ""}`}>
                                  {d.displayName}
                                </span>
                                <span className="acts">
                                  <button
                                    type="button"
                                    className="ev2-iconbtn"
                                    title={`${snap.library.packages[scopePkgId]?.displayName ?? "미분류"}에 추가`}
                                    onClick={() => addDefToPackage(d.id, scopePkgId)}
                                  >
                                    <PlusIcon />
                                  </button>
                                </span>
                              </div>
                              {rows.map((b) => (
                                <BindingRow
                                  key={b.id}
                                  binding={b}
                                  def={d}
                                  wallet={wallet}
                                  pkgName={snap.library.packages[b.packageId]?.displayName ?? b.packageId}
                                  onOpen={() =>
                                    navigate(
                                      `/editor/${encodeURIComponent(d.id)}?wallet=${address}&binding=${encodeURIComponent(b.id)}`,
                                    )
                                  }
                                  onRun={run}
                                  address={address}
                                />
                              ))}
                            </div>
                          );
                        })}
                      </div>
                    )}
                  </div>
                );
              })}
          </div>
        </div>
      </section>
    </div>
  );
}

/** 한 바인딩 줄: 소속 패키지 · 별칭(인라인 편집) · 파라미터(있으면 확장) ·
 *  복제 · 토글 · 제거. 별칭/파라미터가 "지갑별로 서로 다른 정책"을 만든다. */
function BindingRow(props: {
  binding: Binding;
  def: PolicyDef;
  wallet: WalletPolicyState;
  pkgName: string;
  address: string;
  onOpen: () => void;
  onRun: (label: string, fn: () => Promise<unknown>) => Promise<boolean>;
}) {
  const { binding: b, def, wallet, pkgName, address, onOpen, onRun } = props;
  const pkgOn = wallet.packageEnabled[b.packageId] ?? true;
  const effective = isEffectiveOn(wallet, b);
  const [editingAlias, setEditingAlias] = useState(false);
  const [aliasDraft, setAliasDraft] = useState(b.alias ?? "");

  const saveAlias = () => {
    setEditingAlias(false);
    const alias = aliasDraft.trim();
    if ((b.alias ?? "") === alias) return;
    void onRun("별칭 저장", () =>
      updateBinding({ address, bindingId: b.id, patch: { alias: alias || undefined } }),
    );
  };

  const duplicate = () =>
    void onRun("복제", () =>
      bindDef({
        defId: b.defId,
        packageId: b.packageId,
        addresses: [address],
        ...(b.params ? { params: b.params } : {}),
        alias: `${b.alias ?? def.displayName} (복사)`,
      }),
    );

  return (
    <div className={`wt-binding${effective ? "" : " off"}`}>
      <div
        className="wt-binding-main clickable"
        title="이 지갑 인스턴스 편집 — 값을 바꾸면 이 지갑에만 적용돼요"
        onClick={(ev) => {
          if ((ev.target as HTMLElement).closest("button, input, label, select")) return;
          onOpen();
        }}
      >
        <span className="wt-pkg">
          {pkgName}
          {!pkgOn && <span className="wt-pkgoff">패키지 꺼짐</span>}
        </span>
        {editingAlias ? (
          <input
            className="wt-alias-input"
            autoFocus
            value={aliasDraft}
            placeholder={def.displayName}
            onChange={(e) => setAliasDraft(e.target.value)}
            onBlur={saveAlias}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
              if (e.key === "Escape") setEditingAlias(false);
            }}
          />
        ) : (
          <button
            type="button"
            className={`wt-alias${b.alias ? "" : " empty"}`}
            title="이 지갑에서 부를 이름(별칭) 바꾸기"
            onClick={() => {
              setAliasDraft(b.alias ?? "");
              setEditingAlias(true);
            }}
          >
            {b.alias ?? "별칭 없음"}
            <PencilIcon />
          </button>
        )}
        <button type="button" className="ev2-iconbtn" title="이 지갑에 복제" onClick={duplicate}>
          <CopyIcon />
        </button>
        <label className="pm-switch sm" title="이 정책만 켜기/끄기">
          <input
            type="checkbox"
            checked={b.enabled}
            onChange={(e) =>
              void onRun("토글", () =>
                updateBinding({ address, bindingId: b.id, patch: { enabled: e.target.checked } }),
              )
            }
          />
          <span className="trk" />
        </label>
        <button
          type="button"
          className="ev2-iconbtn danger"
          title="이 패키지에서 제거"
          onClick={() => void onRun("제거", () => removeBinding({ address, bindingId: b.id }))}
        >
          <TrashIcon />
        </button>
      </div>
    </div>
  );
}
