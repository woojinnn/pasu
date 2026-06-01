import { useEffect, useRef, useState } from "react";

import { CodeEditor } from "./CodeEditor";
import { EditorWorkspace } from "./EditorWorkspace";
import { WarningModal } from "./WarningModal";
import { initialDoc as makeInitialDoc } from "./doc";
import { parseTree, serializeTree } from "./persist";
import { serializeDoc } from "./serialize";
import type { Doc } from "./types";

import "./editor-shell.css";

/**
 * Mode-toggling shell around `EditorWorkspace` (Builder) and `CodeEditor`
 * (Code).
 *
 * Source of truth:
 *   - `cedarText` — what gets saved to `user_policies.cedar_text` and is
 *     what the Cedar runtime compiles.
 *   - `doc` — the v7 builder tree. `null` when the policy was opened
 *     in Code mode without a stored tree.
 *
 * Switching modes:
 *   - Builder → Code: free. The shell keeps `doc` around so the user
 *     can flip back without losing their tree.
 *   - Code → Builder: when the user has modified the textarea since
 *     the last Builder serialization, the new Cedar diverges from
 *     what the tree would produce. We show a WarningModal:
 *       · "트리에서 재생성" — discard hand-edited Cedar; rebuild from doc.
 *       · "코드 유지 + 트리 폐기" — drop the doc; future builder open
 *         will require parsing Cedar back into a tree (Phase 8+, not yet).
 *       · "취소" — stay on Code.
 *
 * On any change (text or tree), `onChange` fires with the latest pair
 * so the parent page can persist both columns.
 */
export type EditorMode = "builder" | "code";

export interface EditorShellProps {
  /** Pre-loaded builder tree (parsed from `policy_tree`). `null` = no tree saved. */
  initialDoc: Doc | null;
  /** Saved `cedar_text`. Always required — Cedar is the runtime source of truth. */
  initialCedarText: string;
  /** Initial action id when there's no tree yet (used by `makeInitialDoc`). */
  fallbackAction?: string;
  /** Initial mode. Defaults to Builder if a tree exists, Code otherwise. */
  initialMode?: EditorMode;
  /** Fires on every change. `treeJson` is `null` when the user committed
   *  to Code-only (will be persisted as NULL in `policy_tree`). */
  onChange?: (next: { cedarText: string; treeJson: string | null; mode: EditorMode }) => void;
}

export function EditorShell({
  initialDoc,
  initialCedarText,
  fallbackAction = "Amm::Swap",
  initialMode,
  onChange,
}: EditorShellProps) {
  const [mode, setMode] = useState<EditorMode>(initialMode ?? (initialDoc ? "builder" : "code"));
  const [doc, setDoc] = useState<Doc | null>(initialDoc);
  const [cedarText, setCedarText] = useState<string>(initialCedarText);
  const [codeIsValid, setCodeIsValid] = useState<boolean>(true);

  // Snapshot the cedar text that *the builder produced* the last time
  // it was active. Comparing against this tells us whether the user
  // hand-edited the Cedar in Code mode.
  const lastBuilderCedarRef = useRef<string>(initialCedarText);

  const [warn, setWarn] = useState<null | "code-dirty" | "drop-tree">(null);

  // Notify parent on any change.
  const cbRef = useRef(onChange);
  cbRef.current = onChange;
  useEffect(() => {
    cbRef.current?.({
      cedarText,
      treeJson: doc ? serializeTree(doc) : null,
      mode,
    });
  }, [cedarText, doc, mode]);

  // ── handlers wired into child editors ────────────────────────────

  const onBuilderDoc = (nextDoc: Doc) => setDoc(nextDoc);

  const onBuilderCedar = (text: string) => {
    setCedarText(text);
    lastBuilderCedarRef.current = text;
  };

  const onCodeChange = (text: string, ok: boolean) => {
    setCedarText(text);
    setCodeIsValid(ok);
  };

  // ── mode switches ────────────────────────────────────────────────

  const goCode = () => {
    // Builder → Code: always allowed. The current cedar serialization
    // (already in state via onBuilderCedar) becomes the editable seed.
    setMode("code");
  };

  const goBuilder = () => {
    if (!doc) {
      // No saved tree → we can't reconstruct one from arbitrary Cedar
      // yet (Phase 8+). Build a blank doc seeded with the user's action.
      setDoc(makeInitialDoc({ action: fallbackAction }));
      setMode("builder");
      return;
    }
    const builderCedar = serializeDoc(doc);
    if (cedarText !== builderCedar) {
      // The user hand-edited the Cedar in Code mode. We can't safely
      // reconcile that with the tree — ask.
      setWarn("code-dirty");
      return;
    }
    setMode("builder");
  };

  const confirmRebuildFromTree = () => {
    if (!doc) return;
    const rebuilt = serializeDoc(doc);
    setCedarText(rebuilt);
    lastBuilderCedarRef.current = rebuilt;
    setMode("builder");
    setWarn(null);
  };

  const confirmDropTree = () => {
    setDoc(null);
    setWarn(null);
    // Stay in Code mode — the tree is now gone; next save will persist
    // `policy_tree = null` on the backend.
  };

  return (
    <div className="v7-shell">
      <header className="shell-toolbar">
        <div className="mode-pill">
          <button
            className={mode === "builder" ? "active" : ""}
            onClick={() => mode !== "builder" && goBuilder()}
          >
            🧱 Builder
          </button>
          <button
            className={mode === "code" ? "active" : ""}
            onClick={() => mode !== "code" && goCode()}
          >
            📝 Code
          </button>
        </div>
        {mode === "code" && !doc && (
          <span className="hint">트리 없음 — Code 전용 정책</span>
        )}
        {mode === "code" && doc && (
          <button className="link-btn" onClick={() => setWarn("drop-tree")}>
            트리 폐기
          </button>
        )}
        <span className="grow" />
        {mode === "code" && (
          <span className="status">
            {codeIsValid ? "" : <em className="warn">Cedar 문법 오류 — 저장 불가</em>}
          </span>
        )}
      </header>

      {mode === "builder" && doc && (
        <EditorWorkspace
          initialDoc={doc}
          onDocChange={onBuilderDoc}
          onCedarChange={onBuilderCedar}
        />
      )}

      {mode === "code" && (
        <CodeEditor initialText={cedarText} onCedarChange={onCodeChange} />
      )}

      <WarningModal
        open={warn === "code-dirty"}
        title="Cedar 코드가 트리와 다릅니다"
        body={
          <>
            <p>
              Code 모드에서 직접 편집한 Cedar 텍스트가 Builder 트리의 결과와 다릅니다.
              Builder로 돌아가면 직접 편집한 Cedar가 폐기되고 트리에서 다시 생성됩니다.
            </p>
            <p className="hint">
              직접 편집을 유지하고 싶다면 취소 후 툴바의 "트리 폐기" 버튼을 눌러 Code 전용으로 전환하세요.
            </p>
          </>
        }
        confirmLabel="트리에서 재생성"
        confirmTone="primary"
        cancelLabel="취소"
        onConfirm={confirmRebuildFromTree}
        onCancel={() => setWarn(null)}
      />

      <WarningModal
        open={warn === "drop-tree"}
        title="Builder 트리를 폐기할까요?"
        body={
          <p>
            Builder 트리를 제거하면 이 정책은 Code 전용이 됩니다. 다음 저장 시
            <code> policy_tree </code>가 비워지고, 추후 Builder로 돌아가려면 빈 캔버스로 다시 시작해야 합니다.
          </p>
        }
        confirmLabel="트리 폐기"
        confirmTone="danger"
        onConfirm={confirmDropTree}
        onCancel={() => setWarn(null)}
      />
    </div>
  );
}

export { parseTree, serializeTree };
