import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router-dom";

import { listManagedPolicies, stripDashboardId } from "../../server-api";
import { Topbar } from "../../shell/Topbar";

import { EditorPanel } from "./EditorPanel";
import { PublishModal, type PublishSource } from "./PublishModal";
import { nameFromPolicy } from "./policy-meta";
import "../editor.css";

/**
 * `/editor/:id` — load the matching policy from the cached list and
 * render `<EditorPanel mode="edit">`. On delete, navigate back to the
 * list. The Publish button mounts a modal that POSTs the current cedar
 * text to `/market/listings`.
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

  const [publishOpen, setPublishOpen] = useState(false);
  const publishSource: PublishSource | null = useMemo(() => {
    if (!policy) return null;
    return {
      kind: "policy",
      cedarText: policy.text,
      manifest: policy.manifest,
      policyTree: policy.policyTree ?? null,
      suggestedDisplayName: nameFromPolicy(policy),
      suggestedSlug: stripDashboardId(policy.id),
    };
  }, [policy]);

  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle={policy ? nameFromPolicy(policy) : id || "…"}
        right={
          <>
            {policy && (
              <button
                type="button"
                className="btn-secondary"
                onClick={() => setPublishOpen(true)}
                style={{ marginRight: 8 }}
              >
                ↑ Publish
              </button>
            )}
            <Link to="/editor" className="back-link">
              ← 설치된 정책
            </Link>
          </>
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
      <PublishModal
        open={publishOpen}
        source={publishSource}
        onClose={() => setPublishOpen(false)}
      />
    </>
  );
}
