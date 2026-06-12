import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";
import { Trans, useTranslation } from "react-i18next";

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
  const { t } = useTranslation("editor");
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
        subtitle={
          defCount === null ? "…" : t("list.subtitle", { defs: defCount, pkgs: pkgCount })
        }
        right={
          <button type="button" className="ev2-pri" onClick={() => setChooserOpen(true)}>
            <PlusIcon />
            {t("list.newPolicy")}
          </button>
        }
      />

      <div className="ev2-body">
        <div className="ev2-pagetabs" role="tablist" aria-label={t("list.tablistAria")}>
          <button
            type="button"
            role="tab"
            aria-selected={tab === "apply"}
            className={`ev2-tab${tab === "apply" ? " on" : ""}`}
            onClick={() => setTab("apply")}
          >
            {t("list.tabApply")}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === "library"}
            className={`ev2-tab${tab === "library" ? " on" : ""}`}
            onClick={() => setTab("library")}
          >
            {t("list.tabLibrary")}
          </button>
        </div>

        {overviewQ.isLoading && <div className="ev2-status">{t("common:loading")}</div>}
        {overviewQ.isError && <div className="ev2-status">{t("list.storeReadError")}</div>}

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
  const { t } = useTranslation("editor");
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
      onToast(t("actionFailed", { action: label }));
      return false;
    }
  };

  const onDelete = (d: PolicyDef) => {
    const n = defUsageCount(snap, d.id);
    const msg =
      n > 0
        ? t("list.deleteConfirmUsed", { name: d.displayName, count: n })
        : t("list.deleteConfirm", { name: d.displayName });
    if (!window.confirm(msg)) return;
    void run(t("actions.delete"), () => deleteDef(d.id)).then(
      (ok) => ok && onToast(t("list.deletedToast")),
    );
  };

  const createPackage = () =>
    void run(t("actions.createFolder"), () =>
      putPackage({
        id: `pkg::${crypto.randomUUID()}`,
        displayName: t("list.newFolderName"),
        source: "mine",
        updatedAtMs: Date.now(),
      }),
    ).then((ok) => ok && onToast(t("list.folderCreatedToast")));

  const renamePackage = (pkg: PackageDef, name: string) => {
    const trimmed = name.trim();
    if (!trimmed || trimmed === pkg.displayName) return;
    void run(t("actions.rename"), () =>
      putPackage({ ...pkg, displayName: trimmed, updatedAtMs: Date.now() }),
    );
  };

  const removePackage = (pkg: PackageDef) => {
    if (!window.confirm(t("list.deleteFolderConfirm", { name: pkg.displayName }))) return;
    void run(t("actions.deleteFolder"), () => deletePackageApi(pkg.id)).then(
      (ok) => ok && onToast(t("list.folderDeletedToast")),
    );
  };

  // 디렉토리 드래그: 정의의 라이브러리 소속(defaults.packageId) 이동.
  const moveDef = (defId: string, packageId: string) => {
    const d = snap.library.defs[defId];
    if (!d) return;
    const next = packageId === UNCATEGORIZED_PKG ? undefined : packageId;
    if ((d.defaults.packageId ?? undefined) === next) return;
    void run(t("actions.moveFolder"), () =>
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
      onToast(t("list.emptyPackageToast"));
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
      onToast(t("list.publishPrepFailed"));
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
            placeholder={t("list.searchPlaceholder")}
          />
        </div>
        <span className="ev2-spc" />
        <button type="button" className="ev2-sec" onClick={createPackage}>
          <PlusIcon /> {t("list.newFolder")}
        </button>
      </div>

      {presentCats.length > 0 && (
        <div className="ev2-catbar">
          <button
            type="button"
            className={`ev2-catchip${catFilter === "all" ? " on" : ""}`}
            onClick={() => setCatFilter("all")}
          >
            {t("list.allCategories")}
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
            <div className="big">{t("list.emptyTitle")}</div>
            <div className="sm">{t("list.emptyHint")}</div>
          </div>
        ) : (
          <LibraryDirectory
            snap={snap}
            mode="manage"
            query={query}
            catFilter={catFilter}
            onOpenDef={(d) => navigate(`/editor/${encodeURIComponent(d.id)}`)}
            onDuplicate={(d) =>
              void run(t("actions.duplicate"), () => duplicateDef(d.id)).then(
                (ok) => ok && onToast(t("list.duplicatedToast")),
              )
            }
            onDelete={onDelete}
            onDefaults={setDefaultsFor}
            onToggleDefault={(d, enabled) =>
              void run(t("actions.changeDefault"), () =>
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
                      ? t("list.defaultOnToast", { name: d.displayName })
                      : t("list.defaultOffToast", { name: d.displayName }),
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
          <Trans t={t} i18nKey="list.dragHint" components={{ b: <b /> }} />
        </div>
      </div>

      <PublishModal open={publishSrc !== null} source={publishSrc} onClose={() => setPublishSrc(null)} />

      {defaultsFor && (
        <DefDefaultsModal
          def={defaultsFor}
          packages={Object.values(snap.library.packages)}
          onCancel={() => setDefaultsFor(null)}
          onSave={(enabled, packageId) => {
            void run(t("actions.saveDefaults"), () =>
              putDef({
                ...defaultsFor,
                defaults: { ...defaultsFor.defaults, enabled, packageId },
                updatedAtMs: Date.now(),
              }),
            ).then((ok) => ok && onToast(t("list.defaultsSavedToast")));
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
  const { t } = useTranslation("editor");
  const [enabled, setEnabled] = useState(def.defaults.enabled);
  const [packageId, setPackageId] = useState(def.defaults.packageId ?? UNCATEGORIZED_PKG);

  return (
    <div className="ptm-bd" role="dialog" aria-modal onClick={onCancel}>
      <div className="ptm" onClick={(e) => e.stopPropagation()}>
        <div className="ptm-h">
          <div className="ptm-t">{t("library.defaultsTitle")}</div>
          <div className="ptm-s">
            <Trans
              t={t}
              i18nKey="library.defaultsModalDesc"
              values={{ name: def.displayName }}
              components={{ b: <b /> }}
            />
          </div>
        </div>
        <div className="ptm-opts">
          <label className="ptm-field">
            <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
            {t("library.applyNewWalletsDefault")}
          </label>
          <label className="ptm-field">
            {t("library.folderLabel")}
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
              {t("common:cancel")}
            </button>
            <button
              type="button"
              className="ev2-pri"
              onClick={() => onSave(enabled, packageId === UNCATEGORIZED_PKG ? undefined : packageId)}
            >
              {t("common:save")}
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
