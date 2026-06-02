import { useEffect, useMemo, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import {
  dashboardId,
  deleteManagedPolicy,
  putPolicy,
  type ManagedPolicy,
} from "../../server-api";
import type { PolicySeverity } from "../../server-api";

import { stampAnnotations } from "../../editor-v7/annotations";
import { initialDoc as makeInitialDoc } from "../../editor-v7/doc";
import { EditorShell, parseTree } from "../../editor-v7/EditorShell";
import { serializeDoc } from "../../editor-v7/serialize";
import { listTemplates, type PolicyTemplate } from "../../editor-v7/templates";
import type { Doc } from "../../editor-v7/types";

import { nameFromPolicy, severityFromCedar } from "./policy-meta";

/**
 * Shared editor body for the "new policy" and "edit policy" pages. Owns
 * the meta-row (name / severity / template / save / delete), the
 * `<EditorShell>` (Builder / Code toggle), and the put / delete
 * mutations. Lifted out of the old monolithic `EditorPage` so the
 * list view can live at its own route with its own layout.
 *
 * Routing notes:
 * - `mode="new"`: caller renders this for `/editor/new`. On successful
 *   save we call `onSaved(id)` so the parent can `navigate` to
 *   `/editor/:id`.
 * - `mode="edit"`: caller passes the loaded `policy`. On delete we
 *   call `onDeleted()` so the parent can `navigate("/editor")`.
 */

type ShellSeed = {
  initialCedarText: string;
  initialDoc: Doc | null;
  initialMode: "builder" | "code";
};

export interface EditorPanelProps {
  mode: "new" | "edit";
  /** Existing policy in `mode="edit"`; ignored when `mode="new"`. */
  policy?: ManagedPolicy;
  /** Called with the new (or kept) id after a successful save. */
  onSaved?: (id: string) => void;
  /** Called after a successful delete (edit mode only). */
  onDeleted?: () => void;
}

export function EditorPanel({ mode, policy, onSaved, onDeleted }: EditorPanelProps) {
  const qc = useQueryClient();

  const [name, setName] = useState("");
  const [severity, setSeverity] = useState<PolicySeverity>("deny");
  const [cedarText, setCedarText] = useState("");
  const [treeJson, setTreeJson] = useState<string | null>(null);

  const [shellSeed, setShellSeed] = useState<ShellSeed | null>(null);
  const [shellKey, setShellKey] = useState(0);

  // Seed from props. Re-seeds whenever the parent swaps policy (e.g.
  // a refetch returns updated cedar text after a sibling tab edit).
  useEffect(() => {
    if (mode === "new") {
      setName("");
      setSeverity("deny");
      const seedDoc = makeInitialDoc({ action: "Amm::Swap" });
      const seedCedar = serializeDoc(seedDoc);
      setShellSeed({
        initialCedarText: seedCedar,
        initialDoc: seedDoc,
        initialMode: "builder",
      });
      setShellKey((k) => k + 1);
      setCedarText(seedCedar);
      setTreeJson(null);
      return;
    }
    if (policy) {
      setName(nameFromPolicy(policy));
      setSeverity(severityFromCedar(policy.text));
      const parsedDoc = parseTree(policy.policyTree ?? null);
      setShellSeed({
        initialCedarText: policy.text,
        initialDoc: parsedDoc,
        initialMode: parsedDoc ? "builder" : "code",
      });
      setShellKey((k) => k + 1);
      setCedarText(policy.text);
      setTreeJson(policy.policyTree ?? null);
    }
  }, [mode, policy]);

  // ── mutations ────────────────────────────────────────────────────
  const stampedCedar = () =>
    stampAnnotations(cedarText, name.trim() || "untitled", severity);

  const saveMut = useMutation({
    mutationFn: async () => {
      const cedar = stampedCedar();
      const trimmedName = name.trim() || "untitled";
      // Short 8-char nonce — keeps the popup id line readable.
      const shortNonce = () => crypto.randomUUID().split("-")[0];
      const id =
        mode === "new" || !policy ? dashboardId(shortNonce()) : policy.id;
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
      onSaved?.(id);
    },
  });

  const deleteMut = useMutation({
    mutationFn: async () => {
      if (mode !== "edit" || !policy) return;
      await deleteManagedPolicy(policy.id);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["managed-policies"] });
      onDeleted?.();
    },
  });

  const onSave = () => saveMut.mutate();
  const onDelete = () => {
    if (mode !== "edit" || !policy) return;
    if (!confirm(`정책 "${name}"을 삭제할까요?`)) return;
    deleteMut.mutate();
  };

  const onTemplatePick = (t: PolicyTemplate) => {
    setSeverity(t.severity);
    if (mode === "new" && !name) setName(t.name.ko || t.name.en);
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

  if (!shellSeed) {
    return <div className="empty-editor"><div>로딩 중…</div></div>;
  }

  return (
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
        {mode === "edit" && policy && (
          <button
            className="btn-danger"
            onClick={onDelete}
            disabled={deleteMut.isPending}
          >
            삭제
          </button>
        )}
        <button className="btn-primary" onClick={onSave} disabled={saveDisabled}>
          {saveMut.isPending
            ? "저장 중…"
            : mode === "edit"
              ? "저장"
              : "정책 생성"}
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
      <option value="" disabled>
        📋 템플릿 불러오기…
      </option>
      {templates.map((t) => (
        <option key={t.id} value={t.id}>
          {t.name.ko || t.name.en}
        </option>
      ))}
    </select>
  );
}
