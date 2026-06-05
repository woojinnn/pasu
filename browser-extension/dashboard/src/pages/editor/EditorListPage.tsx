import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";

import {
  ENABLED_IDS_STORAGE_KEY,
  dashboardId,
  getEnabledPolicyIds,
  listManagedPolicies,
  listPolicySets,
  putPolicy,
  setEnabledPolicyIds,
  stripDashboardSetId,
  subscribeToBroadcast,
  type ManagedPolicy,
  type PolicySet,
} from "../../server-api";
import { Topbar } from "../../shell/Topbar";
import { FEATURES } from "../../features";
import { EditorListPageV2 } from "./v2/EditorListPageV2";

import { nameFromPolicy, severityFromCedar } from "./policy-meta";
import "../editor.css";

/**
 * Router-exposed entry. Delegates to the v2 list when the
 * `newListView` flag is on; otherwise renders the legacy card grid.
 */
export function EditorListPage() {
  if (FEATURES.newListView) return <EditorListPageV2 />;
  return <EditorListPageLegacy />;
}

/**
 * Card-grid landing for `/editor` — legacy implementation kept until
 * the v2 flag bakes. Policies belonging to a user-defined set are
 * grouped together with a bulk-toggle checkbox; policies not in any
 * set fall into the "ungrouped" section. The enabled-ids contract is
 * unchanged — set toggles fan out into individual member toggles.
 */
function EditorListPageLegacy() {
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
      // Post-namespacing the SW emits keys like
      // `policy-selection:enabled-ids:user_abc`, so we prefix-match instead
      // of equality-check. Also fires on the legacy flat key in case any
      // unmigrated path still writes it.
      const enabledTouched = keys.some(
        (k) =>
          k === ENABLED_IDS_STORAGE_KEY ||
          k.startsWith(`${ENABLED_IDS_STORAGE_KEY}:`),
      );
      // The active-user discriminator flipping (login / logout / account
      // switch) invalidates EVERY per-user cache simultaneously.
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

  const enabledSet = new Set(enabledQ.data ?? []);
  const togglePolicy = (id: string, checked: boolean) => {
    const next = new Set(enabledSet);
    if (checked) next.add(id);
    else next.delete(id);
    toggleMut.mutate([...next]);
  };

  /** Bulk toggle for a set: enable all members if the set is not fully
   *  on, otherwise disable all members. Stale member ids (set references a
   *  policy that no longer exists) are still added/removed so the
   *  enabled-ids state stays in sync with the set's stated intent. */
  const toggleSet = (memberIds: readonly string[], desiredOn: boolean) => {
    const next = new Set(enabledSet);
    if (desiredOn) {
      for (const id of memberIds) next.add(id);
    } else {
      for (const id of memberIds) next.delete(id);
    }
    toggleMut.mutate([...next]);
  };

  const { groupedSets, ungrouped } = useMemo(() => {
    const policies = listQ.data ?? [];
    const sets = setsQ.data ?? [];
    const byId = new Map(policies.map((p) => [p.id, p]));

    // A policy is "grouped" if any set references it. Many-to-many: it may
    // render under several sets, but only once in the ungrouped list.
    const claimedIds = new Set<string>();
    const groupedSets: Array<{ set: PolicySet; members: ManagedPolicy[] }> = [];
    for (const set of sets) {
      const members: ManagedPolicy[] = [];
      for (const mid of set.memberIds) {
        const p = byId.get(mid);
        if (p) {
          members.push(p);
          claimedIds.add(mid);
        }
      }
      groupedSets.push({ set, members });
    }
    const ungrouped = policies.filter((p) => !claimedIds.has(p.id));
    return { groupedSets, ungrouped };
  }, [listQ.data, setsQ.data]);

  const totalPolicies = listQ.data?.length ?? 0;
  const totalSets = setsQ.data?.length ?? 0;

  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle={
          listQ.data
            ? `${totalPolicies} policies · ${totalSets} packages`
            : "…"
        }
        right={
          <>
            {import.meta.env.DEV && <SeedPhase1ADefaultsButton />}
            <Link to="/editor/sets/new" className="btn-secondary new-set-btn">
              + 새 패키지
            </Link>
            <Link to="/editor/new" className="btn-primary new-policy-btn">
              + 새 정책
            </Link>
          </>
        }
      />

      <div className="policy-grid-wrap">
        <header className="policy-grid-head">
          <h2>
            설치된 정책 <span className="cnt">{totalPolicies}</span>
          </h2>
          <p className="hint">
            정책 패키지로 묶어 한 번에 켜고 끌 수 있습니다. 익스텐션 팝업과 실시간
            동기화됩니다.
          </p>
        </header>

        {listQ.isLoading && (
          <div className="policy-grid-status">불러오는 중…</div>
        )}

        {listQ.data && totalPolicies === 0 && totalSets === 0 && (
          <div className="policy-grid-empty">
            <p>아직 설치된 정책이 없습니다.</p>
            <Link to="/editor/new" className="btn-primary">
              + 새 정책 만들기
            </Link>
          </div>
        )}

        {groupedSets.map(({ set, members }) => {
          const enabledCount = members.filter((m) => enabledSet.has(m.id)).length;
          const fullyOn = members.length > 0 && enabledCount === members.length;
          const partiallyOn = enabledCount > 0 && enabledCount < members.length;
          const slug = stripDashboardSetId(set.id);
          return (
            <section className="set-group" key={set.id}>
              <header className="set-group-head">
                <label
                  className="sg-check"
                  title={fullyOn ? "패키지 전체 비활성화" : "패키지 전체 활성화"}
                  onClick={(e) => e.stopPropagation()}
                >
                  <input
                    type="checkbox"
                    checked={fullyOn}
                    ref={(el) => {
                      if (el) el.indeterminate = partiallyOn;
                    }}
                    onChange={() => toggleSet(set.memberIds, !fullyOn)}
                    aria-label={`${set.displayName} 일괄 토글`}
                  />
                </label>
                <span className="sg-name">{set.displayName}</span>
                {set.description && (
                  <span className="sg-meta">{set.description}</span>
                )}
                <span className="sg-counter">
                  {enabledCount}/{members.length}
                </span>
                <Link
                  to={`/editor/sets/${encodeURIComponent(slug)}`}
                  className="sg-edit"
                >
                  편집
                </Link>
              </header>
              {members.length > 0 && (
                <div className="set-group-body">
                  <div className="policy-grid">
                    {members.map((p) => (
                      <PolicyCard
                        key={`${set.id}::${p.id}`}
                        policy={p}
                        enabled={enabledSet.has(p.id)}
                        onToggle={(checked) => togglePolicy(p.id, checked)}
                        onOpen={() =>
                          navigate(`/editor/${encodeURIComponent(p.id)}`)
                        }
                      />
                    ))}
                  </div>
                </div>
              )}
            </section>
          );
        })}

        {ungrouped.length > 0 && (
          <>
            {totalSets > 0 && (
              <div className="ungrouped-head">미분류 ({ungrouped.length})</div>
            )}
            <div className="policy-grid">
              {ungrouped.map((p) => (
                <PolicyCard
                  key={p.id}
                  policy={p}
                  enabled={enabledSet.has(p.id)}
                  onToggle={(checked) => togglePolicy(p.id, checked)}
                  onOpen={() => navigate(`/editor/${encodeURIComponent(p.id)}`)}
                />
              ))}
            </div>
          </>
        )}
      </div>
    </>
  );
}

function SeedPhase1ADefaultsButton() {
  const qc = useQueryClient();
  const [status, setStatus] = useState<
    | { kind: "idle" }
    | { kind: "running"; done: number; total: number }
    | { kind: "done"; ok: number; failed: number }
  >({ kind: "idle" });

  async function runSeed() {
    const { default: bundles } = (await import("./phase1A-seed.json")) as {
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

function PolicyCard({
  policy,
  enabled,
  onToggle,
  onOpen,
}: {
  policy: ManagedPolicy;
  enabled: boolean;
  onToggle: (checked: boolean) => void;
  onOpen: () => void;
}) {
  const sev = severityFromCedar(policy.text);
  const name = nameFromPolicy(policy);
  return (
    <div className={`policy-card ${sev}${enabled ? " is-enabled" : ""}`}>
      <label
        className="pc-check"
        onClick={(e) => e.stopPropagation()}
        title={enabled ? "정책 비활성화" : "정책 활성화"}
      >
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => onToggle(e.target.checked)}
          aria-label={`${name} 활성화`}
        />
      </label>

      <button className="pc-open" type="button" onClick={onOpen}>
        <div className="pc-head">
          <span className="pc-name">{name}</span>
          <span className={`sev ${sev}`}>{sev}</span>
        </div>
        {policy.displayName && policy.displayName.trim() !== name && (
          <p className="pc-desc">{policy.displayName}</p>
        )}
        <div className="pc-foot">
          <span className="pc-id">{policy.id}</span>
        </div>
      </button>
    </div>
  );
}
