import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  dashboardSetId,
  deletePolicySet,
  listManagedPolicies,
  putPolicySet,
  stripDashboardSetId,
  type PolicySet,
} from "../../server-api";

import { nameFromPolicy } from "./policy-meta";

interface EditorSetPanelProps {
  mode: "new" | "edit";
  set?: PolicySet;
  onSaved: (id: string) => void;
  onDeleted?: () => void;
}

/**
 * Shared form for creating and editing a policy set. Membership is a
 * multi-select over the dashboard-managed policy list — the SW does not
 * validate that ids exist, so unchecked-then-rechecked policies remain
 * authoritative on save.
 */
export function EditorSetPanel({ mode, set, onSaved, onDeleted }: EditorSetPanelProps) {
  const qc = useQueryClient();

  const [displayName, setDisplayName] = useState(set?.displayName ?? "");
  const [description, setDescription] = useState(set?.description ?? "");
  const [memberIds, setMemberIds] = useState<Set<string>>(
    () => new Set(set?.memberIds ?? []),
  );
  const [slug, setSlug] = useState(set ? stripDashboardSetId(set.id) : "");

  const policiesQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
  });

  const saveMut = useMutation({
    mutationFn: async () => {
      const id = mode === "edit" && set ? set.id : dashboardSetId(slug.trim());
      await putPolicySet({
        id,
        displayName: displayName.trim(),
        description: description.trim() || undefined,
        memberIds: [...memberIds],
      });
      return id;
    },
    onSuccess: async (id) => {
      await qc.invalidateQueries({ queryKey: ["policy-sets"] });
      onSaved(id);
    },
  });

  const deleteMut = useMutation({
    mutationFn: async () => {
      if (!set) return;
      await deletePolicySet(set.id);
    },
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["policy-sets"] });
      onDeleted?.();
    },
  });

  const policies = policiesQ.data ?? [];
  const allEnabledMembers = useMemo(() => [...memberIds], [memberIds]);

  const toggleMember = (id: string) => {
    const next = new Set(memberIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setMemberIds(next);
  };

  const slugOk = mode === "edit" || /^[A-Za-z0-9_./()-]{1,128}$/.test(slug.trim());
  const canSave =
    displayName.trim().length > 0 && slugOk && !saveMut.isPending;

  return (
    <div className="set-panel">
      <header className="set-panel-head">
        <h2>{mode === "new" ? "새 정책 셋" : "셋 편집"}</h2>
        <p className="hint">
          정책 셋은 여러 정책을 묶어 한 번에 켜고 끌 수 있는 컨테이너입니다. 한
          정책은 여러 셋에 동시에 속할 수 있습니다.
        </p>
      </header>

      <div className="set-panel-form">
        <label className="set-field">
          <span className="set-field-label">이름</span>
          <input
            type="text"
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            placeholder="예: 컴플라이언스 셋"
            maxLength={120}
          />
        </label>

        {mode === "new" && (
          <label className="set-field">
            <span className="set-field-label">슬러그 (URL용)</span>
            <input
              type="text"
              value={slug}
              onChange={(e) => setSlug(e.target.value)}
              placeholder="compliance"
              maxLength={128}
            />
            <span className="set-field-hint">
              영문/숫자/<code>_.-()/</code> 만 허용. 한 번 저장하면 변경 불가.
            </span>
          </label>
        )}

        <label className="set-field">
          <span className="set-field-label">설명 (선택)</span>
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="이 셋이 어떤 목적인지 짧게 설명하세요."
            rows={3}
            maxLength={500}
          />
        </label>

        <div className="set-field">
          <span className="set-field-label">
            포함할 정책{" "}
            <span className="set-field-counter">{allEnabledMembers.length}개 선택</span>
          </span>
          {policiesQ.isLoading && <div className="hint">정책 목록 불러오는 중…</div>}
          {!policiesQ.isLoading && policies.length === 0 && (
            <div className="hint">
              아직 만든 정책이 없습니다. 먼저 정책을 만들어 주세요.
            </div>
          )}
          {policies.length > 0 && (
            <div className="set-member-list">
              {policies.map((p) => {
                const checked = memberIds.has(p.id);
                return (
                  <label
                    key={p.id}
                    className={`set-member-row${checked ? " is-checked" : ""}`}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggleMember(p.id)}
                    />
                    <span className="set-member-name">{nameFromPolicy(p)}</span>
                    <span className="set-member-id">{p.id}</span>
                  </label>
                );
              })}
            </div>
          )}
        </div>
      </div>

      <footer className="set-panel-foot">
        <button
          type="button"
          className="btn-primary"
          disabled={!canSave}
          onClick={() => saveMut.mutate()}
        >
          {saveMut.isPending ? "저장 중…" : mode === "new" ? "셋 만들기" : "변경 저장"}
        </button>
        {mode === "edit" && set && (
          <button
            type="button"
            className="btn-danger"
            disabled={deleteMut.isPending}
            onClick={() => {
              if (window.confirm(`셋 "${set.displayName}" 을(를) 삭제할까요?`)) {
                deleteMut.mutate();
              }
            }}
          >
            {deleteMut.isPending ? "삭제 중…" : "셋 삭제"}
          </button>
        )}
        {saveMut.isError && (
          <span className="set-error">
            저장 실패: {(saveMut.error as Error).message}
          </span>
        )}
        {deleteMut.isError && (
          <span className="set-error">
            삭제 실패: {(deleteMut.error as Error).message}
          </span>
        )}
      </footer>
    </div>
  );
}
