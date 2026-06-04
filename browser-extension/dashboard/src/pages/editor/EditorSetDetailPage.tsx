import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router-dom";

import {
  dashboardSetId,
  listPolicySets,
  stripDashboardSetId,
} from "../../server-api";
import { Topbar } from "../../shell/Topbar";

import { EditorSetPanel } from "./EditorSetPanel";
import { PublishModal, type PublishSource } from "./PublishModal";
import "../editor.css";

export function EditorSetDetailPage() {
  const navigate = useNavigate();
  const params = useParams<{ setId: string }>();
  const slug = params.setId ? decodeURIComponent(params.setId) : "";
  const fullId = slug ? dashboardSetId(slug) : "";

  const setsQ = useQuery({
    queryKey: ["policy-sets"],
    queryFn: listPolicySets,
  });

  const set = useMemo(
    () => setsQ.data?.find((s) => s.id === fullId) ?? null,
    [setsQ.data, fullId],
  );

  const [publishOpen, setPublishOpen] = useState(false);
  const publishSource: PublishSource | null = useMemo(() => {
    if (!set) return null;
    return {
      kind: "set",
      suggestedDisplayName: set.displayName,
      suggestedSlug: stripDashboardSetId(set.id),
      description: set.description,
      memberIds: set.memberIds,
    };
  }, [set]);

  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle={set ? set.displayName : slug || "…"}
        right={
          <>
            {set && (
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
              ← 정책 목록
            </Link>
          </>
        }
      />
      <div className="editor-main editor-main-solo">
        {setsQ.isLoading && (
          <div className="empty-editor"><div>불러오는 중…</div></div>
        )}
        {!setsQ.isLoading && !set && (
          <div className="empty-editor">
            <div>
              <strong>패키지를 찾을 수 없습니다</strong>
              ID: <code>{slug}</code>
              <br />
              <Link to="/editor">← 목록으로 돌아가기</Link>
            </div>
          </div>
        )}
        {set && (
          <EditorSetPanel
            mode="edit"
            set={set}
            onSaved={(savedId) => {
              const savedSlug = stripDashboardSetId(savedId);
              if (savedSlug !== slug) {
                navigate(`/editor/sets/${encodeURIComponent(savedSlug)}`, {
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
