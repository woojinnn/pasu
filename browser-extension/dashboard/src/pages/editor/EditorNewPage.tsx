import { Link, useNavigate } from "react-router-dom";

import { Topbar } from "../../shell/Topbar";

import { EditorPanel } from "./EditorPanel";
import "../editor.css";

/**
 * `/editor/new` — empty editor for a fresh policy. On save, navigates to
 * `/editor/:id` so subsequent edits hit the detail route.
 */
export function EditorNewPage() {
  const navigate = useNavigate();
  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle="새 정책"
        right={
          <Link to="/editor" className="back-link">
            ← 설치된 정책
          </Link>
        }
      />
      <div className="editor-main editor-main-solo">
        <EditorPanel
          mode="new"
          onSaved={(id) => navigate(`/editor/${encodeURIComponent(id)}`)}
        />
      </div>
    </>
  );
}
