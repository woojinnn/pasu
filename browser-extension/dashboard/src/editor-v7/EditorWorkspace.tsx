import { useEffect, useMemo, useReducer, useRef, useState } from "react";

import { testPolicyLocal, validatePolicyLocal, type ValidateResp } from "../cedar";
import { EditorCanvas } from "./canvas/Canvas";
import { evalDoc, type EvalResult, type TxLike } from "./evaluate";
import { Inspector } from "./panels/Inspector";
import { Palette } from "./panels/Palette";
import { editorReducer, initialEditorState } from "./reducer";
import { serializeDoc } from "./serialize";
import type { Doc } from "./types";

import "./panels/panels.css";

/**
 * Three-pane workspace shell:
 *   ┌────────┬────────────────┬──────────┐
 *   │palette │     canvas     │inspector │
 *   └────────┴────────────────┴──────────┘
 *
 * Plus a bottom collapsible drawer with Cedar preview (live) and a
 * tester that re-uses the JS `evalDoc` for an instant verdict and
 * `testPolicyLocal` (wasm) for the authoritative one. `onCedarChange`
 * fires the serialized text upward so the containing page can save it.
 */
export interface EditorWorkspaceProps {
  initialDoc: Doc;
  onDocChange?: (doc: Doc) => void;
  onCedarChange?: (cedarText: string) => void;
}

export function EditorWorkspace({ initialDoc, onDocChange, onCedarChange }: EditorWorkspaceProps) {
  const [state, dispatch] = useReducer(editorReducer, initialDoc, initialEditorState);

  // Stable change notifier — `onDocChange` doesn't need to be in the
  // dep list since the effect runs on every doc transition we care about.
  const cbRef = useRef(onDocChange);
  cbRef.current = onDocChange;
  useEffect(() => {
    cbRef.current?.(state.doc);
  }, [state.doc]);

  // Live Cedar preview — keep the serialized text in component state so
  // the bottom drawer can show it and the parent can persist it. Debounce
  // wasm validation by 300ms so frequent edits don't thrash.
  const cedarText = useMemo(() => serializeDoc(state.doc), [state.doc]);
  const cedarCbRef = useRef(onCedarChange);
  cedarCbRef.current = onCedarChange;
  useEffect(() => {
    cedarCbRef.current?.(cedarText);
  }, [cedarText]);

  const [validateState, setValidateState] = useState<ValidateResp | null>(null);
  const [validating, setValidating] = useState(false);
  useEffect(() => {
    if (!cedarText.trim()) {
      setValidateState({ ok: false, error: "empty" });
      return;
    }
    setValidating(true);
    const t = setTimeout(() => {
      validatePolicyLocal(cedarText)
        .then(setValidateState)
        .catch((e) => setValidateState({ ok: false, error: String(e) }))
        .finally(() => setValidating(false));
    }, 300);
    return () => clearTimeout(t);
  }, [cedarText]);

  // Keyboard shortcuts: ⌘Z undo, ⇧⌘Z redo. Scoped to document so the
  // user can be focused inside an Inspector input.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      if (!meta) return;
      const t = e.target as HTMLElement | null;
      const tag = t?.tagName?.toLowerCase();
      // Don't hijack ⌘Z inside text inputs (user expects native undo there).
      if (tag === "input" || tag === "textarea") return;
      if (e.key === "z" && !e.shiftKey) {
        dispatch({ type: "UNDO" });
        e.preventDefault();
      } else if ((e.key === "z" && e.shiftKey) || e.key === "y") {
        dispatch({ type: "REDO" });
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="v7-workspace-wrap">
      <div className="v7-workspace">
        <Palette dispatch={dispatch} locale={state.doc.locale} />
        <EditorCanvas state={state} dispatch={dispatch} />
        <Inspector state={state} dispatch={dispatch} />
      </div>
      <CedarDrawer
        cedarText={cedarText}
        doc={state.doc}
        validating={validating}
        validateState={validateState}
      />
    </div>
  );
}

// ── bottom drawer: Cedar preview + JS evaluator + wasm test ─────────

function CedarDrawer({
  cedarText,
  doc,
  validating,
  validateState,
}: {
  cedarText: string;
  doc: Doc;
  validating: boolean;
  validateState: ValidateResp | null;
}) {
  const [open, setOpen] = useState(true);
  const [tab, setTab] = useState<"cedar" | "tester">("cedar");

  return (
    <section className={`v7-drawer${open ? " open" : ""}`}>
      <header className="drawer-head">
        <button onClick={() => setOpen((o) => !o)} className="drawer-toggle">
          {open ? "▾" : "▴"} {open ? "닫기" : "열기"}
        </button>
        <div className="drawer-tabs">
          <button className={tab === "cedar" ? "active" : ""} onClick={() => setTab("cedar")}>
            Cedar 미리보기
          </button>
          <button className={tab === "tester" ? "active" : ""} onClick={() => setTab("tester")}>
            트랜잭션 테스트
          </button>
        </div>
        <span className="drawer-status">
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
        </span>
      </header>
      {open && tab === "cedar" && (
        <pre className="cedar-preview">
          <code>{cedarText}</code>
        </pre>
      )}
      {open && tab === "tester" && <TesterPanel cedarText={cedarText} doc={doc} />}
    </section>
  );
}

const DEFAULT_TX = {
  meta: { from: "0xA1c4000000000000000000000000000000007e29" },
  enrichment: { validityDeltaSec: 300, recipientIsContract: false },
  context: {
    recipient: "0xA1c4000000000000000000000000000000007e29",
    slippageBp: 50,
    priceImpactBp: 12,
  },
};

function TesterPanel({ cedarText, doc }: { cedarText: string; doc: Doc }) {
  const [txJson, setTxJson] = useState(() => JSON.stringify(DEFAULT_TX, null, 2));
  const [jsResult, setJsResult] = useState<EvalResult | null>(null);
  const [wasmStatus, setWasmStatus] = useState<{ verdict?: string; error?: string } | null>(null);

  const runJs = () => {
    let tx: TxLike;
    try {
      tx = JSON.parse(txJson) as TxLike;
    } catch (e) {
      setJsResult(null);
      setWasmStatus({ error: `tx JSON parse: ${e instanceof Error ? e.message : String(e)}` });
      return;
    }
    setJsResult(evalDoc(doc, tx));
    setWasmStatus(null);
  };

  const runWasm = async () => {
    let tx: TxLike;
    try {
      tx = JSON.parse(txJson) as TxLike;
    } catch (e) {
      setWasmStatus({ error: `tx JSON parse: ${e instanceof Error ? e.message : String(e)}` });
      return;
    }
    const meta = (tx.meta as Record<string, unknown>) ?? {};
    const ctx = { ...(tx.context as Record<string, unknown>), ...(tx.enrichment as Record<string, unknown>) };
    const from = String(meta.from ?? "0x0");
    const req = {
      principal: `Wallet::"${from}"`,
      action: `Action::"${doc.action}"`,
      resource: 'Protocol::"0x0"',
      entities: [],
      context: ctx,
    };
    try {
      const r = await testPolicyLocal(cedarText, req);
      setWasmStatus({ verdict: r.verdict });
    } catch (e) {
      setWasmStatus({ error: e instanceof Error ? e.message : String(e) });
    }
  };

  return (
    <div className="tester-panel">
      <textarea
        value={txJson}
        onChange={(e) => setTxJson(e.target.value)}
        spellCheck={false}
        rows={8}
      />
      <div className="tester-actions">
        <button onClick={runJs} className="btn-secondary">JS 평가 (즉시)</button>
        <button onClick={runWasm} className="btn-primary">Cedar wasm 평가</button>
      </div>
      {jsResult && (
        <div className={`tester-result ${jsResult.verdict === "ALLOW" ? "ok" : "deny"}`}>
          <strong>JS: {jsResult.verdict}</strong>
          {jsResult.failed.length > 0 && (
            <ul>
              {jsResult.failed.map((f) => (
                <li key={f.id}>
                  {f.guardId} · {f.label}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
      {wasmStatus && (
        <div className={`tester-result ${wasmStatus.error ? "deny" : wasmStatus.verdict === "ALLOW" ? "ok" : "deny"}`}>
          <strong>Wasm: {wasmStatus.error ? "ERROR" : wasmStatus.verdict}</strong>
          {wasmStatus.error && <div className="hint">{wasmStatus.error}</div>}
        </div>
      )}
    </div>
  );
}
