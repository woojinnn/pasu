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
import { defUsageCount } from "./wallet-policies-derive";
import { WalletPoliciesView } from "./WalletPoliciesView";
import { LibraryDirectory } from "./LibraryDirectory";
import { Topbar } from "../../../shell/Topbar";
import { NewPolicyChooser } from "./NewPolicyChooser";
import { CAT_ORDER, catKey, catLabel, catStyle, type CategoryKey } from "./categories";
import { PlusIcon, SearchIcon } from "./icons";
import { blocksToText } from "../../../cedar";
import type { PolicyIR } from "../../../cedar/blocks";
import { collectPackageMembers } from "../publish-package";
import { PublishModal, type PublishSource } from "../PublishModal";

import "./editor-v2.css";

type CatFilter = "all" | CategoryKey;
type EditorTab = "library" | "apply";

interface ToastMsg {
  id: number;
  text: string;
}

/**
 * /editor — 정책 스토리지 v2의 대시보드 진입점. 두 탭:
 *  - 지갑별 정책(기본): 그 지갑의 패키지×바인딩 토글 워크스페이스.
 *  - 라이브러리: 계정의 정의·패키지 관리 — 패키지를 디렉토리처럼 보여주는
 *    폴더 뷰(폴더 멤버십 = defaults.packageId).
 * 모든 데이터는 ps2:get-overview 한 번으로 읽고, 변이 후 invalidate로 재조회한다.
 */
export function EditorListPageV2() {
  const qc = useQueryClient();
  const [sp, setSp] = useSearchParams();
  // 기본 탭 = 적용 현황 (지갑별 워크스페이스). 라이브러리는 ?tab=library.
  const tab: EditorTab = sp.get("tab") === "library" ? "library" : "apply";
  const setTab = (t: EditorTab) =>
    setSp(t === "library" ? { tab: "library" } : {}, { replace: true });

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
  const defCount = snap ? Object.values(snap.library.defs).filter((d) => !d.hidden).length : null;
  const pkgCount = snap ? Object.keys(snap.library.packages).length : null;

  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle={defCount === null ? "…" : `정책 ${defCount}개 · 폴더 ${pkgCount}개`}
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
            aria-selected={tab === "apply"}
            className={`ev2-tab${tab === "apply" ? " on" : ""}`}
            onClick={() => setTab("apply")}
          >
            지갑별 정책
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === "library"}
            className={`ev2-tab${tab === "library" ? " on" : ""}`}
            onClick={() => setTab("library")}
          >
            라이브러리
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
        {snap && tab === "apply" && <WalletPoliciesView onToast={pushToast} />}
      </div>

      <ToastStack toasts={toasts} />
      <NewPolicyChooser open={chooserOpen} onClose={() => setChooserOpen(false)} />
    </>
  );
}

/* ─────────────── 라이브러리 탭 (디렉토리 뷰) ─────────────── */

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
  const [publishSrc, setPublishSrc] = useState<PublishSource | null>(null);

  const presentCats = useMemo(() => {
    const set = new Set<CategoryKey>();
    for (const d of Object.values(snap.library.defs)) set.add(catKey(d.cat));
    return CAT_ORDER.filter((c) => set.has(c));
  }, [snap]);

  const run = async (label: string, fn: () => Promise<unknown>): Promise<boolean> => {
    try {
      await fn();
      invalidate();
      return true;
    } catch (err) {
      console.error(`[v2 library] ${label} failed:`, err);
      onToast(`${label}에 실패했어요`);
      return false;
    }
  };

  const onDelete = (d: PolicyDef) => {
    const n = defUsageCount(snap, d.id);
    const msg =
      n > 0
        ? `정책 "${d.displayName}"를 삭제할까요?\n${n}개 지갑에서 함께 제거됩니다. 되돌릴 수 없어요.`
        : `정책 "${d.displayName}"를 삭제할까요?\n되돌릴 수 없어요.`;
    if (!window.confirm(msg)) return;
    void run("삭제", () => deleteDef(d.id)).then((ok) => ok && onToast("정책을 삭제했어요"));
  };

  const createPackage = () =>
    void run("폴더 생성", () =>
      putPackage({
        id: `pkg::${crypto.randomUUID()}`,
        displayName: "새 폴더",
        source: "mine",
        updatedAtMs: Date.now(),
      }),
    ).then((ok) => ok && onToast("폴더를 만들었어요 — 이름을 바꿔보세요"));

  const renamePackage = (pkg: PackageDef, name: string) => {
    const trimmed = name.trim();
    if (!trimmed || trimmed === pkg.displayName) return;
    void run("이름 변경", () =>
      putPackage({ ...pkg, displayName: trimmed, updatedAtMs: Date.now() }),
    );
  };

  const removePackage = (pkg: PackageDef) => {
    if (
      !window.confirm(
        `폴더 "${pkg.displayName}"를 삭제할까요?\n안의 정책은 '미분류'로 이동해요.`,
      )
    )
      return;
    void run("폴더 삭제", () => deletePackageApi(pkg.id)).then(
      (ok) => ok && onToast("폴더를 삭제했어요"),
    );
  };

  // 디렉토리 드래그: 정의의 라이브러리 소속(defaults.packageId) 이동.
  const moveDef = (defId: string, packageId: string) => {
    const d = snap.library.defs[defId];
    if (!d) return;
    const next = packageId === UNCATEGORIZED_PKG ? undefined : packageId;
    if ((d.defaults.packageId ?? undefined) === next) return;
    void run("폴더 이동", () =>
      putDef({ ...d, defaults: { ...d.defaults, packageId: next }, updatedAtMs: Date.now() }),
    ).then(
      (ok) =>
        ok &&
        onToast(
          `${d.displayName} → ${snap.library.packages[packageId]?.displayName ?? packageId}`,
        ),
    );
  };

  // 패키지 발행: defaults.packageId 기준 구성 defs를 렌더해 PublishModal로.
  const publishPackage = async (pkg: PackageDef) => {
    const members = collectPackageMembers(snap.library.defs, pkg.id);
    if (members.length === 0) {
      onToast("이 패키지에 든 정책이 없어요");
      return;
    }
    try {
      const rendered = await Promise.all(
        members.map(async (d) => ({
          slug: d.id.replace(/^def::/, ""),
          title: d.displayName,
          cedarText: await blocksToText(d.skeleton.ir as PolicyIR),
          manifest: d.skeleton.manifest,
        })),
      );
      setPublishSrc({
        kind: "package",
        suggestedDisplayName: pkg.displayName,
        suggestedSlug: pkg.id.replace(/^pkg::/, ""),
        members: rendered,
      });
    } catch (err) {
      console.error("[v2 library] publishPackage render failed:", err);
      onToast("발행 준비에 실패했어요");
    }
  };

  return (
    <div className="ld-wrap">
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
        <button type="button" className="ev2-sec" onClick={createPackage}>
          <PlusIcon /> 새 폴더
        </button>
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
        {Object.keys(snap.library.defs).length === 0 ? (
          <div className="ev2-empty">
            <div className="big">아직 정책 정의가 없습니다</div>
            <div className="sm">상단 “+ 새 정책” 버튼으로 첫 정의를 만들어 보세요.</div>
          </div>
        ) : (
          <LibraryDirectory
            snap={snap}
            mode="manage"
            query={query}
            catFilter={catFilter}
            onOpenDef={(d) => navigate(`/editor/${encodeURIComponent(d.id)}`)}
            onDuplicate={(d) =>
              void run("복제", () => duplicateDef(d.id)).then(
                (ok) => ok && onToast("정의를 복제했어요"),
              )
            }
            onDelete={onDelete}
            onDefaults={setDefaultsFor}
            onToggleDefault={(d, enabled) =>
              void run("기본 적용 변경", () =>
                putDef({
                  ...d,
                  defaults: { ...d.defaults, enabled },
                  updatedAtMs: Date.now(),
                }),
              ).then(
                (ok) =>
                  ok &&
                  onToast(
                    enabled
                      ? `${d.displayName} — 새 지갑에 기본 적용돼요`
                      : `${d.displayName} — 새 지갑 기본 적용을 껐어요`,
                  ),
              )
            }
            onRenamePackage={renamePackage}
            onDeletePackage={removePackage}
            onPublishPackage={(pkg) => void publishPackage(pkg)}
            onMoveDef={moveDef}
          />
        )}
        <div className="ev2-lefthint">
          정책을 끌어다 폴더에 놓으면 소속이 바뀌어요 — 지갑 적용은 <b>지갑별 정책</b>{" "}
          탭에서.
        </div>
      </div>

      <PublishModal open={publishSrc !== null} source={publishSrc} onClose={() => setPublishSrc(null)} />

      {defaultsFor && (
        <DefDefaultsModal
          def={defaultsFor}
          packages={Object.values(snap.library.packages)}
          onCancel={() => setDefaultsFor(null)}
          onSave={(enabled, packageId) => {
            void run("기본값 저장", () =>
              putDef({
                ...defaultsFor,
                defaults: { ...defaultsFor.defaults, enabled, packageId },
                updatedAtMs: Date.now(),
              }),
            ).then((ok) => ok && onToast("기본값을 저장했어요"));
            setDefaultsFor(null);
          }}
        />
      )}
    </div>
  );
}

/** 새 지갑 기본 적용 여부 + 소속 패키지 — def.defaults 편집 모달. */
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
            소속 폴더
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
