import { useEffect, useMemo, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import {
  dashboardId,
  deleteManagedPolicy,
  putPolicy,
  type ManagedPolicy,
} from "../../server-api";
import type { PolicySeverity } from "../../server-api";

import { stampAnnotations } from "../../editor-v9/annotations";
import { WorkspaceV9 } from "../../editor-v9/Workspace";
import { generateManifest } from "../../editor-v9/manifest-gen";
import type { PolicyIR } from "../../cedar/blocks";

import { nameFromPolicy, severityFromCedar } from "./policy-meta";

/**
 * Shared editor body for the "new policy" and "edit policy" pages. Owns
 * the meta-row (name / severity / save / delete) and the v9 Blockly
 * `<WorkspaceV9>`, plus the put / delete mutations.
 *
 * Routing notes:
 * - `mode="new"`: caller renders this for `/editor/new`. On successful
 *   save we call `onSaved(id)` so the parent can `navigate` to
 *   `/editor/:id`.
 * - `mode="edit"`: caller passes the loaded `policy`. On delete we
 *   call `onDeleted()` so the parent can `navigate("/editor")`.
 */

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
  /** Latest validated policy IR (for enrichment manifest auto-generation). */
  const [ir, setIr] = useState<PolicyIR | null>(null);

  /** Force a Workspace remount when the seeded policy changes. */
  const [workspaceKey, setWorkspaceKey] = useState(0);
  const [initialJson, setInitialJson] = useState<object | null>(null);
  const [ready, setReady] = useState(false);

  // Seed from props. Re-seeds whenever the parent swaps policy (e.g.
  // a refetch returns updated cedar text after a sibling tab edit).
  useEffect(() => {
    if (mode === "new") {
      setName("");
      setSeverity("deny");
      setCedarText("");
      setTreeJson(null);
      setInitialJson(null);
    } else if (policy) {
      setName(nameFromPolicy(policy));
      setSeverity(severityFromCedar(policy.text));
      setCedarText(policy.text);
      setTreeJson(policy.policyTree ?? null);
      setInitialJson(tryParseV9Json(policy.policyTree ?? null));
    }
    setWorkspaceKey((k) => k + 1);
    setReady(true);
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

      // Auto-generate the enrichment manifest from the policy's
      // `context.custom.*` fields. A base-context policy yields `undefined`
      // (no manifest); an unbound enrichment field throws here so the failure
      // surfaces at save time instead of silently fail-opening at runtime.
      let manifest: unknown;
      if (ir) {
        const gen = generateManifest(ir, undefined, { id, severity });
        if (gen.errors.length > 0) {
          throw new Error(gen.errors.map((e) => e.message).join("\n"));
        }
        manifest = gen.manifest;
      }

      await putPolicy({
        id,
        cedarText: cedar,
        policyTree: treeJson,
        displayName: trimmedName,
        ...(manifest !== undefined ? { manifest } : {}),
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

  const saveDisabled = useMemo(() => {
    if (saveMut.isPending) return true;
    return !cedarText.trim();
  }, [saveMut.isPending, cedarText]);

  if (!ready) {
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

      <WorkspaceV9
        key={workspaceKey}
        policyName={name.trim() || "untitled"}
        initialJson={initialJson}
        initialCedarText={mode === "edit" && policy ? policy.text : null}
        onChange={({ cedarText: c, json, ir: nextIr }) => {
          setCedarText(c);
          setTreeJson(JSON.stringify({ v: 9, ws: json }));
          setIr(nextIr ?? null);
        }}
      />
    </>
  );
}

/** v9 stores its workspace JSON wrapped as `{ v:9, ws: {...} }`. Older
 *  v7/v8 docs (no `v:9` marker) are ignored so the editor seeds clean. */
function tryParseV9Json(s: string | null): object | null {
  if (!s) return null;
  try {
    const obj = JSON.parse(s);
    if (obj && typeof obj === "object" && (obj as { v?: number }).v === 9) {
      return (obj as { ws: object }).ws;
    }
    return null;
  } catch {
    return null;
  }
}
