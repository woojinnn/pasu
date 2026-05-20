import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useExtension } from "../sdk-context";
import type { PolicyRule } from "../policy/types";
import { parseCedar } from "../policy/builder-wasm";
import { ModeToggle, type EditorMode } from "../editor/ModeToggle";
import { BuilderView } from "../editor/BuilderView";
import { CodeView } from "../editor/CodeView";
import { PolicyTestPanel } from "../editor/PolicyTestPanel";
import { WarningModal } from "../editor/WarningModal";
import "./EditorPage.css";

interface EditorLocationState {
  id?: string;
  text?: string;
}

const INITIAL_RULE: PolicyRule = {
  id: "dashboard::my/new-rule",
  action: "swap",
  severity: "deny",
  reason: "describe why this should be blocked",
  predicates: [],
};

const PLACEHOLDER_CEDAR = `// Compile a rule from Builder mode to see Cedar text here.
// The Code view is read-only until you opt in via the warning modal.`;

// Pending mode/edit transitions queued while a warning modal is open so the
// user's intent is preserved through the confirm/cancel cycle.
type Pending =
  | { kind: "none" }
  | { kind: "enable-code-edit" }
  | { kind: "switch-mode"; target: EditorMode };

export function EditorPage() {
  const { client, refresh, managed } = useExtension();
  const location = useLocation();
  const navigate = useNavigate();
  const [mode, setMode] = useState<EditorMode>("builder");
  const [rule, setRule] = useState<PolicyRule>(INITIAL_RULE);
  const [cedarText, setCedarText] = useState<string>(PLACEHOLDER_CEDAR);
  const [codeEditable, setCodeEditable] = useState(false);
  const [codeDirty, setCodeDirty] = useState(false);
  const [pending, setPending] = useState<Pending>({ kind: "none" });
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState<
    | { kind: "ok"; text: string }
    | { kind: "err"; text: string }
    | null
  >(null);
  const [hydrateNote, setHydrateNote] = useState<string | null>(null);

  // Hydrate from Library → Edit handoff. parse_cedar succeeds when the
  // policy fits the PolicyRule subset (forbid + AND-of-leaf-predicates) —
  // otherwise we fall back to Code mode and surface a notice so the user
  // knows Builder couldn't represent the rule.
  useEffect(() => {
    const state = (location.state ?? null) as EditorLocationState | null;
    if (!state?.text) return;
    let cancelled = false;
    void (async () => {
      const { rule: parsed, error } = await parseCedar(state.text!);
      if (cancelled) return;
      setCedarText(state.text!);
      if (parsed) {
        setRule(parsed);
        setMode("builder");
        setHydrateNote(`'${state.id ?? parsed.id}' Builder 모드로 로드됨.`);
      } else {
        setRule((r) => ({ ...r, id: state.id ?? r.id }));
        setMode("code");
        setCodeEditable(true);
        setCodeDirty(false);
        setHydrateNote(
          `'${state.id ?? "정책"}'은 Builder로 역변환 불가 (${
            error?.message ?? "unsupported shape"
          }). Code 모드로 표시 중.`,
        );
      }
      // Clear router state so a reload doesn't re-hydrate stale data.
      navigate(location.pathname, { replace: true, state: null });
    })();
    return () => {
      cancelled = true;
    };
  }, [location, navigate]);

  const requestModeSwitch = (target: EditorMode) => {
    if (target === mode) return;
    if (codeEditable && codeDirty && target !== "code") {
      setPending({ kind: "switch-mode", target });
      return;
    }
    applyModeSwitch(target);
  };

  const applyModeSwitch = (target: EditorMode) => {
    setMode(target);
    if (target !== "code") {
      setCodeEditable(false);
      setCodeDirty(false);
    }
  };

  const handleBuilderCompile = (compiled: string) => {
    setCedarText(compiled);
    setCodeEditable(false);
    setCodeDirty(false);
    setSaveMsg(null);
  };

  const handleEditCedar = (text: string) => {
    setCedarText(text);
    setCodeDirty(true);
  };

  const handleConfirmPending = () => {
    if (pending.kind === "enable-code-edit") {
      setCodeEditable(true);
    } else if (pending.kind === "switch-mode") {
      applyModeSwitch(pending.target);
    }
    setPending({ kind: "none" });
  };

  const handleSave = async () => {
    setSaving(true);
    setSaveMsg(null);
    try {
      const result = await client.putRaw({
        id: rule.id,
        text: cedarText,
      });
      setSaveMsg({
        kind: "ok",
        text: `Saved · catalog: ${result.catalog.enabled.length} enabled / ${result.catalog.policies.length} total`,
      });
      void refresh();
    } catch (e) {
      setSaveMsg({
        kind: "err",
        text: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setSaving(false);
    }
  };

  const canSave =
    rule.id.startsWith("dashboard::") &&
    rule.id.length > "dashboard::".length &&
    cedarText.length > 0 &&
    cedarText !== PLACEHOLDER_CEDAR;

  return (
    <div className="editor-page">
      <header className="editor-header">
        <h1>Editor</h1>
        <ModeToggle mode={mode} onChange={requestModeSwitch} />
      </header>
      {hydrateNote ? (
        <div className="editor-hydrate-note" role="status">
          <span>{hydrateNote}</span>
          <button
            type="button"
            className="editor-hydrate-dismiss"
            onClick={() => setHydrateNote(null)}
            aria-label="알림 닫기"
          >
            ×
          </button>
        </div>
      ) : null}
      <div className="editor-grid">
        <section className="editor-canvas card">
          {mode === "builder" ? (
            <BuilderView
              rule={rule}
              onRuleChange={setRule}
              onCedarChange={handleBuilderCompile}
            />
          ) : (
            <CodeView
              cedarText={cedarText}
              editable={codeEditable}
              onChangeCedarText={handleEditCedar}
              onRequestEdit={() => setPending({ kind: "enable-code-edit" })}
            />
          )}
          <footer className="editor-canvas-footer">
            <button
              type="button"
              className="editor-save"
              disabled={!canSave || saving}
              onClick={() => void handleSave()}
            >
              {saving ? "저장 중..." : "정책 저장 (SDK putRaw)"}
            </button>
            {saveMsg ? (
              <div className={`editor-save-msg ${saveMsg.kind}`}>
                {saveMsg.text}
              </div>
            ) : null}
          </footer>
        </section>
        <aside className="editor-side">
          <PolicyTestPanel
            policyId={rule.id}
            cedarText={cedarText}
            onOpenMatched={(matchedId) => {
              const target = managed?.find((m) => m.id === matchedId);
              if (target) {
                navigate("/editor", {
                  state: { id: target.id, text: target.text },
                });
              } else {
                setHydrateNote(
                  `'${matchedId}' 정책을 카탈로그에서 찾지 못했습니다 (대시보드 외부 정책일 수 있음).`,
                );
              }
            }}
          />
        </aside>
      </div>

      <WarningModal
        open={pending.kind === "enable-code-edit"}
        title="Cedar 직접 편집을 시작하시겠어요?"
        body="직접 편집한 정책은 더 이상 Builder에서 안전하게 다시 열리지 않습니다. Builder로 돌아가면 마지막으로 컴파일된 버전으로 복원됩니다 (현재 편집 내용 폐기)."
        confirmLabel="편집 시작"
        cancelLabel="취소"
        onConfirm={handleConfirmPending}
        onCancel={() => setPending({ kind: "none" })}
      />

      <WarningModal
        open={pending.kind === "switch-mode"}
        title="Code 편집 내용을 버리고 이동할까요?"
        body="현재 Code 모드에서 직접 편집한 내용은 폐기되고 가장 최근 Builder 컴파일 결과로 돌아갑니다."
        confirmLabel="이동 (편집 폐기)"
        cancelLabel="Code 모드 유지"
        onConfirm={handleConfirmPending}
        onCancel={() => setPending({ kind: "none" })}
      />
    </div>
  );
}
