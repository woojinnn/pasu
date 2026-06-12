import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  UNCATEGORIZED_PKG,
  type PackageDef,
  type PolicyDef,
  type StoreSnapshot,
} from "../../../server-api/policy-store";
import { defUsageCount } from "./wallet-policies-derive";
import { catKey, catLabel, catStyle, type CategoryKey } from "./categories";
import { mtimeLabel } from "./helpers";
import {
  CaretRightIcon,
  CopyIcon,
  FolderIcon,
  PencilIcon,
  ShieldIcon,
  TrashIcon,
} from "./icons";

/** 라이브러리 정의 드래그 페이로드 — 라이브러리 폴더 드롭 = 소속 변경,
 *  지갑 패키지 드롭 = 그 지갑에 바인딩. */
export const DRAG_DEF_MIME = "application/x-pasu-def-id";

/** def 출처 라벨 키 — t()는 렌더 시점에 호출한다. */
const SOURCE_LABEL_KEY: Record<PolicyDef["source"], string> = {
  builtin: "library.source.builtin",
  mine: "library.source.mine",
  market: "library.source.market",
};

/** 라이브러리의 디렉토리 뷰 — 폴더(라이브러리 패키지) 안에 정의 = 파일. 폴더
 *  멤버십은 `defaults.packageId`(발행·프로비저닝과 같은 "라이브러리 차원 소속" 축).
 *  manage = 라이브러리 탭(전체 액션 + 폴더 간 이동), pick = 지갑 워크스페이스의
 *  드래그 소스(읽기 전용). */
export function LibraryDirectory(props: {
  snap: StoreSnapshot;
  mode: "manage" | "pick";
  query: string;
  catFilter: "all" | CategoryKey;
  onOpenDef?: (d: PolicyDef) => void;
  onDuplicate?: (d: PolicyDef) => void;
  onDelete?: (d: PolicyDef) => void;
  onDefaults?: (d: PolicyDef) => void;
  /** 새 지갑 기본 적용(defaults.enabled) 토글. */
  onToggleDefault?: (d: PolicyDef, enabled: boolean) => void;
  onRenamePackage?: (pkg: PackageDef, name: string) => void;
  onDeletePackage?: (pkg: PackageDef) => void;
  onPublishPackage?: (pkg: PackageDef) => void;
  /** manage 드래그: 정의를 폴더에 놓음 = defaults.packageId 변경. */
  onMoveDef?: (defId: string, packageId: string) => void;
}) {
  const {
    snap, mode, query, catFilter,
    onOpenDef, onDuplicate, onDelete, onDefaults, onToggleDefault,
    onRenamePackage, onDeletePackage, onPublishPackage, onMoveDef,
  } = props;
  const { t } = useTranslation("editor");

  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [draftName, setDraftName] = useState("");

  const packages = useMemo(
    () =>
      Object.values(snap.library.packages).sort((a, b) =>
        a.id === UNCATEGORIZED_PKG ? 1 : b.id === UNCATEGORIZED_PKG ? -1 : a.id.localeCompare(b.id),
      ),
    [snap],
  );

  // 폴더 멤버십 = defaults.packageId (없으면 미분류).
  const byFolder = useMemo(() => {
    const m = new Map<string, PolicyDef[]>();
    const q = query.trim().toLowerCase();
    for (const d of Object.values(snap.library.defs)) {
      if (d.hidden) continue; // 지갑 전용 정책 — 카탈로그 비노출
      if (q && !d.displayName.toLowerCase().includes(q) && !d.id.toLowerCase().includes(q)) continue;
      if (catFilter !== "all" && catKey(d.cat) !== catFilter) continue;
      const raw = d.defaults.packageId;
      const key = raw && snap.library.packages[raw] ? raw : UNCATEGORIZED_PKG;
      const arr = m.get(key) ?? [];
      arr.push(d);
      m.set(key, arr);
    }
    for (const arr of m.values()) arr.sort((a, b) => a.displayName.localeCompare(b.displayName, "ko"));
    return m;
  }, [snap, query, catFilter]);

  const toggleFolder = (id: string) =>
    setCollapsed((prev) => {
      const n = new Set(prev);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const filtering = query.trim().length > 0 || catFilter !== "all";

  return (
    <div className={`ld${mode === "pick" ? " pick" : ""}`}>
      {packages.map((pkg) => {
        const defs = byFolder.get(pkg.id) ?? [];
        // 필터 중에는 결과 없는 폴더를 숨겨 잡음을 줄인다(미분류 포함).
        if (filtering && defs.length === 0) return null;
        const open = !collapsed.has(pkg.id);
        const locked = pkg.id === UNCATEGORIZED_PKG;
        return (
          <div
            key={pkg.id}
            className={`ld-folder${dropTarget === pkg.id ? " droptarget" : ""}`}
            onDragOver={(e) => {
              if (mode === "manage" && onMoveDef && e.dataTransfer.types.includes(DRAG_DEF_MIME)) {
                e.preventDefault();
                setDropTarget(pkg.id);
              }
            }}
            onDragLeave={() => setDropTarget((t) => (t === pkg.id ? null : t))}
            onDrop={(e) => {
              e.preventDefault();
              setDropTarget(null);
              const defId = e.dataTransfer.getData(DRAG_DEF_MIME);
              if (defId && onMoveDef) onMoveDef(defId, pkg.id);
            }}
          >
            <div className="ld-folderhead" onClick={() => toggleFolder(pkg.id)}>
              <span className={`ld-caret${open ? " open" : ""}`}>
                <CaretRightIcon />
              </span>
              <FolderIcon />
              {renaming === pkg.id ? (
                <input
                  autoFocus
                  value={draftName}
                  onClick={(e) => e.stopPropagation()}
                  onChange={(e) => setDraftName(e.target.value)}
                  onBlur={() => {
                    setRenaming(null);
                    if (onRenamePackage) onRenamePackage(pkg, draftName);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                    if (e.key === "Escape") setRenaming(null);
                  }}
                />
              ) : (
                <span className="nm">{pkg.displayName}</span>
              )}
              <span className="cnt">{defs.length}</span>
              {mode === "manage" && (
                <span className="acts" onClick={(e) => e.stopPropagation()}>
                  {onPublishPackage && !locked && (
                    <button type="button" className="ev2-iconbtn" title={t("library.publishFolderTitle")} onClick={() => onPublishPackage(pkg)}>
                      <ShieldIcon />
                    </button>
                  )}
                  {onRenamePackage && !locked && (
                    <button
                      type="button"
                      className="ev2-iconbtn"
                      title={t("library.rename")}
                      onClick={() => {
                        setRenaming(pkg.id);
                        setDraftName(pkg.displayName);
                      }}
                    >
                      <PencilIcon />
                    </button>
                  )}
                  {onDeletePackage && !locked && (
                    <button type="button" className="ev2-iconbtn danger" title={t("common:delete")} onClick={() => onDeletePackage(pkg)}>
                      <TrashIcon />
                    </button>
                  )}
                </span>
              )}
            </div>

            {open && (
              <div className="ld-defs">
                {defs.length === 0 && <div className="ld-empty">{t("library.emptyFolder")}</div>}
                {defs.map((d) => {
                  const cat = catKey(d.cat);
                  const usage = defUsageCount(snap, d.id);
                  return (
                    <div
                      key={d.id}
                      className="ld-def"
                      draggable
                      onDragStart={(e) => {
                        e.dataTransfer.setData(DRAG_DEF_MIME, d.id);
                        e.dataTransfer.effectAllowed = mode === "manage" ? "move" : "copy";
                      }}
                      onClick={() => onOpenDef?.(d)}
                    >
                      <span className="ld-cat" style={{ background: catStyle(cat).hex }} title={catLabel(cat)} />
                      <span className="nm">{d.displayName}</span>
                      <span className="ld-src">{t(SOURCE_LABEL_KEY[d.source])}</span>
                      {mode === "manage" && (
                        <>
                          <span className="ld-meta">
                            {usage > 0 ? t("library.walletUsage", { count: usage }) : ""}
                          </span>
                          {onToggleDefault ? (
                            <button
                              type="button"
                              className={`ld-defaultchip${d.defaults.enabled ? " on" : ""}`}
                              title={t("library.defaultChipTitle")}
                              onClick={(e) => {
                                e.stopPropagation();
                                onToggleDefault(d, !d.defaults.enabled);
                              }}
                            >
                              {d.defaults.enabled ? t("library.defaultOn") : t("library.defaultOff")}
                            </button>
                          ) : (
                            <span className="ld-meta">
                              {d.defaults.enabled ? t("library.defaultApplied") : ""}
                            </span>
                          )}
                          <span className="ld-meta time">{mtimeLabel(d.updatedAtMs, false)}</span>
                          <span className="acts" onClick={(e) => e.stopPropagation()}>
                            {onDefaults && (
                              <button type="button" className="ev2-iconbtn" title={t("library.defaultsTitle")} onClick={() => onDefaults(d)}>
                                <PencilIcon />
                              </button>
                            )}
                            {onDuplicate && (
                              <button type="button" className="ev2-iconbtn" title={t("library.duplicate")} onClick={() => onDuplicate(d)}>
                                <CopyIcon />
                              </button>
                            )}
                            {onDelete && d.source !== "builtin" && (
                              <button type="button" className="ev2-iconbtn danger" title={t("common:delete")} onClick={() => onDelete(d)}>
                                <TrashIcon />
                              </button>
                            )}
                          </span>
                        </>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
