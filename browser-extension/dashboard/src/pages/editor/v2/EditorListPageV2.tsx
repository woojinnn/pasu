import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";

import {
  ENABLED_IDS_STORAGE_KEY,
  dashboardId,
  dashboardSetId,
  getEnabledPolicyIds,
  getPolicyCatalog,
  listListings,
  listManagedPolicies,
  listPolicySets,
  putPolicy,
  putPolicySet,
  setEnabledPolicyIds,
  stripDashboardSetId,
  subscribeToBroadcast,
  type ManagedPolicy,
  type PolicySet,
} from "../../../server-api";
import {
  DAY1_SET_ID,
  buildDay1Policies,
  buildDay1Set,
  isDay1Id,
} from "./day1-baseline";
import { Topbar } from "../../../shell/Topbar";
import { FEATURES } from "../../../features";
import { nameFromPolicy, severityFromCedar } from "../policy-meta";
import { NewPolicyChooser } from "./NewPolicyChooser";

import {
  CAT_ORDER,
  catKey,
  catLabel,
  catStyle,
  type CategoryKey,
} from "./categories";
import {
  CatIcon,
  CaretRightIcon,
  CheckIcon,
  DotIcon,
  FolderIcon,
  GripIcon,
  LockIcon,
  PencilIcon,
  PlusIcon,
  SearchIcon,
  ShieldIcon,
  WarnIcon,
  XIcon,
} from "./icons";
import {
  buildSetMembership,
  filterByScope,
  isDraft,
  isMarketSource,
  mtimeLabel,
  rowOn,
  type ListScope,
} from "./helpers";

import "./editor-v2.css";

type Density = "cozy" | "compact";
type StatusFilter = "all" | "on" | "draft" | "off";
type CatFilter = "all" | CategoryKey;

interface ToastMsg {
  id: number;
  text: string;
}

/**
 * Phase 1 list view — mypolicy-list.jsx ported to the dashboard SPA.
 *
 * Layout: 2-column. Left: package panel (scope nav + drag-drop targets).
 * Right: column-sorted table with search, filters, density toggle, and
 * a bulk action bar that appears on multi-select.
 *
 * Drag-and-drop is intentionally deferred to a follow-up; the new-package
 * zone is wired through the bulk action bar instead so users can still
 * group selections without the pointermove plumbing.
 */
export function EditorListPageV2() {
  const navigate = useNavigate();
  const qc = useQueryClient();

  const listQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
  });
  const setsQ = useQuery({
    queryKey: ["policy-sets"],
    queryFn: listPolicySets,
  });
  const enabledQ = useQuery({
    queryKey: ["enabled-policy-ids"],
    queryFn: getEnabledPolicyIds,
  });
  // baked day1 베이스라인은 managed 목록이 아니라 catalog 에만 있다.
  const catalogQ = useQuery({
    queryKey: ["policy-catalog"],
    queryFn: getPolicyCatalog,
  });

  useEffect(() => {
    const unsubscribe = subscribeToBroadcast((keys) => {
      const enabledTouched = keys.some(
        (k) =>
          k === ENABLED_IDS_STORAGE_KEY ||
          k.startsWith(`${ENABLED_IDS_STORAGE_KEY}:`),
      );
      const userSwitched = keys.includes("dashboard:current-user-id");
      if (enabledTouched || userSwitched) {
        void qc.invalidateQueries({ queryKey: ["enabled-policy-ids"] });
      }
      if (userSwitched) {
        void qc.invalidateQueries({ queryKey: ["managed-policies"] });
        void qc.invalidateQueries({ queryKey: ["policy-sets"] });
        void qc.invalidateQueries({ queryKey: ["policy-catalog"] });
      }
    });
    return unsubscribe;
  }, [qc]);

  const enabledSet = useMemo(
    () => new Set(enabledQ.data ?? []),
    [enabledQ.data],
  );

  const toggleMut = useMutation({
    mutationFn: async (next: string[]) => {
      await setEnabledPolicyIds(next);
      return next;
    },
    onMutate: async (next) => {
      await qc.cancelQueries({ queryKey: ["enabled-policy-ids"] });
      const previous = qc.getQueryData<string[]>(["enabled-policy-ids"]) ?? [];
      qc.setQueryData(["enabled-policy-ids"], next);
      return { previous };
    },
    onError: (_err, _vars, ctx) => {
      if (ctx?.previous) qc.setQueryData(["enabled-policy-ids"], ctx.previous);
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ["enabled-policy-ids"] });
    },
  });

  const togglePolicy = (id: string, on: boolean) => {
    const next = new Set(enabledSet);
    if (on) next.add(id);
    else next.delete(id);
    toggleMut.mutate([...next]);
  };

  const setManyEnabled = (ids: string[], on: boolean) => {
    const next = new Set(enabledSet);
    for (const id of ids) {
      if (on) next.add(id);
      else next.delete(id);
    }
    toggleMut.mutate([...next]);
  };

  // day1 베이스라인(읽기전용)을 managed 목록 앞에 합성 주입한다. 그러면 scope
  // 패널·테이블·토글이 별도 분기 없이 그대로 처리한다. day1 패키지는 항상 맨 위.
  const day1Policies = useMemo(
    () => buildDay1Policies(catalogQ.data),
    [catalogQ.data],
  );
  const policies = useMemo(
    () => [...day1Policies, ...(listQ.data ?? [])],
    [day1Policies, listQ.data],
  );
  const sets = useMemo(() => {
    const day1Set = buildDay1Set(day1Policies);
    return day1Set ? [day1Set, ...(setsQ.data ?? [])] : (setsQ.data ?? []);
  }, [day1Policies, setsQ.data]);
  const setMembership = useMemo(() => buildSetMembership(sets), [sets]);

  /** Map listing_id → current_version for stale-install detection.
   *  We pull one batch of listings (kind-agnostic, up to 200) and build
   *  a local lookup; per-row React Query fan-out would chatter too much
   *  for a list view with a dozen market installs. */
  const updateQ = useQuery({
    queryKey: ["market-listing-versions"],
    queryFn: async () => {
      const items = await listListings({ limit: 200 });
      const map = new Map<string, string>();
      for (const l of items) {
        if (l.current_version) map.set(l.id, l.current_version);
      }
      return map;
    },
    enabled:
      FEATURES.marketUpdateBadge &&
      policies.some((p) => p.source === "market" && !!p.sourceListingId),
    staleTime: 5 * 60_000,
  });
  const upstreamVersionMap = updateQ.data ?? new Map<string, string>();

  const [scope, setScope] = useState<ListScope>({ type: "all" });
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [catFilter, setCatFilter] = useState<CatFilter>("all");
  const [density, setDensity] = useState<Density>("cozy");
  const [selection, setSelection] = useState<Set<string>>(new Set());
  const [toasts, setToasts] = useState<ToastMsg[]>([]);
  const [chooserOpen, setChooserOpen] = useState(false);

  const pushToast = (text: string) => {
    const id = Date.now() + Math.floor(Math.random() * 1000);
    setToasts((t) => [...t, { id, text }]);
    window.setTimeout(() => {
      setToasts((t) => t.filter((m) => m.id !== id));
    }, 2400);
  };

  const totalRules = policies.length;
  const looseCount = useMemo(() => {
    const claimed = new Set<string>();
    for (const ids of setMembership.values()) {
      for (const id of ids) claimed.add(id);
    }
    return policies.filter((p) => !claimed.has(p.id)).length;
  }, [policies, setMembership]);

  const scoped = useMemo(
    () => filterByScope(policies, setMembership, scope),
    [policies, setMembership, scope],
  );

  const presentCats = useMemo(() => {
    const set = new Set<CategoryKey>();
    for (const p of policies) set.add(catKey(p.cat));
    return CAT_ORDER.filter((c) => set.has(c));
  }, [policies]);

  const filteredRows = useMemo(() => {
    let rows = scoped;
    const q = query.trim().toLowerCase();
    if (q) {
      rows = rows.filter(
        (r) =>
          nameFromPolicy(r).toLowerCase().includes(q) ||
          stripDashboardSetId(r.id).toLowerCase().includes(q),
      );
    }
    if (catFilter !== "all") rows = rows.filter((r) => catKey(r.cat) === catFilter);
    if (statusFilter === "on") {
      rows = rows.filter((r) => rowOn(r, enabledSet.has(r.id)));
    } else if (statusFilter === "draft") {
      rows = rows.filter(isDraft);
    } else if (statusFilter === "off") {
      rows = rows.filter((r) => !isDraft(r) && !enabledSet.has(r.id));
    }
    return rows;
  }, [scoped, query, catFilter, statusFilter, enabledSet]);

  const onSelect = (id: string) =>
    setSelection((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const clearSel = () => setSelection(new Set());

  const makePackage = async (ids: string[]) => {
    const stamp = Date.now().toString(36);
    const setId = dashboardSetId(`pkg-${stamp}`);
    try {
      await putPolicySet({
        id: setId,
        displayName: "새 패키지",
        memberIds: ids,
        source: "mine",
      });
      await qc.invalidateQueries({ queryKey: ["policy-sets"] });
      pushToast(`새 패키지를 만들었어요 (${ids.length}개)`);
      clearSel();
    } catch (err) {
      console.error("[v2 list] makePackage failed:", err);
      pushToast("패키지를 만들지 못했어요");
    }
  };

  const activePkg =
    scope.type === "pkg" ? sets.find((s) => s.id === scope.id) ?? null : null;

  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle={
          listQ.data ? `${policies.length} policies · ${sets.length} packages` : "…"
        }
        right={
          <>
            {import.meta.env.DEV && <SeedPhase1ADefaultsButton />}
            {FEATURES.newChooser ? (
              <button
                type="button"
                className="ev2-pri"
                onClick={() => setChooserOpen(true)}
              >
                <PlusIcon />
                새 정책
              </button>
            ) : (
              <Link to="/editor/new" className="ev2-pri">
                <PlusIcon />
                새 정책
              </Link>
            )}
          </>
        }
      />

      <div className="ev2-body">
        <div className="ev2-2col">
          <PackagePanel
            scope={scope}
            setScope={setScope}
            sets={sets}
            policies={policies}
            setMembership={setMembership}
            enabledSet={enabledSet}
            totalRules={totalRules}
            looseCount={looseCount}
          />

          <section className="ev2-right">
            <div className="ev2-ctrl">
              <div className="ev2-search">
                <SearchIcon />
                <input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="정책 이름·slug 검색…"
                />
              </div>
              <span className="ev2-spc" />
              <div className="ev2-seg" role="tablist">
                {(
                  [
                    ["all", "전체"],
                    ["on", "켜진 것"],
                    ["draft", "수정중"],
                    ["off", "꺼짐"],
                  ] as const
                ).map(([k, label]) => (
                  <button
                    key={k}
                    type="button"
                    className={statusFilter === k ? "on" : ""}
                    onClick={() => setStatusFilter(k)}
                  >
                    {label}
                  </button>
                ))}
              </div>
              <div className="ev2-density" title="행 밀도">
                <button
                  type="button"
                  className={density === "cozy" ? "on" : ""}
                  onClick={() => setDensity("cozy")}
                >
                  여유
                </button>
                <button
                  type="button"
                  className={density === "compact" ? "on" : ""}
                  onClick={() => setDensity("compact")}
                >
                  촘촘
                </button>
              </div>
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

            <ScopeHeader
              scope={scope}
              activePkg={activePkg}
              rowCount={filteredRows.length}
              onClearScope={() => setScope({ type: "all" })}
            />

            <div className="ev2-scroll">
              {listQ.isLoading && (
                <div className="ev2-status">불러오는 중…</div>
              )}

              {listQ.data && policies.length === 0 && (
                <div className="ev2-empty">
                  <div className="big">아직 설치된 정책이 없습니다</div>
                  <div className="sm">
                    상단 “+ 새 정책” 버튼이나 마켓에서 정책을 가져와 보세요.
                  </div>
                </div>
              )}

              {policies.length > 0 && (
                <div className={`ev2-table ${density}`}>
                  <div className="ev2-thead">
                    <div className="ev2-c-sel">
                      <button
                        type="button"
                        className={`ev2-selbox head${selection.size > 0 ? " on" : ""}`}
                        onClick={() => {
                          if (selection.size > 0) clearSel();
                          // day1 베이스라인(읽기전용)은 전체선택에서 제외.
                          else
                            setSelection(
                              new Set(
                                filteredRows
                                  .filter((r) => !isDay1Id(r.id))
                                  .map((r) => r.id),
                              ),
                            );
                        }}
                        title="보이는 항목 전체 선택"
                      >
                        {selection.size > 0 && <CheckIcon />}
                      </button>
                    </div>
                    <div className="ev2-c-name">정책</div>
                    <div className="ev2-c-cat">카테고리</div>
                    <div className="ev2-c-sev">심각도</div>
                    <div className="ev2-c-flag">알림</div>
                    <div className="ev2-c-time">마지막 수정</div>
                    <div className="ev2-c-act">상태</div>
                  </div>

                  {filteredRows.map((p) => {
                    const upstream =
                      p.sourceListingId !== undefined
                        ? upstreamVersionMap.get(p.sourceListingId)
                        : undefined;
                    const updateAvailable =
                      !!upstream &&
                      !!p.sourceVersion &&
                      upstream !== p.sourceVersion;
                    return (
                      <PolicyRow
                        key={p.id}
                        policy={p}
                        enabled={enabledSet.has(p.id)}
                        selected={selection.has(p.id)}
                        updateAvailable={updateAvailable}
                        upstreamVersion={upstream}
                        readOnly={isDay1Id(p.id)}
                        onSelect={() => onSelect(p.id)}
                        onToggle={(on) => togglePolicy(p.id, on)}
                        onOpen={() => {
                          // baked day1 정책은 편집 페이지가 없다 — 열기 무시.
                          if (isDay1Id(p.id)) return;
                          navigate(`/editor/${encodeURIComponent(p.id)}`);
                        }}
                      />
                    );
                  })}

                  {filteredRows.length === 0 && policies.length > 0 && (
                    <div className="ev2-empty">
                      <div className="big">표시할 정책이 없어요</div>
                      <div className="sm">
                        필터를 바꾸거나 다른 패키지를 골라보세요.
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>

            {selection.size > 0 && (
              <BulkActionBar
                count={selection.size}
                onClear={clearSel}
                onBulkOn={() => {
                  setManyEnabled([...selection], true);
                  pushToast(`${selection.size}개 켰어요`);
                  clearSel();
                }}
                onBulkOff={() => {
                  setManyEnabled([...selection], false);
                  pushToast(`${selection.size}개 껐어요`);
                  clearSel();
                }}
                onMakePackage={() => void makePackage([...selection])}
              />
            )}
          </section>
        </div>
      </div>

      <ToastStack toasts={toasts} />

      <NewPolicyChooser
        open={chooserOpen}
        onClose={() => setChooserOpen(false)}
      />
    </>
  );
}

/* ─────────────── Seed example policies (DEV) ─────────────── */
/**
 * DEV-only topbar action that seeds the bundled phase1/A example
 * policies into the same managed-policy store the v2 list reads from.
 * Each bundle is written via `putPolicy` (carrying its manifest so the
 * v2 loader can compose the policy's schema), then the `managed-policies`
 * and `enabled-policy-ids` queries are invalidated so the list refreshes.
 * Ported from the removed Legacy list page; behaviour is unchanged.
 */
function SeedPhase1ADefaultsButton() {
  const qc = useQueryClient();
  const [status, setStatus] = useState<
    | { kind: "idle" }
    | { kind: "running"; done: number; total: number }
    | { kind: "done"; ok: number; failed: number }
  >({ kind: "idle" });

  async function runSeed() {
    const { default: bundles } = (await import("../phase1A-seed.json")) as {
      default: ReadonlyArray<{ id: string; cedar: string; manifest: unknown }>;
    };
    setStatus({ kind: "running", done: 0, total: bundles.length });
    let ok = 0;
    let failed = 0;
    for (let i = 0; i < bundles.length; i++) {
      const b = bundles[i];
      try {
        await putPolicy({
          id: dashboardId(b.id),
          cedarText: b.cedar,
          manifest: b.manifest,
          displayName: b.id,
        });
        ok += 1;
      } catch (err) {
        console.warn(`[seed phase1/A] ${b.id} failed:`, err);
        failed += 1;
      }
      setStatus({ kind: "running", done: i + 1, total: bundles.length });
    }
    setStatus({ kind: "done", ok, failed });
    await qc.invalidateQueries({ queryKey: ["managed-policies"] });
    await qc.invalidateQueries({ queryKey: ["enabled-policy-ids"] });
  }

  const label =
    status.kind === "idle"
      ? "+ Seed phase1/A (dev)"
      : status.kind === "running"
        ? `Seeding ${status.done}/${status.total}…`
        : `Seeded ${status.ok}${status.failed ? ` (${status.failed} failed)` : ""}`;

  return (
    <button
      type="button"
      className="btn-secondary"
      disabled={status.kind === "running"}
      onClick={runSeed}
      title="phase1/A 36개 기본 정책을 chrome.storage.local에 시드 (DEV 전용)"
      style={{ marginRight: 8 }}
    >
      {label}
    </button>
  );
}

/* ─────────────── Package Panel ─────────────── */
function PackagePanel(props: {
  scope: ListScope;
  setScope: (s: ListScope) => void;
  sets: PolicySet[];
  policies: ManagedPolicy[];
  setMembership: Map<string, Set<string>>;
  enabledSet: Set<string>;
  totalRules: number;
  looseCount: number;
}) {
  const {
    scope,
    setScope,
    sets,
    policies,
    setMembership,
    enabledSet,
    totalRules,
    looseCount,
  } = props;

  const policyById = useMemo(
    () => new Map(policies.map((p) => [p.id, p])),
    [policies],
  );

  return (
    <aside className="ev2-left">
      <div className="ev2-left-scroll">
        <div className="ev2-left-grp">
          <PackBtn
            active={scope.type === "all"}
            onClick={() => setScope({ type: "all" })}
            icon={<ShieldIcon />}
            name="전체"
            right={<span className="ev2-pk-ct">{totalRules}</span>}
          />
          <PackBtn
            active={scope.type === "loose"}
            onClick={() => setScope({ type: "loose" })}
            icon={<DotIcon />}
            name="단일 정책"
            sub="어느 패키지에도 안 든 내 정책"
            right={<span className="ev2-pk-ct">{looseCount}</span>}
          />
        </div>

        <div className="ev2-left-sec">
          <span className="t">내 패키지</span>
          <span className="ct">{sets.length}</span>
        </div>

        <div className="ev2-left-grp">
          {sets.map((s) => {
            const memberIds = setMembership.get(s.id) ?? new Set<string>();
            const onCount = [...memberIds].filter((id) => {
              const m = policyById.get(id);
              return m ? rowOn(m, enabledSet.has(id)) : false;
            }).length;
            const market = isMarketSource(s);
            const isDay1 = s.id === DAY1_SET_ID;
            const cstyle = catStyle(s.cat);
            return (
              <PackBtn
                key={s.id}
                active={scope.type === "pkg" && scope.id === s.id}
                onClick={() => setScope({ type: "pkg", id: s.id })}
                icon={
                  <span style={{ color: cstyle.hex, display: "grid", placeItems: "center" }}>
                    {isDay1 ? <ShieldIcon /> : <FolderIcon />}
                  </span>
                }
                name={s.displayName}
                sub={
                  <>
                    <b>{onCount}</b>/{memberIds.size} 켜짐
                  </>
                }
                source={
                  isDay1 ? (
                    <>
                      <ShieldIcon />
                      기본 제공
                    </>
                  ) : market ? (
                    <>
                      <ShieldIcon />
                      마켓에서 가져옴
                      {s.sourceVersion ? ` · ${s.sourceVersion}` : ""}
                    </>
                  ) : (
                    <>
                      <PencilIcon />
                      내가 만듦
                    </>
                  )
                }
                badge={s.readOnly ? <LockIcon /> : null}
              />
            );
          })}
        </div>

        {sets.length === 0 && (
          <div className="ev2-left-empty">
            아직 패키지가 없습니다. 정책을 골라 묶어보세요.
          </div>
        )}
      </div>
    </aside>
  );
}

function PackBtn(props: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  name: React.ReactNode;
  sub?: React.ReactNode;
  source?: React.ReactNode;
  right?: React.ReactNode;
  badge?: React.ReactNode;
}) {
  const { active, onClick, icon, name, sub, source, right, badge } = props;
  return (
    <button
      type="button"
      className={`ev2-pk${active ? " active" : ""}`}
      onClick={onClick}
    >
      <span className="ev2-pk-ic">{icon}</span>
      <span className="ev2-pk-body">
        <span className="ev2-pk-nm">
          <span>{name}</span>
          {badge}
        </span>
        {sub && <span className="ev2-pk-sub">{sub}</span>}
        {source && <span className="ev2-pk-src">{source}</span>}
      </span>
      {right && <span className="ev2-pk-right">{right}</span>}
    </button>
  );
}

/* ─────────────── Scope Header ─────────────── */
function ScopeHeader(props: {
  scope: ListScope;
  activePkg: PolicySet | null;
  rowCount: number;
  onClearScope: () => void;
}) {
  const { scope, activePkg, rowCount, onClearScope } = props;
  const title =
    scope.type === "all"
      ? "전체"
      : scope.type === "loose"
        ? "단일 정책"
        : activePkg?.displayName ?? "";
  return (
    <div className="ev2-scopehd">
      <div className="ev2-scope-title">
        <span className="t">{title}</span>
        <span className="ct">{rowCount}개</span>
        {activePkg && activePkg.id === DAY1_SET_ID && (
          <span className="ev2-scope-prov">
            <ShieldIcon /> 기본 제공
          </span>
        )}
        {activePkg && activePkg.id !== DAY1_SET_ID && isMarketSource(activePkg) && (
          <span className="ev2-scope-prov">
            <ShieldIcon /> 마켓에서 가져옴
            {activePkg.sourceVersion ? ` · ${activePkg.sourceVersion}` : ""}
          </span>
        )}
        {activePkg && activePkg.id !== DAY1_SET_ID && !isMarketSource(activePkg) && (
          <span className="ev2-scope-prov mine">
            <PencilIcon /> 내가 만듦
          </span>
        )}
      </div>
      <span className="ev2-spc" />
      {scope.type !== "all" && (
        <button type="button" className="ev2-scope-clear" onClick={onClearScope}>
          <XIcon /> 전체 보기
        </button>
      )}
    </div>
  );
}

/* ─────────────── Policy Row ─────────────── */
function PolicyRow(props: {
  policy: ManagedPolicy;
  enabled: boolean;
  selected: boolean;
  updateAvailable?: boolean;
  upstreamVersion?: string;
  readOnly?: boolean;
  onSelect: () => void;
  onToggle: (on: boolean) => void;
  onOpen: () => void;
}) {
  const {
    policy,
    enabled,
    selected,
    updateAvailable,
    upstreamVersion,
    readOnly,
    onSelect,
    onToggle,
    onOpen,
  } = props;
  const draft = isDraft(policy);
  const on = rowOn(policy, enabled);
  const off = !draft && !enabled;
  const sev = severityFromCedar(policy.text);
  const sevClass = sev === "deny" ? "fail" : sev === "warn" ? "warn" : "info";
  const sevTxt = sev === "deny" ? "차단" : sev === "warn" ? "경고" : "정보";
  const cstyle = catStyle(policy.cat);
  const name = nameFromPolicy(policy);
  const slug = stripDashboardSetId(policy.id);

  const cls = [
    "ev2-trow",
    off && "off",
    draft && "draft",
    selected && "sel",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div
      className={cls}
      onClick={(e) => {
        const target = e.target as HTMLElement;
        if (target.closest("button,.ev2-selbox,.ev2-tg,.ev2-grip")) return;
        // day1 베이스라인(읽기전용)은 shift-click 선택도 막는다 — 선택되면
        // 벌크 토글/패키지 묶기로 baked 정책이 사용자 set 에 섞일 수 있다.
        if (readOnly) return;
        if (e.shiftKey) onSelect();
        else onOpen();
      }}
    >
      <div className="ev2-c-sel">
        {/* day1 베이스라인은 읽기전용 — 선택/패키지 묶기 대상에서 제외. */}
        {!readOnly && (
          <>
            <button
              type="button"
              className={`ev2-selbox${selected ? " on" : ""}`}
              onClick={(e) => {
                e.stopPropagation();
                onSelect();
              }}
              title="선택"
            >
              {selected && <CheckIcon />}
            </button>
            <span className="ev2-grip" title="드래그(곧 추가)">
              <GripIcon />
            </span>
          </>
        )}
      </div>

      <div className="ev2-c-name">
        <span className="ev2-cat-ic" style={cstyle.iconWrap}>
          <CatIcon cat={policy.cat} />
        </span>
        <div className="ev2-nm-wrap">
          <div className="ev2-nm-line">
            <span className="nm-t">{name}</span>
            {draft && (
              <span
                className="ev2-badge-draft"
                title="수정 중 — 평가에서 자동 제외"
              >
                <PencilIcon />
                수정중
              </span>
            )}
          </div>
          <div className="ev2-nm-slug">{slug}</div>
        </div>
      </div>

      <div className="ev2-c-cat">
        <span className="ev2-cat-tag" style={cstyle.tag}>
          {catLabel(policy.cat)}
        </span>
      </div>

      <div className="ev2-c-sev">
        <span className={`ev2-sevtag ${sevClass}`}>
          <span className="dt" />
          {sevTxt}
        </span>
      </div>

      <div className="ev2-c-flag">
        {updateAvailable && (
          <span
            className="ev2-fl-upd"
            title={
              upstreamVersion
                ? `최신 버전: ${upstreamVersion} (설치본: ${policy.sourceVersion ?? "?"})`
                : "업데이트 있음"
            }
          >
            <WarnIcon />
            업데이트
          </span>
        )}
      </div>

      <div className="ev2-c-time">{mtimeLabel(policy.updatedAtMs, draft)}</div>

      <div className="ev2-c-act">
        <button
          type="button"
          className={`ev2-tg${on ? " on" : ""}`}
          disabled={draft}
          onClick={(e) => {
            e.stopPropagation();
            onToggle(!enabled);
          }}
          title={draft ? "draft는 토글 불가" : "켜기/끄기"}
        >
          <span className="sw" />
        </button>
        {!readOnly && (
          <button
            type="button"
            className="ev2-open"
            onClick={(e) => {
              e.stopPropagation();
              onOpen();
            }}
            title="에디터 열기"
          >
            <CaretRightIcon />
          </button>
        )}
      </div>
    </div>
  );
}

/* ─────────────── Bulk Action Bar ─────────────── */
function BulkActionBar(props: {
  count: number;
  onClear: () => void;
  onBulkOn: () => void;
  onBulkOff: () => void;
  onMakePackage: () => void;
}) {
  const { count, onClear, onBulkOn, onBulkOff, onMakePackage } = props;
  return (
    <div className="ev2-selbar">
      <span className="ct">
        <b>{count}</b>개 선택됨
      </span>
      <button type="button" className="ghost" onClick={onClear}>
        해제
      </button>
      <span className="spc" />
      <button type="button" onClick={onBulkOn}>
        {count}개 켜기
      </button>
      <button type="button" onClick={onBulkOff}>
        {count}개 끄기
      </button>
      <button type="button" className="go" onClick={onMakePackage}>
        <FolderIcon /> 패키지로 묶기
      </button>
    </div>
  );
}

/* ─────────────── Toast ─────────────── */
function ToastStack({ toasts }: { toasts: ToastMsg[] }) {
  if (toasts.length === 0) return null;
  return (
    <div className="ev2-toaststack" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className="ev2-toast">
          {t.text}
        </div>
      ))}
    </div>
  );
}

/* Re-export the warn icon so phase-3+ overlay banners can reach it
 * without duplicating the import surface. */
export { WarnIcon };
