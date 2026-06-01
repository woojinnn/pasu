import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";

import {
  createPolicy,
  deletePolicy,
  getPolicy,
  getPolicyTemplates,
  installPolicyToExtension,
  listPolicies,
  patchPolicy,
  removePolicyFromExtension,
  ServerError,
  type PolicySeverity,
  type PolicyTemplate,
} from "../server-api";
import { stampAnnotations } from "../editor-v7/annotations";
import { initialDoc as makeInitialDoc } from "../editor-v7/doc";
import { EditorShell, parseTree } from "../editor-v7/EditorShell";
import { serializeDoc } from "../editor-v7/serialize";
import type { Doc } from "../editor-v7/types";
import { Topbar } from "../shell/Topbar";
import "./editor.css";

/**
 * Policy editor — v7 block builder by default, Cedar Code mode as a
 * fallback for hand-tuning. `EditorShell` holds the source-of-truth
 * doc + cedar pair; this page just owns the metadata (name, severity)
 * and the save / delete plumbing.
 *
 * Each policy row in the sidebar gets a builder/code badge (🧱 / 📝)
 * driven by whether `policy_tree` is set.
 */
type ShellSeed = {
  initialCedarText: string;
  initialDoc: Doc | null;
  initialMode: "builder" | "code";
};

export function EditorPage() {
  const qc = useQueryClient();
  const [params, setParams] = useSearchParams();
  const [selectedId, setSelectedId] = useState<number | "new" | null>(null);

  // Honor ?new=1 (NavRail CTA) and ?policy=<id> (Home triage Editor link).
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

  // ── editor-local state ──────────────────────────────────────────
  const [name, setName] = useState("");
  const [severity, setSeverity] = useState<PolicySeverity>("deny");
  const [cedarText, setCedarText] = useState("");
  const [treeJson, setTreeJson] = useState<string | null>(null);
  const [cedarValid, setCedarValid] = useState<boolean>(true);

  // Seed only changes when selectedId changes — we re-mount `EditorShell`
  // via `key={shellKey}` so its internal mode/doc/cedar reset cleanly.
  const [shellSeed, setShellSeed] = useState<ShellSeed | null>(null);
  const [shellKey, setShellKey] = useState(0);

  const listQ = useQuery({ queryKey: ["policies"], queryFn: listPolicies });
  const templatesQ = useQuery({ queryKey: ["policy-templates"], queryFn: getPolicyTemplates });

  const detailQ = useQuery({
    queryKey: ["policy", selectedId],
    queryFn: () => getPolicy(selectedId as number),
    enabled: typeof selectedId === "number",
  });

  // Re-seed shell + meta when selection changes.
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
      setCedarValid(true);
      return;
    }
    if (detailQ.data) {
      setName(detailQ.data.name);
      setSeverity(detailQ.data.severity);
      const parsedDoc = parseTree(detailQ.data.policy_tree ?? null);
      setShellSeed({
        initialCedarText: detailQ.data.cedar_text,
        initialDoc: parsedDoc,
        initialMode: parsedDoc ? "builder" : "code",
      });
      setShellKey((k) => k + 1);
      setCedarText(detailQ.data.cedar_text);
      setTreeJson(detailQ.data.policy_tree ?? null);
      setCedarValid(true);
    }
  }, [selectedId, detailQ.data]);

  // ── mutations ────────────────────────────────────────────────────
  // Both create + patch re-stamp `@id` / `@severity` annotations onto
  // the cedar text so the inline metadata always mirrors the DB columns.
  const stampedCedar = () => stampAnnotations(cedarText, name.trim() || "untitled", severity);

  const createMut = useMutation({
    mutationFn: async () => {
      const cedar = stampedCedar();
      const resp = await createPolicy({
        name: name.trim() || "untitled",
        cedar_text: cedar,
        policy_tree: treeJson,
        severity,
      });
      // Dual-write into the extension storage so the popup + wasm engine
      // see the new policy without waiting for a reload. Non-fatal — if
      // the extension isn't installed this falls through silently.
      void installPolicyToExtension(resp.id, cedar);
      return resp;
    },
    onSuccess: (resp) => {
      qc.invalidateQueries({ queryKey: ["policies"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      setSelectedId(resp.id);
    },
  });
  const patchMut = useMutation({
    mutationFn: async () => {
      const id = selectedId as number;
      const cedar = stampedCedar();
      await patchPolicy(id, {
        name: name.trim() || "untitled",
        cedar_text: cedar,
        policy_tree: treeJson,
        severity,
      });
      void installPolicyToExtension(id, cedar);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["policies"] });
      qc.invalidateQueries({ queryKey: ["policy", selectedId] });
    },
  });
  const deleteMut = useMutation({
    mutationFn: async () => {
      const id = selectedId as number;
      await deletePolicy(id);
      void removePolicyFromExtension(id);
    },
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

  const onTemplatePick = (t: PolicyTemplate) => {
    setSeverity(t.severity);
    if (selectedId === "new" && !name) setName(t.name.ko || t.name.en);
    // Apply the cedar text by re-seeding the shell. The tree gets cleared
    // (templates don't ship as v7 trees yet); user can rebuild in Builder
    // by starting from a blank canvas.
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
    if (createMut.isPending || patchMut.isPending) return true;
    if (!cedarText.trim()) return true;
    if (!cedarValid) return true;
    return false;
  }, [createMut.isPending, patchMut.isPending, cedarText, cedarValid]);

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
                  <span className="mode-badge" title={p.policy_tree ? "Builder 트리" : "Code 전용"}>
                    {p.policy_tree ? "🧱" : "📝"}
                  </span>
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
                <select value={severity} onChange={(e) => setSeverity(e.target.value as PolicySeverity)}>
                  <option value="deny">deny (차단)</option>
                  <option value="warn">warn (경고)</option>
                  <option value="info">info (정보)</option>
                </select>
                <span className="grow" />
                <TemplateMenu templates={templatesQ.data} onPick={onTemplatePick} />
                {isExisting && (
                  <button className="btn-danger" onClick={onDelete} disabled={deleteMut.isPending}>
                    삭제
                  </button>
                )}
                <button className="btn-primary" onClick={onSave} disabled={saveDisabled}>
                  {createMut.isPending || patchMut.isPending
                    ? "저장 중…"
                    : isExisting
                      ? "저장"
                      : "정책 생성"}
                </button>
              </div>

              {(createMut.error || patchMut.error || deleteMut.error) && (
                <div className="err-banner">
                  {fmtErr(createMut.error || patchMut.error || deleteMut.error)}
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
                  // Builder mode always produces valid cedar (it serializes
                  // from a structured tree). In Code mode the user might
                  // type something broken — the shell's bottom drawer shows
                  // a status pill; we trust the cedar otherwise and rely on
                  // server-side compile to reject malformed text on save.
                  setCedarValid(next.mode === "builder" ? true : next.cedarText.trim().length > 0);
                }}
              />
            </>
          )}
        </div>
      </div>
    </>
  );
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

function fmtErr(e: unknown): string {
  if (e instanceof ServerError) return `${e.status} ${String(e.body)}`;
  return String(e);
}
