import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useExtension } from "../sdk-context";
import type { ManagedPolicy } from "@scopeball/sdk";
import type { PolicyRule } from "../policy/types";
import { compileRule, parseCedar } from "../policy/builder-wasm";
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

const INITIAL_ACTION = "swap";

const INITIAL_RULE: PolicyRule = {
  id: `dashboard::${INITIAL_ACTION}/newrule(1)`,
  action: INITIAL_ACTION,
  severity: "deny",
  reason: "describe why this should be blocked",
  predicates: [],
};

// Default id at load time is `newrule(count+1)`, where `count` is the
// number of `newrule(\d+)`-shaped entries already in the library for this
// action. So a fresh library opens with `newrule(1)`, the next save sees
// `newrule(2)`, etc. If that slot happens to be taken (gaps from manual
// renames, races), we hand off to the same save-time disambiguator the
// user-typed collision path uses.
function pickDefaultRuleId(
  managed: readonly ManagedPolicy[],
  action: string,
): string {
  const numberedRe = new RegExp(
    `^dashboard::${escapeRegex(action)}/newrule\\(\\d+\\)$`,
  );
  const count = managed.filter((p) => numberedRe.test(p.id)).length;
  const base = `dashboard::${action}/newrule(${count + 1})`;
  return disambiguateId(base, managed);
}

// If `id` is already taken, append `(N)` with N counting up from 0 until
// the result is free. So `newrule(1)` collides → `newrule(1)(0)`; if that
// is also taken → `newrule(1)(1)`. The base string never changes — only
// the trailing `(N)` suffix grows.
function disambiguateId(
  id: string,
  managed: readonly ManagedPolicy[],
): string {
  const used = new Set(managed.map((p) => p.id));
  if (!used.has(id)) return id;
  let n = 0;
  while (used.has(`${id}(${n})`)) n++;
  return `${id}(${n})`;
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

// Extracts the `<action>` segment of `dashboard::<action>/<rest>`, or null
// if the id doesn't follow that shape. Used to compare against `rule.action`
// at save time and warn on mismatch.
function actionSegmentOf(id: string): string | null {
  const m = id.match(/^dashboard::([^/]+)\//);
  return m ? m[1] : null;
}

const PLACEHOLDER_CEDAR = `// Compile a rule from Builder mode to see Cedar text here.
// The Code view is read-only until you opt in via the warning modal.`;

// Pending mode/edit transitions queued while a warning modal is open so the
// user's intent is preserved through the confirm/cancel cycle.
type Pending =
  | { kind: "none" }
  | { kind: "enable-code-edit" }
  | { kind: "switch-mode"; target: EditorMode }
  | { kind: "save-action-mismatch"; idAction: string; ruleAction: string };

export function EditorPage() {
  const { client, refresh, managed } = useExtension();
  const location = useLocation();
  const navigate = useNavigate();
  const [mode, setMode] = useState<EditorMode>("builder");
  const [rule, setRule] = useState<PolicyRule>(INITIAL_RULE);
  const [cedarText, setCedarText] = useState<string>(PLACEHOLDER_CEDAR);
  const [codeEditable, setCodeEditable] = useState(false);
  const [codeDirty, setCodeDirty] = useState(false);
  // Snapshot of the exact rule that produced the current `cedarText`. Set
  // by the Builder compile callback and used by `builderDirty` to gate
  // the save button when the user edits a predicate without re-compiling.
  // Reset to `null` after a successful save / fresh form, so the canonical
  // "no compile yet" state is identifiable.
  const [lastCompiledRule, setLastCompiledRule] =
    useState<PolicyRule | null>(null);
  const [pending, setPending] = useState<Pending>({ kind: "none" });
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState<
    | { kind: "ok"; text: string }
    | { kind: "err"; text: string }
    | null
  >(null);
  const [hydrateNote, setHydrateNote] = useState<string | null>(null);
  // Tracks whether we've already settled the rule id on this editor mount —
  // either by hydrating from Library or by stamping a fresh default. Used so
  // the default-id effect doesn't clobber a user's later edit to the field.
  const idSettledRef = useRef(false);
  // True when the current form was loaded from an existing library policy
  // (Library → Edit). Save in that mode is an update and must be allowed
  // to overwrite the same id; the save-time disambiguator only kicks in
  // for fresh-create flows so a user editing `newrule(1)` doesn't
  // accidentally fork it into `newrule(1)(0)`.
  const hydratedFromExistingRef = useRef(false);

  // Stamp `dashboard::newrule(N)` as the default once `managed` has loaded
  // (and we're not hydrating an existing policy). Library size seeds N; the
  // pick function bumps past any collision (option C).
  useEffect(() => {
    if (idSettledRef.current) return;
    if (managed === null) return;
    const hasHydrationState = Boolean(
      (location.state as EditorLocationState | null)?.text,
    );
    if (hasHydrationState) {
      idSettledRef.current = true;
      return;
    }
    setRule((r) => ({ ...r, id: pickDefaultRuleId(managed, r.action) }));
    idSettledRef.current = true;
  }, [managed, location.state]);

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
        // The hydrated text is exactly what `parsed` would compile to (the
        // engine round-trips through parse_cedar), so seed the snapshot to
        // `parsed`. Without this seed, opening a saved rule and immediately
        // clicking save would be blocked by `builderDirty` even though the
        // rule is unchanged.
        setLastCompiledRule(parsed);
        setMode("builder");
        setHydrateNote(`'${state.id ?? parsed.id}' Builder 모드로 로드됨.`);
      } else {
        setRule((r) => ({ ...r, id: state.id ?? r.id }));
        setLastCompiledRule(null);
        setMode("code");
        setCodeEditable(true);
        setCodeDirty(false);
        setHydrateNote(
          `'${state.id ?? "정책"}'은 Builder로 역변환 불가 (${
            error?.message ?? "unsupported shape"
          }). Code 모드로 표시 중.`,
        );
      }
      // Mark this session as an update of an existing library entry so the
      // save path overwrites rather than auto-disambiguating into a fork.
      hydratedFromExistingRef.current = true;
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

  const handleBuilderCompile = (compiled: string, compiledRule: PolicyRule) => {
    setCedarText(compiled);
    // Pair the emitted Cedar with the exact rule snapshot it was produced
    // from — `builderDirty` below compares this against the live `rule` to
    // know whether the user has edited anything since the last compile.
    setLastCompiledRule(compiledRule);
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
    } else if (pending.kind === "save-action-mismatch") {
      setPending({ kind: "none" });
      void performSave();
      return;
    }
    setPending({ kind: "none" });
  };

  const performSave = async () => {
    setSaving(true);
    setSaveMsg(null);
    try {
      // Belt-and-braces: even though `canSave` requires a fresh compile in
      // Builder mode, recompile here from the *current* rule before sending
      // it to the SDK. This catches edge cases where the rule diverged
      // between render and click (race-y state batching, etc.) and ensures
      // the persisted text always matches the rule shape the user sees.
      // Code mode keeps the user-edited text verbatim.
      let textToSave = cedarText;
      if (mode === "builder") {
        const { cedarText: fresh, error } = await compileRule(rule);
        if (!fresh) {
          throw new Error(
            error?.message ?? "Cedar 컴파일 실패 — 조건을 확인하세요",
          );
        }
        textToSave = fresh;
        setCedarText(fresh);
        setLastCompiledRule(rule);
      }

      // Only auto-disambiguate on fresh-create flows. A Library → Edit
      // session is an update of an existing row, so we must let the same
      // id flow through unchanged or the user's edits silently fork into a
      // new policy.
      const currentManaged = managed ?? [];
      const idToSave = hydratedFromExistingRef.current
        ? rule.id
        : disambiguateId(rule.id, currentManaged);

      const result = await client.putRaw({
        id: idToSave,
        text: textToSave,
      });
      const renamedNote =
        idToSave !== rule.id ? ` (renamed to '${idToSave}')` : "";
      setSaveMsg({
        kind: "ok",
        text: `Saved${renamedNote} · catalog: ${result.catalog.enabled.length} enabled / ${result.catalog.policies.length} total`,
      });
      // Reset the form so the next default id re-stamps once `managed`
      // refreshes — otherwise the field stays at the just-saved id and a
      // second click would target the same row (the original bug).
      idSettledRef.current = false;
      hydratedFromExistingRef.current = false;
      setRule(INITIAL_RULE);
      setCedarText(PLACEHOLDER_CEDAR);
      setLastCompiledRule(null);
      setMode("builder");
      setCodeEditable(false);
      setCodeDirty(false);
      await refresh();
    } catch (e) {
      setSaveMsg({
        kind: "err",
        text: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setSaving(false);
    }
  };

  const handleSave = async () => {
    // Soft guard: if the id carries a `dashboard::<action>/...` prefix that
    // disagrees with the currently-selected action, surface a modal so the
    // user can either fix the id or knowingly proceed (e.g. they renamed
    // the action mid-edit but want to keep the old id).
    const idAction = actionSegmentOf(rule.id);
    if (idAction !== null && idAction !== rule.action) {
      setPending({
        kind: "save-action-mismatch",
        idAction,
        ruleAction: rule.action,
      });
      return;
    }
    await performSave();
  };

  // Builder mode dirty: the user has edited the rule since the last compile
  // (or has never compiled at all in this session). `JSON.stringify` is a
  // sufficient deep-equal for PolicyRule because every leaf is plain JSON
  // (string / array / null) — no functions, no class instances, no map order
  // ambiguity. Comparing fresh-vs-cached strings is microseconds even on
  // rules with dozens of predicates.
  const builderDirty =
    mode === "builder" &&
    (lastCompiledRule === null ||
      JSON.stringify(rule) !== JSON.stringify(lastCompiledRule));

  const canSave =
    rule.id.startsWith("dashboard::") &&
    rule.id.length > "dashboard::".length &&
    cedarText.length > 0 &&
    cedarText !== PLACEHOLDER_CEDAR &&
    !builderDirty;

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
              title={
                builderDirty
                  ? "조건이 변경됐습니다. Cedar로 컴파일을 먼저 누르세요."
                  : undefined
              }
            >
              {saving ? "저장 중..." : "정책 저장 (SDK putRaw)"}
            </button>
            {builderDirty &&
            cedarText.length > 0 &&
            cedarText !== PLACEHOLDER_CEDAR ? (
              <div className="editor-save-msg dirty">
                조건이 마지막 컴파일 이후 변경됐습니다 — Cedar로 컴파일을
                다시 누르세요.
              </div>
            ) : null}
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

      <WarningModal
        open={pending.kind === "save-action-mismatch"}
        title="ID와 Action이 일치하지 않습니다"
        body={
          pending.kind === "save-action-mismatch"
            ? `ID 앞부분은 '${pending.idAction}' 인데 현재 Action은 '${pending.ruleAction}' 입니다. 그대로 저장하면 카탈로그 필터링이 의도와 다르게 동작할 수 있습니다.`
            : ""
        }
        confirmLabel="그대로 저장"
        cancelLabel="ID 수정"
        onConfirm={handleConfirmPending}
        onCancel={() => setPending({ kind: "none" })}
      />
    </div>
  );
}
