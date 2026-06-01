import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";

import {
  createPolicy,
  deletePolicy,
  getPolicy,
  getPolicyTemplates,
  getExampleTransactions,
  listPolicies,
  patchPolicy,
  ServerError,
  type ExampleTransaction,
  type PolicySeverity,
  type PolicyTemplate,
} from "../server-api";

import {
  testPolicyLocal,
  validatePolicyLocal,
  type CedarRequestInput,
  type TestPolicyResp,
  type ValidateResp,
} from "../cedar";
import { Topbar } from "../shell/Topbar";
import "./editor.css";

/**
 * Policy editor (MVP / text-based).
 *
 * Layout: left sidebar with the installed policy list; main column with
 * a Cedar textarea + live validation (debounced 400ms) + save / delete.
 * Bottom test panel posts a sample Cedar request and prints the verdict.
 *
 * The block-based scratch editor from front/scopeball-v3 is intentionally
 * not built here — that's a 6-10h palette/canvas job. This MVP unlocks
 * the same backend (validate + test + save) with a fraction of the UI.
 */
export function EditorPage() {
  const qc = useQueryClient();
  const [params, setParams] = useSearchParams();
  const [selectedId, setSelectedId] = useState<number | "new" | null>(null);

  // Honor ?new=1 (from NavRail CTA) and ?policy=<id> (from Home triage Editor link).
  useEffect(() => {
    if (params.get("new") === "1") {
      setSelectedId("new");
      const next = new URLSearchParams(params);
      next.delete("new");
      setParams(next, { replace: true });
      return;
    }
    const pid = params.get("policy");
    if (pid) {
      const n = Number(pid);
      if (!Number.isNaN(n)) {
        setSelectedId(n);
        const next = new URLSearchParams(params);
        next.delete("policy");
        setParams(next, { replace: true });
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Editor draft state — name / cedar / severity. When the selection
  // changes we sync from the fetched policy below.
  const [name, setName] = useState("");
  const [cedarText, setCedarText] = useState("");
  const [severity, setSeverity] = useState<PolicySeverity>("deny");

  const listQ = useQuery({ queryKey: ["policies"], queryFn: listPolicies });
  const templatesQ = useQuery({ queryKey: ["policy-templates"], queryFn: getPolicyTemplates });
  const examplesQ = useQuery({ queryKey: ["example-transactions"], queryFn: getExampleTransactions });

  // When selectedId is a real id, fetch it (so we always see the saved
  // state, not the stale list row).
  const detailQ = useQuery({
    queryKey: ["policy", selectedId],
    queryFn: () => getPolicy(selectedId as number),
    enabled: typeof selectedId === "number",
  });

  // Sync draft state when selection changes.
  useEffect(() => {
    if (selectedId === null) return;
    if (selectedId === "new") {
      setName("");
      setCedarText("permit(principal, action, resource);");
      setSeverity("deny");
      return;
    }
    if (detailQ.data) {
      setName(detailQ.data.name);
      setCedarText(detailQ.data.cedar_text);
      setSeverity(detailQ.data.severity);
    }
  }, [selectedId, detailQ.data]);

  // ── debounced live validation ────────────────────────────────────────
  const [validateState, setValidateState] = useState<ValidateResp | null>(null);
  const [validating, setValidating] = useState(false);
  useEffect(() => {
    if (cedarText.trim() === "") {
      setValidateState({ ok: false, error: "cedar_text must not be empty" });
      return;
    }
    setValidating(true);
    const t = setTimeout(() => {
      validatePolicyLocal(cedarText)
        .then(setValidateState)
        .catch((e) => setValidateState({ ok: false, error: String(e) }))
        .finally(() => setValidating(false));
    }, 400);
    return () => clearTimeout(t);
  }, [cedarText]);

  // ── save / create / delete ───────────────────────────────────────────
  const createMut = useMutation({
    mutationFn: () =>
      createPolicy({ name: name.trim() || "untitled", cedar_text: cedarText, severity }),
    onSuccess: (resp) => {
      qc.invalidateQueries({ queryKey: ["policies"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      setSelectedId(resp.id);
    },
  });
  const patchMut = useMutation({
    mutationFn: () =>
      patchPolicy(selectedId as number, { name: name.trim() || "untitled", cedar_text: cedarText, severity }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["policies"] });
      qc.invalidateQueries({ queryKey: ["policy", selectedId] });
    },
  });
  const deleteMut = useMutation({
    mutationFn: () => deletePolicy(selectedId as number),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["policies"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      setSelectedId(null);
    },
  });

  const isEditing = selectedId !== null;
  const isExisting = typeof selectedId === "number";

  const onSave = () => {
    if (selectedId === "new") createMut.mutate();
    else if (isExisting) patchMut.mutate();
  };
  const onDelete = () => {
    if (!isExisting) return;
    if (!confirm(`정책 "${name}"을 삭제할까요?`)) return;
    deleteMut.mutate();
  };

  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle={listQ.data ? `${listQ.data.length} policies` : "…"}
      />
      <div className="editor-layout">
        <aside className="policy-side">
          <div className="side-head">
            <h3>설치된 정책</h3>
            <span className="cnt">{listQ.data?.length ?? 0}</span>
          </div>
          <button className="new-btn" onClick={() => setSelectedId("new")}>+ 새 정책</button>
          <div className="side-list">
            {listQ.isLoading && <div style={{ padding: 10, fontSize: 12 }}>불러오는 중…</div>}
            {listQ.data?.map((p) => (
              <button
                key={p.id}
                className={`pi${selectedId === p.id ? " active" : ""}`}
                onClick={() => setSelectedId(p.id)}
              >
                <div>
                  {p.name}
                  <span className={`sev ${p.severity}`}>{p.severity}</span>
                </div>
                <span className="sub">id #{p.id} · {p.enabled ? "enabled" : "disabled"}</span>
              </button>
            ))}
            {listQ.data?.length === 0 && (
              <div style={{ padding: 10, fontSize: 12, color: "var(--slate-400)" }}>
                정책이 없습니다. 위 "+ 새 정책" 또는 우측 템플릿에서 시작.
              </div>
            )}
          </div>
        </aside>

        <div className="editor-main">
          {!isEditing && (
            <div className="empty-editor">
              <div>
                <strong>정책을 선택하거나 새로 만드세요</strong>
                좌측 리스트에서 기존 정책을 클릭하거나 "+ 새 정책".
                <br />또는 아래 템플릿에서 불러올 수 있습니다.
              </div>
            </div>
          )}

          {isEditing && (
            <>
              <div className="editor-card">
                <div className="ec-head">
                  <input
                    className="name-input"
                    type="text"
                    placeholder="정책 이름"
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                  />
                  <select value={severity} onChange={(e) => setSeverity(e.target.value as PolicySeverity)}>
                    <option value="deny">deny (차단)</option>
                    <option value="warn">warn (경고)</option>
                    <option value="info">info (정보)</option>
                  </select>
                  <span className="grow" />
                  <TemplateMenu templates={templatesQ.data} onPick={(t) => {
                    setCedarText(t.cedar_text);
                    setSeverity(t.severity);
                    if (selectedId === "new" && !name) setName(t.name.ko || t.name.en);
                  }} />
                </div>
                <textarea
                  spellCheck={false}
                  value={cedarText}
                  onChange={(e) => setCedarText(e.target.value)}
                  placeholder="permit(principal, action, resource) when { /* … */ };"
                />
                <div className="ec-foot">
                  <ValidateBadge validating={validating} state={validateState} />
                  <span className="spacer" />
                  {isExisting && (
                    <button
                      className="btn danger"
                      onClick={onDelete}
                      disabled={deleteMut.isPending}
                    >
                      삭제
                    </button>
                  )}
                  <button
                    className="btn primary"
                    onClick={onSave}
                    disabled={createMut.isPending || patchMut.isPending || Boolean(validateState && !validateState.ok && cedarText.trim() !== "")}
                  >
                    {createMut.isPending || patchMut.isPending ? "저장 중…" : isExisting ? "저장" : "정책 생성"}
                  </button>
                </div>
              </div>

              {(createMut.error || patchMut.error || deleteMut.error) && (
                <div className="err-banner">
                  {fmtErr(createMut.error || patchMut.error || deleteMut.error)}
                </div>
              )}

              <TestPanel
                cedarText={cedarText}
                cedarOk={validateState?.ok ?? false}
                examples={examplesQ.data}
              />
            </>
          )}
        </div>
      </div>
    </>
  );
}

// ── validate badge ──────────────────────────────────────────────────────

function ValidateBadge({ validating, state }: { validating: boolean; state: ValidateResp | null }) {
  if (validating || !state) return <span className="status checking">검증 중…</span>;
  if (state.ok) return <span className="status ok">✓ 구문 OK</span>;
  return <span className="status err" title={state.error}>✗ {state.error}</span>;
}

// ── template picker ─────────────────────────────────────────────────────

function TemplateMenu({ templates, onPick }: { templates?: PolicyTemplate[]; onPick: (t: PolicyTemplate) => void }) {
  return (
    <select
      onChange={(e) => {
        const t = templates?.find((x) => x.id === e.target.value);
        if (t) onPick(t);
        e.target.value = "";
      }}
      defaultValue=""
    >
      <option value="" disabled>📋 템플릿 불러오기…</option>
      {templates?.map((t) => (
        <option key={t.id} value={t.id}>{t.name.ko || t.name.en}</option>
      ))}
    </select>
  );
}

// ── test panel ──────────────────────────────────────────────────────────

function TestPanel({
  cedarText,
  cedarOk,
  examples,
}: {
  cedarText: string;
  cedarOk: boolean;
  examples?: ExampleTransaction[];
}) {
  // Default request — easy starting point so the user can hit "테스트" immediately.
  const defaultReq: CedarRequestInput = useMemo(
    () => ({
      principal: 'Wallet::"0x0000000000000000000000000000000000000000"',
      action: 'Action::"Amm::Swap"',
      resource: 'Protocol::"0x0000000000000000000000000000000000000000"',
      entities: [],
      context: {},
    }),
    [],
  );
  const [requestJson, setRequestJson] = useState(() => JSON.stringify(defaultReq, null, 2));
  const [result, setResult] = useState<TestPolicyResp | null>(null);
  const [parseErr, setParseErr] = useState<string | null>(null);

  // Cedar evaluation now runs in-browser via @scopeball/cedar-wasm.
  // No server roundtrip, no need to save the policy first — we evaluate
  // the current draft directly.
  const testMut = useMutation({
    mutationFn: () => {
      let req: CedarRequestInput;
      try {
        req = JSON.parse(requestJson) as CedarRequestInput;
      } catch (e) {
        throw new Error(`JSON parse: ${e instanceof Error ? e.message : String(e)}`);
      }
      if (!cedarOk) throw new Error("Cedar 문법이 유효해야 테스트할 수 있습니다");
      return testPolicyLocal(cedarText, req);
    },
    onSuccess: (resp) => {
      setResult(resp);
      setParseErr(null);
    },
    onError: (e) => {
      setResult(null);
      setParseErr(e instanceof Error ? e.message : String(e));
    },
  });

  const loadExample = (ex: ExampleTransaction) => {
    // Best-effort mapping: example's `meta.from` → principal, derive action
    // from `action.domain/kind`, mash context+enrichment into context object.
    const meta = (ex.meta as Record<string, unknown>) ?? {};
    const action = (ex.action as Record<string, unknown>) ?? {};
    const from = (meta.from as string) ?? "0x0000000000000000000000000000000000000000";
    const to = (meta.to as string) ?? "0x0000000000000000000000000000000000000000";
    const domain = String(action.domain ?? "Generic");
    const kind = String(action.kind ?? "Tx");
    const actionKey = `${capitalize(domain)}::${capitalize(kind)}`;
    const ctx = { ...(ex.context ?? {}), ...((ex.enrichment as Record<string, unknown>) ?? {}) };
    const req: CedarRequestInput = {
      principal: `Wallet::"${from}"`,
      action: `Action::"${actionKey}"`,
      resource: `Protocol::"${to}"`,
      entities: [],
      context: ctx,
    };
    setRequestJson(JSON.stringify(req, null, 2));
    setResult(null);
    setParseErr(null);
  };

  return (
    <div className="editor-card test-card">
      <h4>테스트</h4>
      <div className="tc-controls">
        <select
          onChange={(e) => {
            const ex = examples?.find((x) => x.id === e.target.value);
            if (ex) loadExample(ex);
            e.target.value = "";
          }}
          defaultValue=""
        >
          <option value="" disabled>🧪 예시 트랜잭션…</option>
          {examples?.map((ex) => (
            <option key={ex.id} value={ex.id}>{ex.label.ko || ex.label.en}</option>
          ))}
        </select>
        <span style={{ flex: 1 }} />
        <button
          className="btn primary"
          onClick={() => testMut.mutate()}
          disabled={!cedarOk || testMut.isPending}
          title={!cedarOk ? "Cedar 문법 검증 통과 후 테스트 가능" : ""}
        >
          {testMut.isPending ? "실행 중…" : "테스트 실행"}
        </button>
      </div>
      <textarea
        spellCheck={false}
        value={requestJson}
        onChange={(e) => setRequestJson(e.target.value)}
      />
      {parseErr && <div className="err-banner" style={{ marginTop: 8 }}>{parseErr}</div>}
      {result && (
        <div className={`verdict-result ${result.verdict}`}>
          <div className="v-head">
            <span className={`sev-pill ${result.verdict}`}><span className="pd" />{result.verdict}</span>
            <span>매칭 {result.matched.length}건</span>
          </div>
          {result.matched.map((m, i) => (
            <div key={i} className="matched-row">
              · {m.policy_id} <span style={{ color: "var(--fail-700)" }}>[{m.severity}]</span>
              {m.reason ? ` — ${m.reason}` : ""}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function capitalize(s: string): string {
  return s.length > 0 ? s[0].toUpperCase() + s.slice(1) : s;
}

function fmtErr(e: unknown): string {
  if (e instanceof ServerError) return `${e.status} ${String(e.body)}`;
  return String(e);
}
