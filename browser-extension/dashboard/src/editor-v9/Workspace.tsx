/**
 * editor-v9 — Blockly Workspace mount, IR-backed.
 *
 * Pipeline:
 *   Workspace ──workspaceToIR──▶ PolicyIR[] ──blocksToText──▶ Cedar text
 *                                       └─ validateIR()       (gates onChange)
 *
 *   Cedar text ──textToBlocks──▶ PolicyIR[] ──irToWorkspace──▶ Workspace   (Phase D)
 *
 * Seeding precedence on mount:
 *   1. `initialJson`        — verbatim Blockly workspace (saved tree, v:9 wrap);
 *   2. `initialCedarText`   — parse via wasm/SW → IR → blocks;
 *   3. neither              — drop an empty `policy_hat` so the user has a
 *                             starting handle.
 *
 * The same path powers the "Cedar 코드 가져오기" textarea below the canvas:
 * paste a policy, hit 불러오기, and irToWorkspace replaces the current canvas.
 *
 * StrictMode-safe via `wsRef.current` guard. Recomputes (workspace → cedar
 * text) are debounced 250 ms so a rapid drag burst makes one round-trip.
 */

import * as Blockly from "blockly";
import * as En from "blockly/msg/en";
import { useEffect, useMemo, useRef, useState } from "react";

import { registerBlocks } from "./blocks/register";
import { blocksToText, textToBlocks } from "./bridge";
import { irToWorkspace } from "./mapping/irToWorkspace";
import { workspaceToIR } from "./mapping/workspaceToIR";
import { validateIR, type EditorError } from "./errors";
import { buildToolbox } from "./toolbox/build";
import { BLOCK_TYPES } from "./mapping/block-types";
import { registerMakeParamContextMenu } from "./Param/make-param";
import { ParamSidebar } from "./Param/ParamSidebar";
import { ParamFillPanel } from "./Param/ParamFillPanel";
import type { PolicyIR } from "../cedar/blocks";
import type { Expr } from "../cedar/blocks/ir";
import { buildProbes, diagnoseFromResult } from "../cedar/diagnosis";
import { pathToBlockId, enumeratePaths } from "../cedar/diagnosis/path";
import { runDiagnosisProbes } from "../server-api/diagnosis";
import { applyCulprits, clearCulprits } from "./diagnosis-highlight";
import { SAMPLE_ACTIONS } from "./sample-actions";
import { chainToDottedPath } from "./mapping/attr-path";
import { getGloss } from "./gloss";
import "./diagnosis-highlight.css";

Blockly.setLocale(En as unknown as Record<string, string>);

export interface WorkspaceV9Props {
  /** Serialised Blockly workspace JSON (preferred seed). */
  initialJson?: object | null;
  /** Cedar text fallback seed; consulted only if `initialJson` is null. */
  initialCedarText?: string | null;
  policyName?: string;
  locale?: "ko" | "en";
  /** Hide the bottom "Cedar 코드 가져오기" (paste-to-import) details
   *  panel. The v2 editor exposes a dedicated Cedar tab for editing
   *  raw text, so the inline import box is redundant + visually
   *  confusing (its placeholder shows an unrelated example policy). */
  hideImport?: boolean;
  /** Hide the bottom "Cedar 미리보기" details panel. The legacy panel
   *  shows the same cedar that the v2 Cedar tab edits, so consumers
   *  that already render a separate Cedar surface can dedupe it. */
  hidePreview?: boolean;
  onChange?: (next: {
    cedarText: string;
    json: object;
    errors: EditorError[];
    /** Validated policy IR for the current workspace (present only when the
     *  policy compiled). Lets the save path auto-generate the enrichment
     *  manifest from the `context.custom.*` fields the policy reads. */
    ir?: PolicyIR;
  }) => void;
}

export function WorkspaceV9({
  initialJson,
  initialCedarText,
  policyName = "untitled",
  locale = "ko",
  hideImport = false,
  hidePreview = false,
  onChange,
}: WorkspaceV9Props) {
  const mountRef = useRef<HTMLDivElement | null>(null);
  const wsRef = useRef<Blockly.WorkspaceSvg | null>(null);
  const [cedarText, setCedarText] = useState("");
  const [errors, setErrors] = useState<EditorError[]>([]);
  const [bridgeError, setBridgeError] = useState<string | null>(null);
  const [importText, setImportText] = useState("");
  const [importError, setImportError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [currentPolicy, setCurrentPolicy] = useState<PolicyIR | null>(null);
  const [filledText, setFilledText] = useState<string | null>(null);
  const [filledError, setFilledError] = useState<string | null>(null);
  const [simulating, setSimulating] = useState(false);
  const [simulateMsg, setSimulateMsg] = useState<string | null>(null);
  // Expr→blockId identity map for the LAST irToWorkspace render the Simulate
  // handler does. Only valid for the IR objects passed to that render, so the
  // handler rebuilds it (and re-renders the canvas) on every click.
  const blockIdByNodeRef = useRef<Map<Expr, string>>(new Map());

  const toolbox = useMemo(() => buildToolbox(locale), [locale]);

  useEffect(() => {
    if (!mountRef.current) return;
    if (wsRef.current) return;

    try {
      registerBlocks();
      registerMakeParamContextMenu();
    } catch (e) {
      setBridgeError(`registerBlocks failed: ${String(e)}`);
      return;
    }

    let ws: Blockly.WorkspaceSvg;
    try {
      ws = Blockly.inject(mountRef.current, {
        toolbox: toolbox as unknown as Blockly.utils.toolbox.ToolboxDefinition,
        trashcan: true,
        scrollbars: true,
        zoom: { controls: true, wheel: true, startScale: 0.9, minScale: 0.4, maxScale: 2 },
        grid: { spacing: 20, length: 3, colour: "#E5E6E3", snap: true },
      });
    } catch (e) {
      setBridgeError(`Blockly.inject failed: ${String(e)}`);
      return;
    }
    wsRef.current = ws;

    let cancelled = false;
    const seed = async () => {
      try {
        if (initialJson) {
          Blockly.serialization.workspaces.load(initialJson, ws);
        } else if (initialCedarText && initialCedarText.trim()) {
          const policies = await textToBlocks(initialCedarText);
          if (cancelled) return;
          irToWorkspace(ws, policies);
          // Fallback: empty result → still drop a hat so the user can edit.
          if (policies.length === 0) emptyHat(ws);
        } else {
          emptyHat(ws);
        }
      } catch (e) {
        console.warn("[v9] workspace seed failed", e);
        emptyHat(ws);
      }
    };

    const recompute = async () => {
      const errs: EditorError[] = [];
      const policies = workspaceToIR(ws, errs);
      const head = policies[0] ?? null;
      const validated = validateIR(head, errs);
      const wsJson = Blockly.serialization.workspaces.save(ws);

      setCurrentPolicy(head);

      if (!validated.ok || !validated.ir) {
        setCedarText("");
        setErrors(validated.errors);
        setBridgeError(null);
        onChange?.({ cedarText: "", json: wsJson, errors: validated.errors });
        return;
      }

      try {
        const text = await blocksToText(validated.ir);
        setCedarText(text);
        setErrors([]);
        setBridgeError(null);
        onChange?.({ cedarText: text, json: wsJson, errors: [], ir: validated.ir });
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setBridgeError(msg);
        setCedarText("");
        onChange?.({ cedarText: "", json: wsJson, errors: [{ kind: "cedar", message: msg }] });
      }
    };

    let debounce: ReturnType<typeof setTimeout> | null = null;
    const listener = (event: Blockly.Events.Abstract) => {
      if (event.isUiEvent) return;
      if (debounce) clearTimeout(debounce);
      debounce = setTimeout(() => void recompute(), 250);
    };
    ws.addChangeListener(listener);

    void seed().then(() => void recompute());

    requestAnimationFrame(() => Blockly.svgResize(ws));

    return () => {
      cancelled = true;
      if (debounce) clearTimeout(debounce);
      ws.removeChangeListener(listener);
      ws.dispose();
      wsRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const onResize = () => {
      if (wsRef.current) Blockly.svgResize(wsRef.current);
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  void policyName;

  const onImportClick = async () => {
    if (!wsRef.current) return;
    if (!importText.trim()) {
      setImportError("Cedar 코드를 입력하세요");
      return;
    }
    setImporting(true);
    setImportError(null);
    try {
      const policies = await textToBlocks(importText);
      irToWorkspace(wsRef.current, policies);
      if (policies.length === 0) emptyHat(wsRef.current);
      setImportText("");
    } catch (e) {
      setImportError(e instanceof Error ? e.message : String(e));
    } finally {
      setImporting(false);
    }
  };

  const errorCount = errors.length + (bridgeError ? 1 : 0);

  const onJumpToHole = (name: string) => {
    const ws = wsRef.current;
    if (!ws) return;
    for (const b of ws.getAllBlocks(false)) {
      if (b.type === BLOCK_TYPES.expr_hole && b.getFieldValue("NAME") === name) {
        ws.centerOnBlock(b.id, true);
        b.select();
        return;
      }
    }
  };

  const onFilledIR = async (filled: PolicyIR) => {
    setFilledError(null);
    try {
      const text = await blocksToText(filled);
      setFilledText(text);
    } catch (e) {
      setFilledError(e instanceof Error ? e.message : String(e));
      setFilledText(null);
    }
  };

  const onApplyFilledToCanvas = () => {
    if (!wsRef.current || !filledText) return;
    void (async () => {
      try {
        const policies = await textToBlocks(filledText);
        irToWorkspace(wsRef.current!, policies);
        setFilledText(null);
        setFilledError(null);
      } catch (e) {
        setFilledError(e instanceof Error ? e.message : String(e));
      }
    })();
  };

  // ────────────────────────────────────────────────────────────────────────
  // REFERENCE INTEGRATION for denial diagnosis — copy this pattern for any new
  // surface. Full guide: `src/cedar/diagnosis/README.md`.
  //
  // On-demand: evaluate the draft `forbid` policy against a sample action and
  // red-box the sub-clause(s) that caused the (simulated) denial. Steps:
  //   1. workspaceToIR() ONCE → `policy` (with a validity guard).
  //   2. guard: only `forbid` policies are diagnosable (a fired forbid = denial).
  //   3. pick the sample for the policy's action id (bail if none).
  //   4. buildProbes(policy) → bail to @reason if `!diagnosable` (hole/raw).
  //   5. re-render via irToWorkspace(ws, policies, map) so the Expr→blockId
  //      identity map is keyed by the SAME `policy` objects we diagnose — this is
  //      the load-bearing seam (README §4): map + buildProbes + diagnoseFromResult
  //      + pathToBlockId must all share one PolicyIR object or nothing highlights.
  //   6. runDiagnosisProbes({ ...sample(), probes }) → Cedar oracle (WASM, via SW).
  //      (`sample` is a factory function — call it.)
  //   7. diagnoseFromResult(policy, probeIds, result) → culprit leaf paths.
  //   8. pathToBlockId(policy, map) → applyCulprits(ws, pathMap, culprits, note).
  // ────────────────────────────────────────────────────────────────────────
  const onSimulate = async () => {
    const ws = wsRef.current;
    if (!ws || simulating) return;
    setSimulating(true);
    setSimulateMsg(null);
    clearCulprits(ws); // drop any prior red boxes before re-evaluating
    try {
      // Re-render from the current workspace so the Expr→blockId identity map is
      // built from the SAME IR objects we diagnose — they're created here, with
      // no intervening edit, so identity matches by construction. (Tradeoff: the
      // canvas reflows to default block layout on each Simulate.)
      const errs: EditorError[] = [];
      const policies = workspaceToIR(ws, errs);
      const policy = policies[0] ?? null;
      if (!policy || errs.length > 0) {
        setSimulateMsg("유효한 정책이 없습니다");
        return;
      }

      if (policy.effect !== "forbid") {
        clearCulprits(ws);
        setSimulateMsg("forbid 정책만 진단할 수 있습니다");
        return;
      }

      // Action uid id (Pascal) — only an `== Action::"Id"` scope names one.
      const actionScope = policy.scope.action;
      const actionId =
        actionScope.kind === "scopeEq" ? actionScope.entity.id : null;
      const sample = actionId ? SAMPLE_ACTIONS[actionId] : undefined;
      if (!sample) {
        setSimulateMsg("이 액션의 샘플이 없습니다");
        return;
      }

      const { probes, diagnosable } = buildProbes(policy);
      if (!diagnosable) {
        const reason = policy.annotations.find((a) => a.name === "reason")?.value;
        setSimulateMsg(reason ? `진단 불가 — ${reason}` : "이 정책은 진단할 수 없습니다");
        clearCulprits(ws);
        return;
      }

      blockIdByNodeRef.current = new Map<Expr, string>();
      irToWorkspace(ws, policies, blockIdByNodeRef.current);

      const result = await runDiagnosisProbes({ ...sample(), probes });
      const d = diagnoseFromResult(policy, probes.map((p) => p.id), result);

      const pathMap = pathToBlockId(policy, blockIdByNodeRef.current);
      const byPath = new Map(enumeratePaths(policy).map((e) => [e.path, e.node]));
      const note = (p: string): string | null => {
        const node = byPath.get(p);
        if (!node || node.kind !== "binary") return null;
        const leftPath = chainToDottedPath(node.left);
        const lhs = (leftPath !== null ? getGloss(leftPath)?.ko : undefined) ?? leftPath ?? "?";
        const rhs =
          node.right.kind === "lit" ? String(node.right.value) : "?";
        return `${lhs} ${node.op} ${rhs}`;
      };
      applyCulprits(ws, pathMap, d.culprits, note);

      if (d.culprits.length === 0) {
        setSimulateMsg("이 샘플에서는 거부되지 않습니다 (위반 조건 없음)");
      } else {
        setSimulateMsg(`위반 조건 ${d.culprits.length}개를 빨간 박스로 표시했습니다`);
      }
    } catch (e) {
      setSimulateMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setSimulating(false);
    }
  };

  return (
    <div style={{
      display: "flex",
      flexDirection: "column",
      height: "calc(100vh - 200px)",
      minHeight: 600,
      width: "100%",
    }}>
      <div style={{
        padding: "6px 12px",
        background: "var(--surface, #fff)",
        borderBottom: "1px solid var(--hairline-soft, #E5E6E3)",
        fontFamily: "var(--ff-mono, monospace)",
        fontSize: 11,
        color: "var(--slate-500, #475569)",
        display: "flex",
        gap: 12,
        alignItems: "center",
      }}>
        <span>Blockly v9 · 좌측 카테고리에서 블록을 끌어다 정책 트리에 붙이세요</span>
        {errorCount > 0 && (
          <span style={{ color: "var(--fail-700, #7F4740)" }}>
            ⚠ {errorCount}개 문제
          </span>
        )}
        <button
          onClick={() => void onSimulate()}
          disabled={simulating}
          style={{ marginLeft: "auto", padding: "3px 12px", fontSize: 11 }}
          title="샘플 액션으로 정책을 평가해 거부 원인 블록을 빨간 박스로 표시합니다"
        >
          {simulating ? "시뮬레이션 중…" : "Simulate"}
        </button>
        {simulateMsg && (
          <span style={{ color: "var(--slate-500, #475569)" }}>{simulateMsg}</span>
        )}
      </div>

      <div style={{ display: "flex", flex: 1, minHeight: 500 }}>
        <div
          ref={mountRef}
          style={{
            flex: 1,
            minHeight: 500,
            height: 500,
            position: "relative",
            background: "#fafbfa",
          }}
        />
        <ParamSidebar policy={currentPolicy} onJump={onJumpToHole} />
      </div>

      {!hidePreview && (
        <details style={{ background: "var(--fog-200, #fafaf9)", borderTop: "1px solid var(--hairline-soft, #E5E6E3)" }}>
          <summary style={{ padding: "6px 12px", cursor: "pointer", fontSize: 12, color: "var(--slate-500, #475569)" }}>
            Cedar 미리보기 ({cedarText.split("\n").length} 줄) {errorCount > 0 && `· ${errorCount}개 문제`}
          </summary>
          {errors.length > 0 && (
            <ul style={{ margin: 0, padding: "6px 24px", fontSize: 12, color: "var(--fail-700, #7F4740)" }}>
              {errors.map((e, i) => (
                <li key={i}>{e.message}</li>
              ))}
            </ul>
          )}
          {bridgeError && (
            <div style={{ padding: "6px 12px", fontSize: 12, color: "var(--fail-700, #7F4740)" }}>
              Cedar 변환 실패: {bridgeError}
            </div>
          )}
          <pre style={{
            margin: 0, padding: 12, fontSize: 12,
            fontFamily: "var(--ff-mono, monospace)",
            maxHeight: 200, overflow: "auto",
            background: "var(--fog-100, #fcfcfc)",
          }}>
            {cedarText || (errorCount > 0 ? "" : "(빈 정책)")}
          </pre>
        </details>
      )}
      {hidePreview && (errors.length > 0 || bridgeError) && (
        <div style={{
          margin: 0,
          padding: "8px 12px",
          background: "var(--fail-50, #FAEAE6)",
          borderTop: "1px solid var(--hairline-soft, #E5E6E3)",
          fontSize: 12,
          color: "var(--fail-700, #7F4740)",
        }}>
          {bridgeError && <div>Cedar 변환 실패: {bridgeError}</div>}
          {errors.map((e, i) => (
            <div key={i}>⚠ {e.message}</div>
          ))}
        </div>
      )}

      <ParamFillPanel template={currentPolicy} onFilled={(p) => void onFilledIR(p)} />

      {filledText !== null && (
        <details
          open
          style={{
            background: "var(--ok-50, #f0f6f1)",
            borderTop: "1px solid var(--hairline-soft, #E5E6E3)",
          }}
        >
          <summary style={{ padding: "6px 12px", cursor: "pointer", fontSize: 12 }}>
            적용된 정책 (파라미터 채움 완료)
          </summary>
          <pre style={{
            margin: 0, padding: 12, fontSize: 12,
            fontFamily: "var(--ff-mono, monospace)",
            maxHeight: 200, overflow: "auto",
          }}>
            {filledText}
          </pre>
          <div style={{ padding: "6px 12px", display: "flex", gap: 8, alignItems: "center" }}>
            <button onClick={onApplyFilledToCanvas} style={{ padding: "4px 12px", fontSize: 12 }}>
              결과로 캔버스 교체
            </button>
            <button
              onClick={() => navigator.clipboard?.writeText(filledText).catch(() => {})}
              style={{ padding: "4px 12px", fontSize: 12 }}
            >
              복사
            </button>
            <button
              onClick={() => { setFilledText(null); setFilledError(null); }}
              style={{ padding: "4px 12px", fontSize: 12 }}
            >
              닫기
            </button>
          </div>
          {filledError && (
            <div style={{ padding: "6px 12px", fontSize: 12, color: "var(--fail-700, #7F4740)" }}>
              ⚠ {filledError}
            </div>
          )}
        </details>
      )}

      {!hideImport && (
        <details style={{ background: "var(--fog-100, #fcfcfc)", borderTop: "1px solid var(--hairline-soft, #E5E6E3)" }}>
          <summary style={{ padding: "6px 12px", cursor: "pointer", fontSize: 12, color: "var(--slate-500, #475569)" }}>
            Cedar 코드 가져오기 — 텍스트를 붙여 넣고 블록으로 변환
          </summary>
          <div style={{ padding: "8px 12px", display: "flex", flexDirection: "column", gap: 6 }}>
            <textarea
              value={importText}
              onChange={(e) => setImportText(e.target.value)}
              placeholder={'permit (\n  principal,\n  action == Action::"Swap",\n  resource\n) when { context.amount > 100 };'}
              style={{
                fontFamily: "var(--ff-mono, monospace)",
                fontSize: 12,
                minHeight: 100,
                padding: 8,
                border: "1px solid var(--hairline-soft, #E5E6E3)",
                borderRadius: 4,
                resize: "vertical",
              }}
              disabled={importing}
            />
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <button
                onClick={() => void onImportClick()}
                disabled={importing || !importText.trim()}
                style={{ padding: "4px 12px", fontSize: 12 }}
              >
                {importing ? "변환 중…" : "불러오기"}
              </button>
              {importError && (
                <span style={{ color: "var(--fail-700, #7F4740)", fontSize: 12 }}>
                  ⚠ {importError}
                </span>
              )}
              <span style={{ marginLeft: "auto", fontSize: 11, color: "var(--slate-500, #475569)" }}>
                주의: 현재 캔버스가 덮어쓰입니다
              </span>
            </div>
          </div>
        </details>
      )}
    </div>
  );
}

function emptyHat(ws: Blockly.WorkspaceSvg): void {
  ws.clear();
  const hat = ws.newBlock(BLOCK_TYPES.policy_hat);
  hat.initSvg();
  hat.render();
  hat.moveBy(50, 30);
}
