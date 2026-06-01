import { useEffect, useState } from "react";

import { validatePolicyLocal, type ValidateResp } from "../cedar";

/**
 * Plain Cedar textarea — what the user sees when they switch to Code
 * mode. Mirrors the existing MVP `EditorPage` textarea, but isolated
 * so the shell can swap it with `EditorWorkspace` without churning
 * state in the parent page.
 *
 * Live wasm validation runs at 300ms debounce; the result bubbles up
 * via `onCedarChange` so the parent can gate the Save button.
 */
export interface CodeEditorProps {
  initialText: string;
  onCedarChange: (text: string, valid: boolean) => void;
}

export function CodeEditor({ initialText, onCedarChange }: CodeEditorProps) {
  const [text, setText] = useState(initialText);
  const [validating, setValidating] = useState(false);
  const [validateState, setValidateState] = useState<ValidateResp | null>(null);

  useEffect(() => {
    if (!text.trim()) {
      const empty: ValidateResp = { ok: false, error: "cedar_text must not be empty" };
      setValidateState(empty);
      onCedarChange(text, false);
      return;
    }
    setValidating(true);
    const t = setTimeout(() => {
      validatePolicyLocal(text)
        .then((r) => {
          setValidateState(r);
          onCedarChange(text, r.ok);
        })
        .catch((e) => {
          const err: ValidateResp = { ok: false, error: String(e) };
          setValidateState(err);
          onCedarChange(text, false);
        })
        .finally(() => setValidating(false));
    }, 300);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);

  return (
    <div className="v7-code-editor">
      <div className="ce-head">
        {validating ? (
          <span className="status checking">검증 중…</span>
        ) : validateState?.skipped ? (
          <span className="status checking" title="cedar wasm 브리지 미연결 — 저장 시 서버에서 검증합니다">
            · wasm 미연결 · 검증 건너뜀
          </span>
        ) : validateState?.ok ? (
          <span className="status ok">✓ 구문 OK</span>
        ) : (
          <span className="status err" title={validateState?.error ?? ""}>
            ✗ {validateState?.error ?? "—"}
          </span>
        )}
      </div>
      <textarea
        spellCheck={false}
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="permit(principal, action, resource) when { /* … */ };"
      />
    </div>
  );
}
