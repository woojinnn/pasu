import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";

import {
  ENABLED_IDS_STORAGE_KEY,
  getEnabledPolicyIds,
  listManagedPolicies,
  setEnabledPolicyIds,
  subscribeToBroadcast,
  type ManagedPolicy,
} from "../../server-api";
import { Topbar } from "../../shell/Topbar";

import { nameFromPolicy, severityFromCedar } from "./policy-meta";
import "../editor.css";

/**
 * Card-grid landing for `/editor`. Each card is a full-area button
 * into `/editor/:id` plus an "enabled" checkbox that mirrors the
 * extension popup's checkbox column — flipping it here flips it there
 * (and vice versa) because both UIs read/write the same SW storage
 * key (`policy-selection:enabled-ids`).
 */
export function EditorListPage() {
  const navigate = useNavigate();
  const qc = useQueryClient();

  const listQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
  });

  const enabledQ = useQuery({
    queryKey: ["enabled-policy-ids"],
    queryFn: getEnabledPolicyIds,
  });

  // Bidirectional sync: when the popup (or another tab) toggles a
  // checkbox, the SW writes `policy-selection:enabled-ids`, the
  // dashboard-bridge content script broadcasts the change to this
  // page, and we refetch our cached set so the UI re-renders.
  useEffect(() => {
    const unsubscribe = subscribeToBroadcast((keys) => {
      if (keys.includes(ENABLED_IDS_STORAGE_KEY)) {
        void qc.invalidateQueries({ queryKey: ["enabled-policy-ids"] });
      }
    });
    return unsubscribe;
  }, [qc]);

  const toggleMut = useMutation({
    mutationFn: async (next: string[]) => {
      await setEnabledPolicyIds(next);
      return next;
    },
    // Optimistic update so the checkbox flips immediately while the SW
    // round-trip (+ engine reinstall) is in flight. The broadcast above
    // will reconcile if the SW rejects.
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
  const toggle = (id: string, checked: boolean) => {
    const next = new Set(enabledSet);
    if (checked) next.add(id);
    else next.delete(id);
    toggleMut.mutate([...next]);
  };

  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle={listQ.data ? `${listQ.data.length} policies` : "…"}
        right={
          <Link to="/editor/new" className="btn-primary new-policy-btn">
            + 새 정책
          </Link>
        }
      />

      <div className="policy-grid-wrap">
        <header className="policy-grid-head">
          <h2>
            설치된 정책 <span className="cnt">{listQ.data?.length ?? 0}</span>
          </h2>
          <p className="hint">
            체크박스로 활성/비활성 토글, 카드 클릭으로 편집기로 이동. 익스텐션
            팝업과 실시간 동기화됩니다.
          </p>
        </header>

        {listQ.isLoading && (
          <div className="policy-grid-status">불러오는 중…</div>
        )}

        {listQ.data && listQ.data.length === 0 && (
          <div className="policy-grid-empty">
            <p>아직 설치된 정책이 없습니다.</p>
            <Link to="/editor/new" className="btn-primary">
              + 새 정책 만들기
            </Link>
          </div>
        )}

        {listQ.data && listQ.data.length > 0 && (
          <div className="policy-grid">
            {listQ.data.map((p) => (
              <PolicyCard
                key={p.id}
                policy={p}
                enabled={enabledSet.has(p.id)}
                onToggle={(checked) => toggle(p.id, checked)}
                onOpen={() => navigate(`/editor/${encodeURIComponent(p.id)}`)}
              />
            ))}
          </div>
        )}
      </div>
    </>
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
      {/* Checkbox lives outside the navigation button so clicking it
          doesn't propagate up and open the detail page. */}
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
