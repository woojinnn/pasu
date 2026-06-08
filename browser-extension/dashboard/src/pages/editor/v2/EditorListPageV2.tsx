import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";

import {
  ENABLED_IDS_STORAGE_KEY,
  dashboardId,
  dashboardSetId,
  getEnabledPolicyIds,
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

type StatusFilter = "all" | "on" | "draft" | "off";

/** dataTransfer MIME for dragging policy rows onto a package. Carries a JSON
 *  array of policy ids (one, or the whole selection when a selected row drags). */
const DRAG_MIME = "application/x-policy-ids";
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

  const policies = listQ.data ?? [];
  const sets = setsQ.data ?? [];
  const setMembership = useMemo(() => buildSetMembership(sets), [sets]);
  const policyById = useMemo(
    () => new Map(policies.map((p) => [p.id, p])),
    [policies],
  );
  // How many packages each policy belongs to — drives the "N개 패키지" badge
  // (many-to-many: a policy can live in several packages).
  const pkgCountByPolicy = useMemo(() => {
    const m = new Map<string, number>();
    for (const ids of setMembership.values()) {
      for (const id of ids) m.set(id, (m.get(id) ?? 0) + 1);
    }
    return m;
  }, [setMembership]);
  // "Armed live" members of each package: in `memberIds`, not muted, a real
  // non-draft policy. Package on/off acts ONLY on these — a muted member is
  // along for the ride but untouched by activation.
  const armedLiveBySet = useMemo(() => {
    const m = new Map<string, string[]>();
    for (const s of sets) {
      const muted = new Set(s.mutedMemberIds ?? []);
      m.set(
        s.id,
        s.memberIds.filter((id) => {
          const p = policyById.get(id);
          return p && !isDraft(p) && !muted.has(id);
        }),
      );
    }
    return m;
  }, [sets, policyById]);
  // Packages whose every armed member is currently ON. Used by the "ON wins"
  // rule: turning a package OFF keeps a shared armed member enabled when
  // another fully-on package still needs it.
  const onSetIds = useMemo(() => {
    const out = new Set<string>();
    for (const s of sets) {
      const live = armedLiveBySet.get(s.id) ?? [];
      if (
        live.length > 0 &&
        live.every((id) => rowOn(policyById.get(id)!, enabledSet.has(id)))
      ) {
        out.add(s.id);
      }
    }
    return out;
  }, [sets, armedLiveBySet, policyById, enabledSet]);

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
  const [selection, setSelection] = useState<Set<string>>(new Set());
  const [toasts, setToasts] = useState<ToastMsg[]>([]);
  const [chooserOpen, setChooserOpen] = useState(false);
  // Packages expand IN PLACE in the left panel (a dropdown of their members);
  // the right table always shows the full list (scope is all/loose only).
  const [expandedPkgs, setExpandedPkgs] = useState<Set<string>>(new Set());
  const toggleExpand = (id: string) =>
    setExpandedPkgs((prev) => {
      const n = new Set(prev);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  const expandPkg = (id: string) =>
    setExpandedPkgs((prev) => new Set(prev).add(id));

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

  // Build a full `putPolicySet` payload from an existing set, applying a patch.
  // `putPolicySet` is a full overwrite, so we must echo every preserved field.
  const setToOpts = (s: PolicySet, patch: Partial<PolicySet>) => {
    const muted = patch.mutedMemberIds ?? s.mutedMemberIds;
    return {
      id: s.id,
      displayName: patch.displayName ?? s.displayName,
      memberIds: patch.memberIds ?? s.memberIds,
      ...(muted ? { mutedMemberIds: muted } : {}),
      ...(s.description != null ? { description: s.description } : {}),
      ...(s.source ? { source: s.source } : {}),
      ...(s.readOnly !== undefined ? { readOnly: s.readOnly } : {}),
      ...(s.cat ? { cat: s.cat } : {}),
      ...(s.sourceListingId ? { sourceListingId: s.sourceListingId } : {}),
      ...(s.sourceVersion ? { sourceVersion: s.sourceVersion } : {}),
    };
  };

  const createEmptyPackage = async () => {
    const stamp = Date.now().toString(36);
    const setId = dashboardSetId(`pkg-${stamp}`);
    try {
      await putPolicySet({
        id: setId,
        displayName: "새 패키지",
        memberIds: [],
        source: "mine",
      });
      await qc.invalidateQueries({ queryKey: ["policy-sets"] });
      expandPkg(setId);
      pushToast("빈 패키지를 만들었어요 — 정책을 끌어다 넣어보세요");
    } catch (err) {
      console.error("[v2 list] createEmptyPackage failed:", err);
      pushToast("패키지를 만들지 못했어요");
    }
  };

  const renamePackage = async (s: PolicySet, name: string) => {
    const trimmed = name.trim();
    if (!trimmed || trimmed === s.displayName) return;
    try {
      await putPolicySet(setToOpts(s, { displayName: trimmed }));
      await qc.invalidateQueries({ queryKey: ["policy-sets"] });
    } catch (err) {
      console.error("[v2 list] renamePackage failed:", err);
      pushToast("이름을 바꾸지 못했어요");
    }
  };

  // Drop policies onto a package → union into its members (dedup; a policy may
  // belong to many packages).
  const addToPackage = async (setId: string, ids: string[]) => {
    const s = sets.find((x) => x.id === setId);
    if (!s || s.readOnly) return;
    const merged = new Set([...s.memberIds, ...ids]);
    const added = merged.size - s.memberIds.length;
    if (added === 0) {
      pushToast("이미 패키지에 들어있어요");
      return;
    }
    try {
      await putPolicySet(setToOpts(s, { memberIds: [...merged] }));
      await qc.invalidateQueries({ queryKey: ["policy-sets"] });
      expandPkg(setId);
      pushToast(`${s.displayName}에 ${added}개 추가했어요`);
    } catch (err) {
      console.error("[v2 list] addToPackage failed:", err);
      pushToast("패키지에 넣지 못했어요");
    }
  };

  // Remove one policy from a package (the dropdown's × button).
  const removeFromPackage = async (setId: string, policyId: string) => {
    const s = sets.find((x) => x.id === setId);
    if (!s || s.readOnly) return;
    const next = s.memberIds.filter((id) => id !== policyId);
    if (next.length === s.memberIds.length) return;
    const muted = (s.mutedMemberIds ?? []).filter((id) => id !== policyId);
    try {
      await putPolicySet(setToOpts(s, { memberIds: next, mutedMemberIds: muted }));
      await qc.invalidateQueries({ queryKey: ["policy-sets"] });
      pushToast(`${s.displayName}에서 뺐어요`);
    } catch (err) {
      console.error("[v2 list] removeFromPackage failed:", err);
      pushToast("빼지 못했어요");
    }
  };

  // Package on/off — acts ONLY on ARMED members (muted ones are untouched),
  // with "ON wins": turning a package off leaves an armed member enabled when
  // another fully-on package still arms it.
  const togglePackage = (s: PolicySet, on: boolean) => {
    const live = armedLiveBySet.get(s.id) ?? [];
    if (live.length === 0) return;
    if (on) {
      setManyEnabled(live, true);
      pushToast(`${s.displayName} 켰어요`);
      return;
    }
    const keep = new Set<string>();
    for (const other of sets) {
      if (other.id === s.id || !onSetIds.has(other.id)) continue;
      for (const id of armedLiveBySet.get(other.id) ?? []) keep.add(id);
    }
    const toDisable = live.filter((id) => !keep.has(id));
    setManyEnabled(toDisable, false);
    const kept = live.length - toDisable.length;
    pushToast(
      kept > 0
        ? `${s.displayName} 껐어요 (공유 ${kept}개는 유지)`
        : `${s.displayName} 껐어요`,
    );
  };

  // Per-member "armed" flag inside a package (the dropdown toggle): decides
  // whether activating the package turns this policy on. Independent of the
  // policy's global enabled bit; muting does NOT change the policy's state.
  const toggleArmed = async (s: PolicySet, policyId: string, armed: boolean) => {
    if (s.readOnly) return;
    const muted = new Set(s.mutedMemberIds ?? []);
    if (armed) muted.delete(policyId);
    else muted.add(policyId);
    try {
      await putPolicySet(setToOpts(s, { mutedMemberIds: [...muted] }));
      await qc.invalidateQueries({ queryKey: ["policy-sets"] });
    } catch (err) {
      console.error("[v2 list] toggleArmed failed:", err);
      pushToast("바꾸지 못했어요");
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
            onCreate={() => void createEmptyPackage()}
            onTogglePackage={togglePackage}
            onDropPolicies={(setId, ids) => void addToPackage(setId, ids)}
            onRename={(s, name) => void renamePackage(s, name)}
            expandedPkgs={expandedPkgs}
            onToggleExpand={toggleExpand}
            onRemoveFromPackage={(setId, pid) => void removeFromPackage(setId, pid)}
            onToggleArmed={(s, pid, armed) => void toggleArmed(s, pid, armed)}
            onOpenPolicy={(id) => navigate(`/editor/${encodeURIComponent(id)}`)}
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
              onRename={(name) => {
                if (activePkg) void renamePackage(activePkg, name);
              }}
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
                <div className="ev2-table compact">
                  <div className="ev2-thead">
                    <div className="ev2-c-sel">
                      <button
                        type="button"
                        className={`ev2-selbox head${selection.size > 0 ? " on" : ""}`}
                        onClick={() => {
                          if (selection.size > 0) clearSel();
                          else setSelection(new Set(filteredRows.map((r) => r.id)));
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
                        packageCount={pkgCountByPolicy.get(p.id) ?? 0}
                        dragIds={
                          selection.has(p.id) && selection.size > 1
                            ? [...selection]
                            : [p.id]
                        }
                        onSelect={() => onSelect(p.id)}
                        onToggle={(on) => togglePolicy(p.id, on)}
                        onOpen={() =>
                          navigate(`/editor/${encodeURIComponent(p.id)}`)
                        }
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
  onCreate: () => void;
  onTogglePackage: (s: PolicySet, on: boolean) => void;
  onDropPolicies: (setId: string, ids: string[]) => void;
  onRename: (s: PolicySet, name: string) => void;
  expandedPkgs: Set<string>;
  onToggleExpand: (id: string) => void;
  onRemoveFromPackage: (setId: string, policyId: string) => void;
  onToggleArmed: (s: PolicySet, policyId: string, armed: boolean) => void;
  onOpenPolicy: (id: string) => void;
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
    onCreate,
    onTogglePackage,
    onDropPolicies,
    onRename,
    expandedPkgs,
    onToggleExpand,
    onRemoveFromPackage,
    onToggleArmed,
    onOpenPolicy,
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
          <span className="ev2-spc" />
          <button
            type="button"
            className="ev2-pkg-add"
            onClick={onCreate}
            title="빈 패키지 만들기"
          >
            <PlusIcon />새 패키지
          </button>
        </div>

        <div className="ev2-left-grp">
          {sets.map((s) => {
            const memberIds = setMembership.get(s.id) ?? new Set<string>();
            const muted = new Set(s.mutedMemberIds ?? []);
            // Package state reflects only ARMED members (memberIds − muted).
            const armedLive = [...memberIds].filter((id) => {
              const m = policyById.get(id);
              return m && !isDraft(m) && !muted.has(id);
            });
            const onCount = armedLive.filter((id) =>
              rowOn(policyById.get(id)!, enabledSet.has(id)),
            ).length;
            const pkgState =
              armedLive.length === 0
                ? "empty"
                : onCount === 0
                  ? "off"
                  : onCount === armedLive.length
                    ? "on"
                    : "partial";
            const mutedCount = [...memberIds].filter((id) => muted.has(id)).length;
            // Lock "포함" (armed) editing while the package is ON — changing
            // inclusion then would desync members from their global state.
            // Turn the package off first to re-pick included policies.
            const armedLocked = s.readOnly || pkgState === "on";
            const market = isMarketSource(s);
            const cstyle = catStyle(s.cat);
            const expanded = expandedPkgs.has(s.id);
            const members = [...memberIds]
              .map((id) => policyById.get(id))
              .filter((p): p is ManagedPolicy => !!p);
            return (
              <div key={s.id} className="ev2-pkg-item">
                <PackBtn
                  active={expanded}
                  expanded={expanded}
                  onClick={() => onToggleExpand(s.id)}
                  icon={
                    <span style={{ color: cstyle.hex, display: "grid", placeItems: "center" }}>
                      <FolderIcon />
                    </span>
                  }
                  name={s.displayName}
                  sub={
                    <>
                      <b>{onCount}</b>/{armedLive.length} 켜짐
                      {mutedCount > 0 && (
                        <span className="ev2-pk-muted"> · {mutedCount} 제외</span>
                      )}
                    </>
                  }
                  source={
                    market ? (
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
                  pkgState={pkgState}
                  onTogglePkg={
                    pkgState === "empty"
                      ? undefined
                      : (on) => onTogglePackage(s, on)
                  }
                  onRename={
                    s.readOnly ? undefined : (name) => onRename(s, name)
                  }
                  onDropPolicies={
                    s.readOnly ? undefined : (ids) => onDropPolicies(s.id, ids)
                  }
                />
                {expanded && (
                  <div className="ev2-pkg-members">
                    {members.length === 0 ? (
                      <div className="ev2-pkg-mini-empty">
                        비어 있어요 — 오른쪽에서 정책을 끌어다 넣으세요
                      </div>
                    ) : (
                      members.map((m) => {
                        const armed = !muted.has(m.id);
                        return (
                          <div
                            key={m.id}
                            className={`ev2-pkg-mrow${armed ? "" : " muted"}`}
                          >
                            <button
                              type="button"
                              className={`ev2-pkg-mtg${armed ? " on" : ""}`}
                              disabled={armedLocked}
                              onClick={(e) => {
                                e.stopPropagation();
                                if (!armedLocked) onToggleArmed(s, m.id, !armed);
                              }}
                              title={
                                armedLocked && !s.readOnly
                                  ? "패키지가 켜져 있을 땐 변경 불가 — 패키지를 끄고 포함을 바꾸세요"
                                  : armed
                                    ? "패키지를 켤 때 이 정책 포함 (끄면 제외)"
                                    : "제외됨 — 켜면 패키지 활성화 시 포함"
                              }
                            >
                              <span className="sw" />
                            </button>
                            <span
                              className="ev2-pkg-mnm"
                              onClick={() => onOpenPolicy(m.id)}
                              title="에디터 열기"
                            >
                              {nameFromPolicy(m)}
                            </span>
                            {!s.readOnly && (
                              <button
                                type="button"
                                className="ev2-pkg-mrm"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  onRemoveFromPackage(s.id, m.id);
                                }}
                                title="패키지에서 빼기"
                              >
                                <XIcon />
                              </button>
                            )}
                          </div>
                        );
                      })
                    )}
                  </div>
                )}
              </div>
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
  /** When defined, the row is an expandable package: shows a rotating caret. */
  expanded?: boolean;
  pkgState?: "on" | "off" | "partial" | "empty";
  onTogglePkg?: (on: boolean) => void;
  onRename?: (name: string) => void;
  onDropPolicies?: (ids: string[]) => void;
}) {
  const {
    active,
    onClick,
    icon,
    name,
    sub,
    source,
    right,
    badge,
    expanded,
    pkgState,
    onTogglePkg,
    onRename,
    onDropPolicies,
  } = props;
  const [dragOver, setDragOver] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draftName, setDraftName] = useState("");

  const dropProps = onDropPolicies
    ? {
        onDragOver: (e: React.DragEvent) => {
          if (!e.dataTransfer.types.includes(DRAG_MIME)) return;
          e.preventDefault();
          e.dataTransfer.dropEffect = "copy";
          setDragOver(true);
        },
        onDragLeave: () => setDragOver(false),
        onDrop: (e: React.DragEvent) => {
          e.preventDefault();
          setDragOver(false);
          const raw = e.dataTransfer.getData(DRAG_MIME);
          if (!raw) return;
          try {
            const ids = JSON.parse(raw);
            if (Array.isArray(ids) && ids.length > 0) onDropPolicies(ids);
          } catch {
            /* ignore malformed payload */
          }
        },
      }
    : {};

  const commitRename = () => {
    if (onRename) onRename(draftName);
    setEditing(false);
  };

  return (
    <div
      className={`ev2-pk${active ? " active" : ""}${dragOver ? " dragover" : ""}`}
      role="button"
      tabIndex={0}
      onClick={() => {
        if (!editing) onClick();
      }}
      onKeyDown={(e) => {
        if (editing) return;
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
      {...dropProps}
    >
      {expanded !== undefined && (
        <span className={`ev2-pk-caret${expanded ? " open" : ""}`}>
          <CaretRightIcon />
        </span>
      )}
      <span className="ev2-pk-ic">{icon}</span>
      <span className="ev2-pk-body">
        <span className="ev2-pk-nm">
          {editing && onRename ? (
            <input
              className="ev2-pk-rename"
              autoFocus
              value={draftName}
              onChange={(e) => setDraftName(e.target.value)}
              onClick={(e) => e.stopPropagation()}
              onBlur={commitRename}
              onKeyDown={(e) => {
                e.stopPropagation();
                if (e.key === "Enter") commitRename();
                else if (e.key === "Escape") setEditing(false);
              }}
            />
          ) : (
            <span
              onDoubleClick={
                onRename
                  ? (e) => {
                      e.stopPropagation();
                      setDraftName(typeof name === "string" ? name : "");
                      setEditing(true);
                    }
                  : undefined
              }
              title={onRename ? "더블클릭으로 이름 변경" : undefined}
            >
              {name}
            </span>
          )}
          {badge}
        </span>
        {sub && <span className="ev2-pk-sub">{sub}</span>}
        {source && <span className="ev2-pk-src">{source}</span>}
      </span>
      {onTogglePkg && pkgState && pkgState !== "empty" ? (
        <span className="ev2-pk-right">
          <button
            type="button"
            className={`ev2-pk-tg ${pkgState}`}
            onClick={(e) => {
              e.stopPropagation();
              onTogglePkg(pkgState !== "on");
            }}
            title={pkgState === "on" ? "패키지 끄기" : "패키지 켜기"}
          >
            <span className="sw" />
          </button>
        </span>
      ) : right ? (
        <span className="ev2-pk-right">{right}</span>
      ) : null}
    </div>
  );
}

/* ─────────────── Scope Header ─────────────── */
function ScopeHeader(props: {
  scope: ListScope;
  activePkg: PolicySet | null;
  rowCount: number;
  onClearScope: () => void;
  onRename: (name: string) => void;
}) {
  const { scope, activePkg, rowCount, onClearScope, onRename } = props;
  const [editing, setEditing] = useState(false);
  const [draftName, setDraftName] = useState("");
  const title =
    scope.type === "all"
      ? "전체"
      : scope.type === "loose"
        ? "단일 정책"
        : activePkg?.displayName ?? "";
  const canRename = !!activePkg && !activePkg.readOnly;
  const commit = () => {
    onRename(draftName);
    setEditing(false);
  };
  return (
    <div className="ev2-scopehd">
      <div className="ev2-scope-title">
        {editing && canRename ? (
          <input
            className="ev2-scope-rename"
            autoFocus
            value={draftName}
            onChange={(e) => setDraftName(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === "Enter") commit();
              else if (e.key === "Escape") setEditing(false);
            }}
          />
        ) : (
          <span
            className="t"
            onDoubleClick={
              canRename
                ? () => {
                    setDraftName(title);
                    setEditing(true);
                  }
                : undefined
            }
            title={canRename ? "더블클릭으로 이름 변경" : undefined}
          >
            {title}
          </span>
        )}
        <span className="ct">{rowCount}개</span>
        {activePkg && isMarketSource(activePkg) && (
          <span className="ev2-scope-prov">
            <ShieldIcon /> 마켓에서 가져옴
            {activePkg.sourceVersion ? ` · ${activePkg.sourceVersion}` : ""}
          </span>
        )}
        {activePkg && !isMarketSource(activePkg) && (
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
  packageCount: number;
  dragIds: string[];
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
    packageCount,
    dragIds,
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
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(DRAG_MIME, JSON.stringify(dragIds));
        e.dataTransfer.effectAllowed = "copy";
      }}
      onClick={(e) => {
        const target = e.target as HTMLElement;
        if (target.closest("button,.ev2-selbox,.ev2-tg,.ev2-grip")) return;
        if (e.shiftKey) onSelect();
        else onOpen();
      }}
    >
      <div className="ev2-c-sel">
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
        <span
          className="ev2-grip"
          title={
            dragIds.length > 1
              ? `드래그해서 패키지에 넣기 (${dragIds.length}개)`
              : "드래그해서 패키지에 넣기"
          }
        >
          <GripIcon />
        </span>
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
            {packageCount > 1 && (
              <span
                className="ev2-badge-pkg"
                title={`${packageCount}개 패키지에 포함됨`}
              >
                <FolderIcon />
                {packageCount}개 패키지
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
