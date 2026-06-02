import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";

import {
  dashboardId,
  deleteManagedPolicy,
  listManagedPolicies,
  putPolicy,
  type ManagedPolicy,
} from "../server-api";
import type { PolicySeverity } from "../server-api";

import { stampAnnotations } from "../editor-v7/annotations";
import { initialDoc as makeInitialDoc } from "../editor-v7/doc";
import { EditorShell, parseTree } from "../editor-v7/EditorShell";
import { serializeDoc } from "../editor-v7/serialize";
import { listTemplates, type PolicyTemplate } from "../editor-v7/templates";
import type { Doc } from "../editor-v7/types";
import { Topbar } from "../shell/Topbar";
import "./editor.css";

/**
 * Policy editor — Scratch-style block builder on top of the v7 doc model,
 * with a Cedar code fallback. Persists to the extension's
 * `chrome.storage.local` via the SW dashboard handlers; there is no
 * server-side policy table anymore.
 *
 * The sidebar lists every `dashboard::*` policy the SW knows about; the
 * builder/code editor lives inside `<EditorShell>`. Save serializes
 * the v7 tree + stamps `@id` / `@severity` / `@reason` annotations and
 * pushes the result through `dashboard:put-raw`.
 */

type Selected = string | "new" | null;

const SEVERITY_RE = /@severity\("(deny|warn|info)"\)/;
const ID_ANNOTATION_RE = /@id\("([^"]+)"\)/;

function severityFromCedar(text: string): PolicySeverity {
  const m = text.match(SEVERITY_RE);
  return (m?.[1] as PolicySeverity | undefined) ?? "deny";
}

function nameFromPolicy(p: ManagedPolicy): string {
  if (p.displayName?.trim()) return p.displayName.trim();
  const m = p.text.match(ID_ANNOTATION_RE);
  return m?.[1] ?? "untitled";
}

type ShellSeed = {
  initialCedarText: string;
  initialDoc: Doc | null;
  initialMode: "builder" | "code";
};

export function EditorPage() {
  const qc = useQueryClient();
  const [params, setParams] = useSearchParams();
  const [selectedId, setSelectedId] = useState<Selected>(null);

  // Honor ?new=1 (NavRail CTA) and ?policy=<id> (Home → Editor link).
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
      setSelectedId(pid);
      const next = new URLSearchParams(params);
      next.delete("policy");
      setParams(next, { replace: true });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Editor-local state ─ kept here, not in EditorShell, so save/delete
  // mutations can read the latest snapshot.
  const [name, setName] = useState("");
  const [severity, setSeverity] = useState<PolicySeverity>("deny");
  const [cedarText, setCedarText] = useState("");
  const [treeJson, setTreeJson] = useState<string | null>(null);

  const [shellSeed, setShellSeed] = useState<ShellSeed | null>(null);
  const [shellKey, setShellKey] = useState(0);

  const listQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
  });

  const selectedPolicy = useMemo(() => {
    if (typeof selectedId !== "string" || selectedId === "new") return null;
    return listQ.data?.find((p) => p.id === selectedId) ?? null;
  }, [listQ.data, selectedId]);

  // Re-seed the editor whenever selection changes.
  useEffect(() => {
    if (selectedId === null) {
      setShellSeed(null);
      return;
    }
    if (selectedId === "new") {
      setName("");
      setSeverity("deny");
      const seedDoc = makeInitialDoc({ action: "Amm::Swap" });
      const seedCedar = serializeDoc(seedDoc);
      setShellSeed({ initialCedarText: seedCedar, initialDoc: seedDoc, initialMode: "builder" });
      setShellKey((k) => k + 1);
      setCedarText(seedCedar);
      setTreeJson(null);
      return;
    }
    if (selectedPolicy) {
      setName(nameFromPolicy(selectedPolicy));
      setSeverity(severityFromCedar(selectedPolicy.text));
      const parsedDoc = parseTree(selectedPolicy.policyTree ?? null);
      setShellSeed({
        initialCedarText: selectedPolicy.text,
        initialDoc: parsedDoc,
        initialMode: parsedDoc ? "builder" : "code",
      });
      setShellKey((k) => k + 1);
      setCedarText(selectedPolicy.text);
      setTreeJson(selectedPolicy.policyTree ?? null);
    }
  }, [selectedId, selectedPolicy]);

  // ── mutations ─────────────────────────────────────────────────────
  const stampedCedar = () =>
    stampAnnotations(cedarText, name.trim() || "untitled", severity);

  const saveMut = useMutation({
    mutationFn: async () => {
      const cedar = stampedCedar();
      const trimmedName = name.trim() || "untitled";
      const id =
        selectedId === "new" || selectedId === null
          ? dashboardId(crypto.randomUUID())
          : selectedId;
      await putPolicy({
        id,
        cedarText: cedar,
        policyTree: treeJson,
        displayName: trimmedName,
      });
      return id;
    },
    onSuccess: (id) => {
      qc.invalidateQueries({ queryKey: ["managed-policies"] });
      setSelectedId(id);
    },
  });

  const deleteMut = useMutation({
    mutationFn: async () => {
      if (typeof selectedId !== "string" || selectedId === "new") return;
      await deleteManagedPolicy(selectedId);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["managed-policies"] });
      setSelectedId(null);
    },
  });

  const isEditing = selectedId !== null;
  const isExisting = typeof selectedId === "string" && selectedId !== "new";

  const onSave = () => saveMut.mutate();
  const onDelete = () => {
    if (!isExisting) return;
    if (!confirm(`정책 "${name}"을 삭제할까요?`)) return;
    deleteMut.mutate();
  };

  const onTemplatePick = (t: PolicyTemplate) => {
    setSeverity(t.severity);
    if (selectedId === "new" && !name) setName(t.name.ko || t.name.en);
    setShellSeed({
      initialCedarText: t.cedar_text,
      initialDoc: null,
      initialMode: "code",
    });
    setShellKey((k) => k + 1);
    setCedarText(t.cedar_text);
    setTreeJson(null);
  };

  const saveDisabled = useMemo(() => {
    if (saveMut.isPending) return true;
    return !cedarText.trim();
  }, [saveMut.isPending, cedarText]);

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
            {listQ.data?.map((p) => {
              const sev = severityFromCedar(p.text);
              return (
                <button
                  key={p.id}
                  className={`pi${selectedId === p.id ? " active" : ""}`}
                  onClick={() => setSelectedId(p.id)}
                >
                  <div>
                    <span className="mode-badge" title={p.policyTree ? "Builder 트리" : "Code 전용"}>
                      {p.policyTree ? "🧱" : "📝"}
                    </span>
                    {nameFromPolicy(p)}
                    <span className={`sev ${sev}`}>{sev}</span>
                  </div>
                  <span className="sub">{p.id}</span>
                </button>
              );
            })}
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

          {isEditing && shellSeed && (
            <>
              <div className="meta-row">
                <input
                  className="name-input"
                  type="text"
                  placeholder="정책 이름"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                />
                <select
                  value={severity}
                  onChange={(e) => setSeverity(e.target.value as PolicySeverity)}
                >
                  <option value="deny">deny (차단)</option>
                  <option value="warn">warn (경고)</option>
                  <option value="info">info (정보)</option>
                </select>
                <span className="grow" />
                <TemplateMenu onPick={onTemplatePick} />
                {isExisting && (
                  <button
                    className="btn-danger"
                    onClick={onDelete}
                    disabled={deleteMut.isPending}
                  >
                    삭제
                  </button>
                )}
                <button className="btn-primary" onClick={onSave} disabled={saveDisabled}>
                  {saveMut.isPending ? "저장 중…" : isExisting ? "저장" : "정책 생성"}
                </button>
              </div>

              {(saveMut.error || deleteMut.error) && (
                <div className="err-banner">
                  {String(saveMut.error || deleteMut.error)}
                </div>
              )}

              <EditorShell
                key={shellKey}
                initialCedarText={shellSeed.initialCedarText}
                initialDoc={shellSeed.initialDoc}
                initialMode={shellSeed.initialMode}
                onChange={(next) => {
                  setCedarText(next.cedarText);
                  setTreeJson(next.treeJson);
                }}
              />
            </>
          )}
        </div>
      </div>
    </>
  );
}

function TemplateMenu({ onPick }: { onPick: (t: PolicyTemplate) => void }) {
  const templates = listTemplates();
  return (
    <select
      onChange={(e) => {
        const t = templates.find((x) => x.id === e.target.value);
        if (t) onPick(t);
        e.target.value = "";
      }}
      defaultValue=""
    >
      <option value="" disabled>📋 템플릿 불러오기…</option>
      {templates.map((t) => (
        <option key={t.id} value={t.id}>{t.name.ko || t.name.en}</option>
      ))}
    </select>
  );
}
