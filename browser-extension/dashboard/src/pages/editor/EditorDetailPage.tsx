import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router-dom";

import { listManagedPolicies } from "../../server-api";
import { Topbar } from "../../shell/Topbar";

import { EditorPanel } from "./EditorPanel";
import { nameFromPolicy } from "./policy-meta";
import "../editor.css";

/**
 * `/editor/:id` — load the matching policy from the cached list and
 * render `<EditorPanel mode="edit">`. On delete, navigate back to the
 * list.
 */
export function EditorDetailPage() {
  const navigate = useNavigate();
  const params = useParams<{ id: string }>();
  const id = params.id ? decodeURIComponent(params.id) : "";

  const listQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
  });

  const policy = useMemo(
    () => listQ.data?.find((p) => p.id === id) ?? null,
    [listQ.data, id],
  );

  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle={policy ? nameFromPolicy(policy) : id || "…"}
        right={
          <Link to="/editor" className="back-link">
            ← 설치된 정책
          </Link>
        }
      />
      <div className="editor-main editor-main-solo">
        {listQ.isLoading && (
          <div className="empty-editor"><div>불러오는 중…</div></div>
        )}
        {!listQ.isLoading && !policy && (
          <div className="empty-editor">
            <div>
              <strong>정책을 찾을 수 없습니다</strong>
              ID: <code>{id}</code>
              <br />
              <Link to="/editor">← 목록으로 돌아가기</Link>
            </div>
          </div>
        )}
        {policy && (
          <EditorPanel
            mode="edit"
            policy={policy}
            onSaved={(savedId) => {
              if (savedId !== id) {
                navigate(`/editor/${encodeURIComponent(savedId)}`, {
                  replace: true,
                });
              }
            }}
            onDeleted={() => navigate("/editor")}
          />
        )}
      </div>
    </>
  );
}
