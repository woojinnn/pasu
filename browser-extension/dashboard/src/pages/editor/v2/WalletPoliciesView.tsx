import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";

import { useQuery, useQueryClient } from "@tanstack/react-query";

import {
  bindDef,
  getOverview,
  isEffectiveOn,
  putDef,
  putWalletFolder,
  removeBinding,
  removeWalletFolder,
  putWalletPackage,
  removeWalletPackage,
  setPackageEnabled,
  updateBinding,
  UNCATEGORIZED_PKG,
  type Binding,
  type PolicyDef,
  type StoreSnapshot,
  type WalletPolicyState,
} from "../../../server-api/policy-store";
import { listWallets } from "../../../server-api/wallets";
import { useProvisionWallets } from "../../use-provision-wallets";
import { deriveWalletRows, packageDisplayOn } from "./wallet-policies-derive";
import { DRAG_DEF_MIME } from "./LibraryDirectory";
import { catKey, catLabel, catStyle } from "./categories";
import { blocksToText } from "../../../cedar";
import type { PolicyIR } from "../../../cedar/blocks";
import { PublishModal, type PublishSource } from "../PublishModal";
import { CaretRightIcon, CopyIcon, FolderIcon, PencilIcon, PlusIcon, ShieldIcon, TrashIcon } from "./icons";

/** 지갑별 정책 — 좌: 이 지갑의 패키지(추가/이름변경/토글/드롭), 우: 라이브러리
 *  디렉토리 모양의 정책 트리. 각 정책 아래에 "이 지갑에서 들어가 있는 패키지"가
 *  바인딩 줄(on/off)로 쌓인다 — 왼쪽 패키지에 추가될 때마다 한 줄씩. */
export function WalletPoliciesView(props: { onToast: (text: string) => void }) {
  const { onToast } = props;
  const qc = useQueryClient();

  const walletsQ = useQuery({ queryKey: ["wallets"], queryFn: listWallets });
  const overviewQ = useQuery({ queryKey: ["ps2-overview"], queryFn: getOverview });
  const invalidate = () => void qc.invalidateQueries({ queryKey: ["ps2-overview"] });

  // 서버 지갑이 ps2 스토어에 아직 없으면 프로비저닝(멱등) — 홈과 공용 훅.
  useProvisionWallets(
    walletsQ.data ? walletsQ.data.map((w) => w.address) : null,
    overviewQ.data ?? null,
    invalidate,
  );

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
    packages: {},
    packageEnabled: {},
  };
  const walletPkgName = (pid: string) =>
    pid === UNCATEGORIZED_PKG ? "미분류" : (wallet.packages?.[pid]?.displayName ?? pid);

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
        walletPkgName(a.packageId).localeCompare(
          walletPkgName(b.packageId),
          "ko",
        ),
      );
    }
    return m;
  }, [wallet, snap]);

  // 좌측 레일 = 이 지갑의 패키지(지갑 소속 객체) + 미분류(가상). 라이브러리의
  // 폴더와는 별개 — 지갑에서 무엇을 해도 라이브러리에 비치지 않는다.
  const packages = useMemo(() => {
    // 미분류는 가상 그룹 — 실제로 미분류 바인딩이 있을 때만 보인다.
    const hasUncat = Object.values(wallet.bindings).some(
      (b) => b.packageId === UNCATEGORIZED_PKG,
    );
    const list = [
      ...(hasUncat ? [{ id: UNCATEGORIZED_PKG, displayName: "미분류", updatedAtMs: 0 }] : []),
      ...Object.values(wallet.packages ?? {}),
    ];
    return list.sort((a, b) =>
      a.id === UNCATEGORIZED_PKG ? -1 : b.id === UNCATEGORIZED_PKG ? 1 : a.id.localeCompare(b.id),
    );
  }, [wallet]);

  // 우측 트리: 라이브러리 디렉토리 구조(폴더 멤버십 = defaults.packageId).
  const defsByFolder = useMemo(() => {
    const m = new Map<string, PolicyDef[]>();
    for (const d of Object.values(snap.library.defs)) {
      if (d.hidden) continue; // 지갑 전용 정책은 별도 섹션에서
      // 죽은 패키지를 가리키면 미분류로 — 안 그러면 폴더가 안 그려져 정책이
      // 사라져 보인다.
      const raw = d.defaults.packageId;
      const key = raw && snap.library.packages[raw] ? raw : UNCATEGORIZED_PKG;
      const arr = m.get(key) ?? [];
      arr.push(d);
      m.set(key, arr);
    }
    for (const arr of m.values()) arr.sort((a, b) => a.displayName.localeCompare(b.displayName, "ko"));
    return m;
  }, [snap]);

  // 이 지갑 전용 정책(모델 A): homeWallet=이 지갑인 템플릿을 **지갑 전용 폴더**
  // 기준으로 그룹 — 바인딩(적용) 여부와 무관하게 보인다. 인스턴스 축(패키지)과
  // 분리된 정리 축. 키 "__uncat__" = 미분류.
  const walletOnlyByFolder = useMemo(() => {
    const m = new Map<string, PolicyDef[]>();
    for (const d of Object.values(snap.library.defs)) {
      if (d.hidden !== true || d.homeWallet !== address.toLowerCase()) continue;
      const key = d.walletFolderId ?? "__uncat__";
      const arr = m.get(key) ?? [];
      arr.push(d);
      m.set(key, arr);
    }
    for (const arr of m.values()) arr.sort((a, b) => a.displayName.localeCompare(b.displayName, "ko"));
    return m;
  }, [snap, address]);

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

  /** def를 지갑 패키지에 추가(드래그&드롭) = 바인딩 한 줄 추가. */
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
        ok && onToast(`${def.displayName} → ${walletPkgName(pkgId)}`),
    );
  };

  // 지갑 패키지 CRUD — 전부 이 지갑 안에서만 일어난다.
  const createPackage = () =>
    void run("패키지 생성", () =>
      putWalletPackage({
        address,
        pkg: { id: `pkg::${crypto.randomUUID()}`, displayName: "새 패키지" },
      }),
    ).then((ok) => ok && onToast("패키지를 만들었어요 — 이름을 바꿔보세요"));

  const renamePackage = (pkgId: string) => {
    const pkg = wallet.packages?.[pkgId];
    const name = draftName.trim();
    setRenaming(null);
    if (!pkg || !name || name === pkg.displayName) return;
    void run("이름 변경", () =>
      putWalletPackage({ address, pkg: { id: pkgId, displayName: name } }),
    );
  };

  const removePackage = (pkgId: string) => {
    const pkg = wallet.packages?.[pkgId];
    if (!pkg) return;
    const n = Object.values(wallet.bindings).filter((b) => b.packageId === pkgId).length;
    if (
      !window.confirm(
        `"${pkg.displayName}" 패키지를 이 지갑에서 제거할까요?\n안의 정책 인스턴스 ${n}개도 함께 제거돼요. (라이브러리의 폴더·정책은 그대로예요)`,
      )
    )
      return;
    void run("패키지 제거", () => removeWalletPackage({ address, packageId: pkgId })).then(
      (ok) => ok && onToast("이 지갑에서 패키지를 제거했어요"),
    );
  };

  const toggleFolder = (id: string) =>
    setCollapsed((prev) => {
      const n = new Set(prev);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  // 지갑 전용 폴더 목록: 이름순, 미분류는 맨 뒤 — 멤버가 있거나, 폴더가 있어
  // "미분류로 되돌리는" 드롭 대상이 필요할 때 보인다.
  const ownFolderIds = useMemo(() => {
    const ids = Object.values(wallet.folders ?? {})
      .sort((a, b) => a.displayName.localeCompare(b.displayName, "ko"))
      .map((f) => f.id);
    if (walletOnlyByFolder.has("__uncat__") || ids.length > 0) ids.push("__uncat__");
    return ids;
  }, [wallet, walletOnlyByFolder]);

  const createWalletFolder = () =>
    void run("폴더 생성", () =>
      putWalletFolder({
        address,
        folder: { id: `fold::${crypto.randomUUID()}`, displayName: "새 폴더" },
      }),
    ).then((ok) => ok && onToast("폴더를 만들었어요 — 이름을 바꿔보세요"));

  const renameWalletFolderUi = (folderId: string) => {
    const current = wallet.folders?.[folderId]?.displayName ?? "";
    const name = window.prompt("폴더 이름", current)?.trim();
    if (!name || name === current) return;
    void run("폴더 이름 변경", () =>
      putWalletFolder({ address, folder: { id: folderId, displayName: name } }),
    );
  };

  // 지갑 전용 템플릿의 폴더 간 드래그 이동. folderId null = 미분류.
  const [folderDropTarget, setFolderDropTarget] = useState<string | null>(null);
  const moveDefToWalletFolder = (defId: string, folderId: string | null) => {
    const d = snap.library.defs[defId];
    // 라이브러리 정책을 지갑 폴더에 떨어뜨리는 건 의미가 없다 — 전용 템플릿만.
    if (!d || d.hidden !== true || d.homeWallet !== address.toLowerCase()) return;
    if ((d.walletFolderId ?? null) === folderId) return;
    const folderName = folderId ? (wallet.folders?.[folderId]?.displayName ?? folderId) : "미분류";
    void run("폴더 이동", () =>
      putDef({ ...d, walletFolderId: folderId ?? undefined, updatedAtMs: Date.now() }),
    ).then((ok) => ok && onToast(`${d.displayName} → ${folderName}`));
  };

  const deleteWalletFolderUi = (folderId: string) => {
    const name = wallet.folders?.[folderId]?.displayName ?? folderId;
    if (!window.confirm(`"${name}" 폴더를 삭제할까요?\n안의 정책은 미분류로 이동해요(삭제되지 않아요).`))
      return;
    void run("폴더 삭제", () => removeWalletFolder({ address, folderId })).then(
      (ok) => ok && onToast("폴더를 삭제했어요 — 정책은 미분류로 옮겼어요"),
    );
  };

  // 마켓 게시 — 지갑 패키지(보이는 그대로: 바인딩의 def, 중복 제거) 또는 개별
  // 정책을 PublishModal로. 라이브러리 디렉토리의 폴더 발행과 같은 Source 모양.
  const [publishSrc, setPublishSrc] = useState<PublishSource | null>(null);

  const renderMember = async (d: PolicyDef) => ({
    slug: d.id.replace(/^def::/, ""),
    title: d.displayName,
    cedarText: await blocksToText(d.skeleton.ir as PolicyIR),
    manifest: d.skeleton.manifest,
  });

  const publishWalletPackage = async (pkgId: string, members: Binding[]) => {
    const defs = [
      ...new Map(members.map((b) => [b.defId, snap.library.defs[b.defId]])).values(),
    ].filter((d): d is PolicyDef => !!d);
    if (defs.length === 0) {
      onToast("이 패키지에 든 정책이 없어요");
      return;
    }
    try {
      setPublishSrc({
        kind: "package",
        suggestedDisplayName: walletPkgName(pkgId),
        suggestedSlug: pkgId.replace(/^pkg::/, ""),
        members: await Promise.all(defs.map(renderMember)),
      });
    } catch (err) {
      console.error("[v2 apply] publish package render failed:", err);
      onToast("게시 준비에 실패했어요");
    }
  };

  const publishDef = async (d: PolicyDef) => {
    try {
      const m = await renderMember(d);
      setPublishSrc({
        kind: "policy",
        cedarText: m.cedarText,
        manifest: m.manifest,
        suggestedDisplayName: d.displayName,
        suggestedSlug: m.slug,
      });
    } catch (err) {
      console.error("[v2 apply] publish policy render failed:", err);
      onToast("게시 준비에 실패했어요");
    }
  };

  const totalActive = Object.values(wallet.bindings).filter((b) => isEffectiveOn(wallet, b)).length;

  /** 폴더 박스 한 개 — 전용 섹션(지갑 전용 폴더)과 공유 섹션(라이브러리 폴더)이
   *  같은 모양을 공유한다. bindingFilter가 있으면 그 그룹의 바인딩 줄만.
   *  actions = 폴더 헤더의 관리 버튼(이름변경/삭제 — 지갑 전용 폴더만). */
  const renderFolder = (
    folder: { id: string; displayName: string },
    defs: PolicyDef[],
    bindingFilter: ((b: Binding) => boolean) | null,
    opts?: {
      actions?: React.ReactNode;
      showEmpty?: boolean;
      /** 지갑 전용 폴더의 드롭 대상 id — null=미분류, undefined=드롭 비활성. */
      dropFolderId?: string | null;
    },
  ) => {
    if (defs.length === 0 && !opts?.showEmpty) return null;
    const open = !collapsed.has(folder.id);
    const droppable = opts?.dropFolderId !== undefined;
    return (
      <div key={folder.id} className="ld-folder">
        <div
          className={`ld-folderhead${droppable && folderDropTarget === folder.id ? " droptarget" : ""}`}
          onClick={() => toggleFolder(folder.id)}
          onDragOver={
            droppable
              ? (e) => {
                  if (e.dataTransfer.types.includes(DRAG_DEF_MIME)) {
                    e.preventDefault();
                    setFolderDropTarget(folder.id);
                  }
                }
              : undefined
          }
          onDragLeave={
            droppable ? () => setFolderDropTarget((t) => (t === folder.id ? null : t)) : undefined
          }
          onDrop={
            droppable
              ? (e) => {
                  e.preventDefault();
                  setFolderDropTarget(null);
                  const defId = e.dataTransfer.getData(DRAG_DEF_MIME);
                  if (defId) moveDefToWalletFolder(defId, opts?.dropFolderId ?? null);
                }
              : undefined
          }
        >
          <span className={`ld-caret${open ? " open" : ""}`}>
            <CaretRightIcon />
          </span>
          <FolderIcon />
          <span className="nm">{folder.displayName}</span>
          <span className="cnt">{defs.length}</span>
          {opts?.actions && (
            <span className="acts" onClick={(e) => e.stopPropagation()}>
              {opts.actions}
            </span>
          )}
        </div>
        {open && (
          <div className="ld-defs">
            {defs.map((d) => {
              const cat = catKey(d.cat);
              const rows = (bindingsByDef.get(d.id) ?? []).filter(
                (b) =>
                  (scope === "all" || b.packageId === scope) &&
                  (bindingFilter === null || bindingFilter(b)),
              );
              return (
                <div key={d.id} className="wt-def">
                  <div
                    className="ld-def"
                    draggable
                    title="클릭해서 템플릿 편집 · 끌어서 패키지에 적용 / 전용 폴더로 이동"
                    onClick={() => navigate(`/editor/${encodeURIComponent(d.id)}`)}
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
                    <span className={`nm${rows.length === 0 ? " dim" : ""}`}>{d.displayName}</span>
                    <button
                      type="button"
                      className="ev2-iconbtn wt-pub"
                      title="이 정책을 마켓에 게시"
                      onClick={(e) => {
                        e.stopPropagation();
                        void publishDef(d);
                      }}
                    >
                      <ShieldIcon />
                    </button>
                  </div>
                  {rows.map((b) => (
                    <BindingRow
                      key={b.id}
                      binding={b}
                      def={d}
                      wallet={wallet}
                      pkgName={walletPkgName(b.packageId)}
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
  };

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
                      {!empty && (
                        <button
                          type="button"
                          className="ev2-iconbtn"
                          title="이 패키지를 마켓에 게시"
                          onClick={() => void publishWalletPackage(pkg.id, members)}
                        >
                          <ShieldIcon />
                        </button>
                      )}
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
            {scope === "all" ? "전체 정책" : walletPkgName(scope)}
          </span>
        </div>

        <div className="ev2-scroll">
          <div className="ld">
            {(walletOnlyByFolder.size > 0 || Object.keys(wallet.folders ?? {}).length > 0) && (
              <div className="wt-section">
                <div className="wt-section-h">
                  이 지갑 전용 정책
                  <button
                    type="button"
                    className="ev2-iconbtn wt-newfolder"
                    title="새 폴더"
                    onClick={createWalletFolder}
                  >
                    <PlusIcon />
                  </button>
                </div>
                {ownFolderIds.map((fid) => {
                  const all = walletOnlyByFolder.get(fid) ?? [];
                  // 좌측 scope(패키지) 필터: 그 패키지에 인스턴스가 있는 템플릿만.
                  const defs =
                    scope === "all"
                      ? all
                      : all.filter((d) =>
                          (bindingsByDef.get(d.id) ?? []).some((b) => b.packageId === scope),
                        );
                  const isUncat = fid === "__uncat__";
                  return renderFolder(
                    {
                      id: `own:${fid}`,
                      displayName: isUncat ? "미분류" : (wallet.folders?.[fid]?.displayName ?? fid),
                    },
                    defs,
                    null,
                    {
                      showEmpty: scope === "all",
                      dropFolderId: isUncat ? null : fid,
                      actions: isUncat ? undefined : (
                        <>
                          <button
                            type="button"
                            className="ev2-iconbtn"
                            title="폴더 이름 변경"
                            onClick={() => renameWalletFolderUi(fid)}
                          >
                            <PencilIcon />
                          </button>
                          <button
                            type="button"
                            className="ev2-iconbtn danger"
                            title="폴더 삭제 (안의 정책은 미분류로)"
                            onClick={() => deleteWalletFolderUi(fid)}
                          >
                            <TrashIcon />
                          </button>
                        </>
                      ),
                    },
                  );
                })}
              </div>
            )}
            <div className="wt-section">
              <div className="wt-section-h">라이브러리 공유 정책</div>
              {Object.values(snap.library.packages)
                .sort((a, b) =>
                  a.id === UNCATEGORIZED_PKG ? 1 : b.id === UNCATEGORIZED_PKG ? -1 : a.id.localeCompare(b.id),
                )
                .map((folder) => {
                  let defs = defsByFolder.get(folder.id) ?? [];
                  if (scope !== "all") {
                    defs = defs.filter((d) =>
                      (bindingsByDef.get(d.id) ?? []).some((b) => b.packageId === scope),
                    );
                  }
                  return renderFolder(folder, defs, null);
                })}
            </div>
          </div>
        </div>
      </section>

      <PublishModal open={publishSrc !== null} source={publishSrc} onClose={() => setPublishSrc(null)} />
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
