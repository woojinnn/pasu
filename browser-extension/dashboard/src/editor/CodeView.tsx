import "./CodeView.css";

interface CodeViewProps {
  cedarText: string;
  editable: boolean;
  onChangeCedarText: (text: string) => void;
  onRequestEdit: () => void;
}

// Phase 1: read-only preview by default. Builder → Code is the only
// supported direction until the parse_cedar WASM is wired into the Code
// → Builder roundtrip flow (deferred to M-4'+).
export function CodeView({
  cedarText,
  editable,
  onChangeCedarText,
  onRequestEdit,
}: CodeViewProps) {
  return (
    <div className="code-view">
      {!editable ? (
        <div className="code-notice">
          <div className="code-notice-text">
            <strong>Read-only preview</strong>
            <span>
              Cedar 텍스트는 Builder에서 생성됩니다. 직접 편집하면 Builder
              로 돌아갈 때 현재 편집 내용이 폐기됩니다.
            </span>
          </div>
          <button
            type="button"
            className="code-edit-btn"
            onClick={onRequestEdit}
          >
            직접 편집
          </button>
        </div>
      ) : (
        <div className="code-notice code-notice-active">
          <strong>Code mode — 직접 편집 중</strong>
          <span>
            Builder로 돌아가면 현재 편집 내용은 폐기되고 가장 최근 Builder
            상태로 복원됩니다.
          </span>
        </div>
      )}
      {editable ? (
        <textarea
          className="code-block code-textarea"
          value={cedarText}
          spellCheck={false}
          onChange={(e) => onChangeCedarText(e.target.value)}
        />
      ) : (
        <pre className="code-block">
          <code>{cedarText || "// (compile a rule in Builder mode to see Cedar output)"}</code>
        </pre>
      )}
    </div>
  );
}
