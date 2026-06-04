import { Link, useNavigate } from "react-router-dom";

import { Topbar } from "../../shell/Topbar";
import { stripDashboardSetId } from "../../server-api";

import { EditorSetPanel } from "./EditorSetPanel";
import "../editor.css";

export function EditorSetNewPage() {
  const navigate = useNavigate();
  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle="새 패키지"
        right={
          <Link to="/editor" className="back-link">
            ← 정책 목록
          </Link>
        }
      />
      <div className="editor-main editor-main-solo">
        <EditorSetPanel
          mode="new"
          onSaved={(id) =>
            navigate(`/editor/sets/${encodeURIComponent(stripDashboardSetId(id))}`)
          }
        />
      </div>
    </>
  );
}
