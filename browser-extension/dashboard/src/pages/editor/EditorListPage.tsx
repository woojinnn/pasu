import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";

import { listManagedPolicies, type ManagedPolicy } from "../../server-api";
import { Topbar } from "../../shell/Topbar";

import { nameFromPolicy, severityFromCedar } from "./policy-meta";
import "../editor.css";

/**
 * Card-grid landing for `/editor`. Each card is a full-area button into
 * `/editor/:id`. Empty state is a single CTA into `/editor/new`.
 */
export function EditorListPage() {
  const navigate = useNavigate();
  const listQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
  });

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
            활성화된 트랜잭션·서명 정책을 관리하세요. 카드를 클릭하면 편집기로
            이동합니다.
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
  onOpen,
}: {
  policy: ManagedPolicy;
  onOpen: () => void;
}) {
  const sev = severityFromCedar(policy.text);
  const name = nameFromPolicy(policy);
  const mode = policy.policyTree ? "Builder" : "Code";
  return (
    <button className={`policy-card ${sev}`} onClick={onOpen} type="button">
      <div className="pc-head">
        <span className="pc-name">{name}</span>
        <span className={`sev ${sev}`}>{sev}</span>
      </div>
      {policy.displayName && policy.displayName.trim() !== name && (
        <p className="pc-desc">{policy.displayName}</p>
      )}
      <div className="pc-foot">
        <span className="pc-id">{policy.id}</span>
        <span className="pc-mode">{mode}</span>
      </div>
    </button>
  );
}
