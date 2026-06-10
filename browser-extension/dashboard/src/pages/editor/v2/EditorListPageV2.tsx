import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";

import { subscribeToBroadcast } from "../../../server-api";
import {
  UNCATEGORIZED_PKG,
  deleteDef,
  deletePackage as deletePackageApi,
  duplicateDef,
  getOverview,
  putDef,
  putPackage,
  type PackageDef,
  type PolicyDef,
  type StoreSnapshot,
} from "../../../server-api/policy-store";
import { defUsageCount } from "./apply-matrix-derive";
import { Topbar } from "../../../shell/Topbar";
import { NewPolicyChooser } from "./NewPolicyChooser";
import { CAT_ORDER, catKey, catLabel, catStyle, type CategoryKey } from "./categories";
import {
  CopyIcon,
  FolderIcon,
  PencilIcon,
  PlusIcon,
  SearchIcon,
  TrashIcon,
} from "./icons";
import { mtimeLabel } from "./helpers";

import "./editor-v2.css";

type CatFilter = "all" | CategoryKey;
type EditorTab = "library" | "apply";

interface ToastMsg {
  id: number;
  text: string;
}

/**
 * /editor — 정책 스토리지 v2의 대시보드 진입점. 두 탭:
 *  - 라이브러리: 계정의 정의(PolicyDef)·패키지(PackageDef) 관리. 지갑 적용과는
 *    분리된 "정의" 차원만 다룬다(켜기/끄기 없음 — 그것은 지갑×바인딩의 일).
 *  - 적용 현황: 지갑×패키지 매트릭스(바인딩 토글·파라미터·복사/이동).
 * 모든 데이터는 ps2:get-overview 한 번으로 읽고, 변이 후 invalidate로 재조회한다.
 */
export function EditorListPageV2() {
  const qc = useQueryClient();
  const [sp, setSp] = useSearchParams();
  const tab: EditorTab = sp.get("tab") === "apply" ? "apply" : "library";
  const setTab = (t: EditorTab) =>
    setSp(t === "apply" ? { tab: "apply" } : {}, { replace: true });

  const overviewQ = useQuery({ queryKey: ["ps2-overview"], queryFn: getOverview });
  const invalidate = () => void qc.invalidateQueries({ queryKey: ["ps2-overview"] });

  useEffect(() => {
    const unsubscribe = subscribeToBroadcast((keys) => {
      const touched =
        keys.some((k) => k.startsWith("ps2:")) || keys.includes("dashboard:current-user-id");
      if (touched) void qc.invalidateQueries({ queryKey: ["ps2-overview"] });
    });
    return unsubscribe;
  }, [qc]);

  const [toasts, setToasts] = useState<ToastMsg[]>([]);
  const pushToast = (text: string) => {
    const id = Date.now() + Math.floor(Math.random() * 1000);
    setToasts((t) => [...t, { id, text }]);
    window.setTimeout(() => setToasts((t) => t.filter((m) => m.id !== id)), 2400);
  };

  const [chooserOpen, setChooserOpen] = useState(false);

  const snap = overviewQ.data ?? null;
  const defCount = snap ? Object.keys(snap.library.defs).length : null;
  const pkgCount = snap ? Object.keys(snap.library.packages).length : null;

  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle={defCount === null ? "…" : `정책 ${defCount}개 · 패키지 ${pkgCount}개`}
        right={
          <button type="button" className="ev2-pri" onClick={() => setChooserOpen(true)}>
            <PlusIcon />새 정책
          </button>
        }
      />

      <div className="ev2-body">
        <div className="ev2-pagetabs" role="tablist" aria-label="에디터 영역">
          <button
            type="button"
            role="tab"
            aria-selected={tab === "library"}
            className={`ev2-tab${tab === "library" ? " on" : ""}`}
            onClick={() => setTab("library")}
          >
            라이브러리
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === "apply"}
            className={`ev2-tab${tab === "apply" ? " on" : ""}`}
            onClick={() => setTab("apply")}
          >
            적용 현황
          </button>
        </div>

        {overviewQ.isLoading && <div className="ev2-status">불러오는 중…</div>}
        {overviewQ.isError && (
          <div className="ev2-status">
            정책 스토어를 읽지 못했어요 — 확장이 설치/활성화되어 있는지 확인해 주세요.
          </div>
        )}

        {snap && tab === "library" && (
          <LibraryTab snap={snap} onToast={pushToast} invalidate={invalidate} />
        )}
        {snap && tab === "apply" && <div className="ev2-status">적용 현황 — 준비 중</div>}
      </div>

      <ToastStack toasts={toasts} />
      <NewPolicyChooser open={chooserOpen} onClose={() => setChooserOpen(false)} />
    </>
  );
}

/* ─────────────── 라이브러리 탭 ─────────────── */

function LibraryTab(props: {
  snap: StoreSnapshot;
  onToast: (text: string) => void;
  invalidate: () => void;
}) {
  const { snap, onToast, invalidate } = props;
  const navigate = useNavigate();

  const [query, setQuery] = useState("");
  const [catFilter, setCatFilter] = useState<CatFilter>("all");
  const [defaultsFor, setDefaultsFor] = useState<PolicyDef | null>(null);

  const defs = useMemo(
    () =>
      Object.values(snap.library.defs).sort((a, b) =>
        a.displayName.localeCompare(b.displayName, "ko"),
      ),
    [snap],
  );
  const packages = useMemo(
    () =>
      Object.values(snap.library.packages).sort((a, b) =>
        a.id === UNCATEGORIZED_PKG ? 1 : b.id === UNCATEGORIZED_PKG ? -1 : a.id.localeCompare(b.id),
      ),
    [snap],
  );

  const presentCats = useMemo(() => {
    const set = new Set<CategoryKey>();
    for (const d of defs) set.add(catKey(d.cat));
    return CAT_ORDER.filter((c) => set.has(c));
  }, [defs]);

  const filtered = useMemo(() => {
    let rows = defs;
    const q = query.trim().toLowerCase();
    if (q) {
      rows = rows.filter(
        (d) => d.displayName.toLowerCase().includes(q) || d.id.toLowerCase().includes(q),
      );
    }
    if (catFilter !== "all") rows = rows.filter((d) => catKey(d.cat) === catFilter);
    return rows;
  }, [defs, query, catFilter]);

  const onDuplicate = async (d: PolicyDef) => {
    try {
      await duplicateDef(d.id);
      invalidate();
      onToast("정의를 복제했어요");
    } catch (err) {
      console.error("[v2 library] duplicate failed:", err);
      onToast("복제하지 못했어요");
    }
  };

  const onDelete = async (d: PolicyDef) => {
    const n = defUsageCount(snap, d.id);
    const msg =
      n > 0
        ? `정책 "${d.displayName}"를 삭제할까요?\n${n}개 지갑에서 함께 제거됩니다. 되돌릴 수 없어요.`
        : `정책 "${d.displayName}"를 삭제할까요?\n되돌릴 수 없어요.`;
    if (!window.confirm(msg)) return;
    try {
      await deleteDef(d.id);
      invalidate();
      onToast("정책을 삭제했어요");
    } catch (err) {
      console.error("[v2 library] delete failed:", err);
      onToast("삭제하지 못했어요");
    }
  };

  return (
    <div className="ev2-2col">
      <PackageSection snap={snap} packages={packages} onToast={onToast} invalidate={invalidate} />

      <section className="ev2-right">
        <div className="ev2-ctrl">
          <div className="ev2-search">
            <SearchIcon />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="정책 이름 검색…"
            />
          </div>
          <span className="ev2-spc" />
        </div>

        {presentCats.length > 0 && (
          <div className="ev2-catbar">
            <button
              type="button"
              className={`ev2-catchip${catFilter === "all" ? " on" : ""}`}
              onClick={() => setCatFilter("all")}
            >
              모든 카테고리
            </button>
            {presentCats.map((c) => (
              <button
                key={c}
                type="button"
                className={`ev2-catchip${catFilter === c ? " on" : ""}`}
                onClick={() => setCatFilter(c)}
              >
                <span className="dot" style={{ background: catStyle(c).hex }} />
                {catLabel(c)}
              </button>
            ))}
          </div>
        )}

        <div className="ev2-scroll">
          {defs.length === 0 && (
            <div className="ev2-empty">
              <div className="big">아직 정책 정의가 없습니다</div>
              <div className="sm">상단 “+ 새 정책” 버튼으로 첫 정의를 만들어 보세요.</div>
            </div>
          )}

          {defs.length > 0 && (
            <div className="ev2-table compact lib">
              <div className="ev2-thead">
                <div className="ev2-c-name">정책</div>
                <div className="ev2-c-cat">카테고리</div>
                <div className="ev2-c-holes">파라미터</div>
                <div className="ev2-c-use">적용 지갑</div>
                <div className="ev2-c-defon">새 지갑 기본</div>
                <div className="ev2-c-time">마지막 수정</div>
                <div className="ev2-c-act" />
              </div>

              {filtered.map((d) => (
                <DefRow
                  key={d.id}
                  def={d}
                  usage={defUsageCount(snap, d.id)}
                  onOpen={() => navigate(`/editor/${encodeURIComponent(d.id)}`)}
                  onDuplicate={() => void onDuplicate(d)}
                  onDefaults={() => setDefaultsFor(d)}
                  onDelete={d.source === "builtin" ? null : () => void onDelete(d)}
                />
              ))}

              {filtered.length === 0 && (
                <div className="ev2-empty">
                  <div className="big">표시할 정책이 없어요</div>
                  <div className="sm">검색어나 카테고리 필터를 바꿔보세요.</div>
                </div>
              )}
            </div>
          )}
        </div>
      </section>

      {defaultsFor && (
        <DefDefaultsModal
          def={defaultsFor}
          packages={packages}
          onCancel={() => setDefaultsFor(null)}
          onSave={async (enabled, packageId) => {
            try {
              await putDef({
                ...defaultsFor,
                defaults: { ...defaultsFor.defaults, enabled, packageId },
                updatedAtMs: Date.now(),
              });
              invalidate();
              onToast("기본값을 저장했어요");
            } catch (err) {
              console.error("[v2 library] defaults save failed:", err);
              onToast("기본값을 저장하지 못했어요");
            }
            setDefaultsFor(null);
          }}
        />
      )}
    </div>
  );
}

const SOURCE_LABEL: Record<PolicyDef["source"], string> = {
  builtin: "내장",
  mine: "내 정책",
  market: "마켓",
};

function DefRow(props: {
  def: PolicyDef;
  usage: number;
  onOpen: () => void;
  onDuplicate: () => void;
  onDefaults: () => void;
  onDelete: (() => void) | null;
}) {
  const { def, usage, onOpen, onDuplicate, onDefaults, onDelete } = props;
  const cat = catKey(def.cat);
  return (
    <div className="ev2-trow" onClick={onOpen}>
      <div className="ev2-c-name">
        <div className="nm">{def.displayName}</div>
        <div className="sub">{SOURCE_LABEL[def.source]}</div>
      </div>
      <div className="ev2-c-cat">
        <span className="ev2-catchip mini">
          <span className="dot" style={{ background: catStyle(cat).hex }} />
          {catLabel(cat)}
        </span>
      </div>
      <div className="ev2-c-holes">{def.holes.length > 0 ? `${def.holes.length}개` : "–"}</div>
      <div className="ev2-c-use">{usage > 0 ? `${usage}개` : "–"}</div>
      <div className="ev2-c-defon">{def.defaults.enabled ? "적용" : "–"}</div>
      <div className="ev2-c-time">{mtimeLabel(def.updatedAtMs, false)}</div>
      <div className="ev2-c-act" onClick={(e) => e.stopPropagation()}>
        <button type="button" className="ev2-iconbtn" title="기본값 설정" onClick={onDefaults}>
          <PencilIcon />
        </button>
        <button type="button" className="ev2-iconbtn" title="복제" onClick={onDuplicate}>
          <CopyIcon />
        </button>
        {onDelete && (
          <button type="button" className="ev2-iconbtn danger" title="삭제" onClick={onDelete}>
            <TrashIcon />
          </button>
        )}
      </div>
    </div>
  );
}

/** 새 지갑 기본 적용 여부 + 기본 패키지 — def.defaults 편집 모달. */
function DefDefaultsModal(props: {
  def: PolicyDef;
  packages: PackageDef[];
  onCancel: () => void;
  onSave: (enabled: boolean, packageId: string | undefined) => void;
}) {
  const { def, packages, onCancel, onSave } = props;
  const [enabled, setEnabled] = useState(def.defaults.enabled);
  const [packageId, setPackageId] = useState(def.defaults.packageId ?? UNCATEGORIZED_PKG);

  return (
    <div className="ptm-bd" role="dialog" aria-modal onClick={onCancel}>
      <div className="ptm" onClick={(e) => e.stopPropagation()}>
        <div className="ptm-h">
          <div className="ptm-t">기본값 설정</div>
          <div className="ptm-s">
            <b>{def.displayName}</b> — 앞으로 추가되는 지갑에 어떻게 적용할까요?
          </div>
        </div>
        <div className="ptm-opts">
          <label className="ptm-field">
            <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
            새 지갑에 기본으로 적용
          </label>
          <label className="ptm-field">
            기본 패키지
            <select value={packageId} onChange={(e) => setPackageId(e.target.value)}>
              {packages.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.displayName}
                </option>
              ))}
            </select>
          </label>
          <div className="ptm-row">
            <button type="button" className="ev2-sec" onClick={onCancel}>
              취소
            </button>
            <button
              type="button"
              className="ev2-pri"
              onClick={() => onSave(enabled, packageId === UNCATEGORIZED_PKG ? undefined : packageId)}
            >
              저장
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

/* ─────────────── 패키지 섹션 (좌측) ─────────────── */

function PackageSection(props: {
  snap: StoreSnapshot;
  packages: PackageDef[];
  onToast: (text: string) => void;
  invalidate: () => void;
}) {
  const { snap, packages, onToast, invalidate } = props;
  const [renaming, setRenaming] = useState<string | null>(null);
  const [draftName, setDraftName] = useState("");

  // 패키지별 멤버 바인딩 인스턴스 수(모든 지갑 합계).
  const memberCount = useMemo(() => {
    const m = new Map<string, number>();
    for (const w of Object.values(snap.wallets.byAddress)) {
      for (const b of Object.values(w.bindings)) {
        m.set(b.packageId, (m.get(b.packageId) ?? 0) + 1);
      }
    }
    return m;
  }, [snap]);

  const createPackage = async () => {
    try {
      await putPackage({
        id: `pkg::${crypto.randomUUID()}`,
        displayName: "새 패키지",
        source: "mine",
        updatedAtMs: Date.now(),
      });
      invalidate();
      onToast("패키지를 만들었어요 — 이름을 바꿔보세요");
    } catch (err) {
      console.error("[v2 library] createPackage failed:", err);
      onToast("패키지를 만들지 못했어요");
    }
  };

  const renamePackage = async (pkg: PackageDef) => {
    const name = draftName.trim();
    setRenaming(null);
    if (!name || name === pkg.displayName) return;
    try {
      await putPackage({ ...pkg, displayName: name, updatedAtMs: Date.now() });
      invalidate();
    } catch (err) {
      console.error("[v2 library] renamePackage failed:", err);
      onToast("이름을 바꾸지 못했어요");
    }
  };

  const removePackage = async (pkg: PackageDef) => {
    if (
      !window.confirm(
        `패키지 "${pkg.displayName}"를 삭제할까요?\n안의 정책 인스턴스는 '미분류'로 이동해요.`,
      )
    )
      return;
    try {
      await deletePackageApi(pkg.id);
      invalidate();
      onToast("패키지를 삭제했어요");
    } catch (err) {
      console.error("[v2 library] deletePackage failed:", err);
      onToast("패키지를 삭제하지 못했어요");
    }
  };

  return (
    <aside className="ev2-left">
      <div className="ev2-leftsec">
        <div className="ev2-lefthead">
          <span>패키지</span>
          <button type="button" className="ev2-iconbtn" title="새 패키지" onClick={() => void createPackage()}>
            <PlusIcon />
          </button>
        </div>
        <div className="ev2-pkglist">
          {packages.map((pkg) => {
            const locked = pkg.id === UNCATEGORIZED_PKG;
            return (
              <div key={pkg.id} className="ev2-pkgrow">
                <FolderIcon />
                {renaming === pkg.id ? (
                  <input
                    autoFocus
                    value={draftName}
                    onChange={(e) => setDraftName(e.target.value)}
                    onBlur={() => void renamePackage(pkg)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void renamePackage(pkg);
                      if (e.key === "Escape") setRenaming(null);
                    }}
                  />
                ) : (
                  <span className="nm">{pkg.displayName}</span>
                )}
                <span className="cnt">{memberCount.get(pkg.id) ?? 0}</span>
                {!locked && (
                  <span className="acts">
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
                      onClick={() => void removePackage(pkg)}
                    >
                      <TrashIcon />
                    </button>
                  </span>
                )}
              </div>
            );
          })}
        </div>
        <div className="ev2-lefthint">
          패키지 켜기/끄기는 지갑별 설정이에요 — <b>적용 현황</b> 탭에서 관리해요.
        </div>
      </div>
    </aside>
  );
}

function ToastStack({ toasts }: { toasts: ToastMsg[] }) {
  if (toasts.length === 0) return null;
  return (
    <div className="ev2-toaststack">
      {toasts.map((t) => (
        <div key={t.id} className="ev2-toast">
          {t.text}
        </div>
      ))}
    </div>
  );
}
