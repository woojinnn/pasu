// Manifest editor page — Phase 7.2.
//
// Per-action manifest authoring UI. Route: `/manifests/:action`.
//
// One form drives one `PolicyManifest`:
//   - `id`                  — top-level user-chosen string
//   - `requires[]`          — rows of (id, method, optional, params, outputs)
//   - `params[k] = selector` — uses SelectorPicker (params mode)
//   - `outputs[i]`          — uses OutputRow (which embeds a result-mode picker)
//
// Buttons:
//   - Preview → SDK.previewManifest, then navigate to `/schema?action=…`
//               so the (future) schema viewer can render the diff.
//   - Save    → SDK.putManifest (which goes through the atomic-install
//               path in the SW); on rejected error envelope (the SDK
//               request wrapper throws an Error annotated with the
//               `{kind, message}` from the server), surface the message.
//
// State is held as local React state (mirroring `LibraryPage.tsx`'s
// pattern — no react-hook-form anywhere in the codebase yet).

import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import type { AliasTableEntry, PolicyManifest } from "@scopeball/sdk";
import { useExtension } from "../sdk-context";
import { OutputRow, type OutputDraft } from "../manifest/output-row";
import { SelectorPicker } from "../manifest/selector-picker";
import "./manifest-editor.css";

interface ParamRow {
  key: string;
  selector: string;
}

interface RequiresRow {
  id: string;
  method: string;
  optional: boolean;
  params: ParamRow[];
  outputs: OutputDraft[];
}

interface ManifestDraft {
  id: string;
  requires: RequiresRow[];
}

/**
 * sessionStorage key for the manifest-editor → schema-viewer Preview
 * hand-off. The viewer consumes (and clears) this slot on mount when
 * the URL carries `?fromPreview=true`.
 */
export const PREVIEW_HANDOFF_KEY = "manifest-editor:preview-handoff";

function emptyDraft(): ManifestDraft {
  return { id: "", requires: [] };
}

function emptyRequires(): RequiresRow {
  return { id: "", method: "", optional: false, params: [], outputs: [] };
}

function emptyOutput(): OutputDraft {
  return { field: "", type: "", from: "", required: false };
}

// Per-field validation for the form draft (carry-over L). The Save
// button is disabled until `valid` is true; individual reasons are
// surfaced inline next to each offending field via the `errors` map.
interface DraftValidation {
  valid: boolean;
  manifestIdErr: string | null;
  // requirementErrs[i] = error for the i-th requirement row.
  requirementErrs: Array<{
    idErr: string | null;
    methodErr: string | null;
    needsOutputs: boolean;
    outputErrs: Array<{ fieldErr: string | null; typeErr: string | null }>;
  }>;
}

export function validateDraft(draft: ManifestDraft): DraftValidation {
  const manifestIdErr = draft.id.trim() === "" ? "Manifest id is required" : null;
  const requirementErrs = draft.requires.map((r) => {
    const idErr = r.id.trim() === "" ? "Requirement id is required" : null;
    const methodErr = r.method.trim() === "" ? "Method is required" : null;
    // A required (non-optional) requirement must declare at least one
    // output — otherwise it contributes nothing to context.custom and
    // can't actually fail closed.
    const nonEmptyOutputs = r.outputs.filter((o) => o.field.trim() !== "");
    const needsOutputs = !r.optional && nonEmptyOutputs.length === 0;
    const outputErrs = r.outputs.map((o) => ({
      fieldErr: o.field.trim() === "" ? "Field name is required" : null,
      typeErr: o.type.trim() === "" ? "Type is required" : null,
    }));
    return { idErr, methodErr, needsOutputs, outputErrs };
  });
  const valid =
    manifestIdErr === null &&
    requirementErrs.every(
      (e) =>
        e.idErr === null &&
        e.methodErr === null &&
        !e.needsOutputs &&
        e.outputErrs.every((o) => o.fieldErr === null && o.typeErr === null),
    );
  return { valid, manifestIdErr, requirementErrs };
}

// Convert the form draft to the wire `PolicyManifest`. `params` becomes a
// k→selector record per requirement. We strip empty rows so the user can
// keep blank scaffolding rows in the UI without polluting the manifest.
function draftToManifest(draft: ManifestDraft, action: string): PolicyManifest {
  return {
    id: draft.id,
    schema_version: 1,
    requires: draft.requires.map((r) => {
      const params: Record<string, string> = {};
      for (const p of r.params) {
        if (p.key) params[p.key] = p.selector;
      }
      return {
        id: r.id,
        when: { action },
        method: r.method,
        optional: r.optional,
        params,
        outputs: r.outputs
          .filter((o) => o.field)
          .map((o) => ({
            kind: "context",
            field: o.field,
            type: o.type,
            from: o.from,
            required: o.required,
          })),
      };
    }),
  };
}

// Reverse — used when an existing manifest is loaded from storage so the
// editor can pre-fill. Tolerant of missing fields.
function manifestToDraft(m: PolicyManifest): ManifestDraft {
  const reqs = Array.isArray(m.requires) ? m.requires : [];
  return {
    id: m.id ?? "",
    requires: reqs.map((raw) => {
      const r = raw as Partial<{
        id: string;
        method: string;
        optional: boolean;
        params: Record<string, string>;
        outputs: Array<{
          field?: string;
          type?: string;
          from?: string;
          required?: boolean;
        }>;
      }>;
      return {
        id: r.id ?? "",
        method: r.method ?? "",
        optional: r.optional ?? false,
        params: Object.entries(r.params ?? {}).map(([key, selector]) => ({
          key,
          selector,
        })),
        outputs: (r.outputs ?? []).map((o) => ({
          field: o.field ?? "",
          type: o.type ?? "",
          from: o.from ?? "",
          required: o.required ?? false,
        })),
      };
    }),
  };
}

export function ManifestEditor(): JSX.Element {
  const { action = "" } = useParams<{ action: string }>();
  const navigate = useNavigate();
  const { client } = useExtension();

  const [draft, setDraft] = useState<ManifestDraft>(emptyDraft);
  const [aliasEntries, setAliasEntries] = useState<AliasTableEntry[]>([]);
  const [busy, setBusy] = useState<null | "preview" | "save">(null);
  const [err, setErr] = useState<{ kind: string; message: string } | null>(
    null,
  );
  const [info, setInfo] = useState<string | null>(null);

  // Load alias table + existing manifest on mount. Alias table is pure;
  // the manifest may be null (fresh action).
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [aliases, { manifest }] = await Promise.all([
          client.getAliasTable(),
          client.getManifest(action),
        ]);
        if (cancelled) return;
        setAliasEntries(aliases.entries);
        if (manifest) setDraft(manifestToDraft(manifest));
      } catch (e) {
        // Loading is non-fatal — the user can still author from scratch.
        // We log so the error isn't completely silent for devs.
        console.warn("[ManifestEditor] mount load failed:", e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client, action]);

  const onPreview = useCallback(async () => {
    setBusy("preview");
    setErr(null);
    setInfo(null);
    try {
      const output = await client.previewManifest(action, draftToManifest(draft, action));
      // Phase 7 codex carry-over J: stash the preview output in
      // `sessionStorage` so SchemaViewer can render it as a "draft
      // preview" overlay instead of just re-fetching the
      // currently-installed schema. The key is namespaced by action
      // (one preview per action at a time); SchemaViewer consumes and
      // clears the slot on mount.
      try {
        sessionStorage.setItem(
          PREVIEW_HANDOFF_KEY,
          JSON.stringify({ action, output, savedAtMs: Date.now() }),
        );
      } catch {
        // sessionStorage is unavailable (e.g. happy-dom test env edge
        // cases). Fall through to the legacy hand-off — SchemaViewer
        // will just render the installed schema.
      }
      navigate(`/schema?action=${encodeURIComponent(action)}&fromPreview=true`);
    } catch (e) {
      setErr(extractErr(e));
    } finally {
      setBusy(null);
    }
  }, [client, action, draft, navigate]);

  const onSave = useCallback(async () => {
    setBusy("save");
    setErr(null);
    setInfo(null);
    try {
      const result = await client.putManifest(action, draftToManifest(draft, action));
      setInfo(`Installed. enrichedSchemaHash=${result.enrichedSchemaHash}`);
    } catch (e) {
      setErr(extractErr(e));
    } finally {
      setBusy(null);
    }
  }, [client, action, draft]);

  // Phase 7 codex carry-over L: validate before enabling Save.
  // `Preview` stays clickable while invalid — the user is allowed to
  // probe the server-side validator without committing.
  const validation = useMemo(() => validateDraft(draft), [draft]);

  const aliasOptions = useMemo(
    () =>
      aliasEntries.map((e) => ({
        name: e.name,
        kind: e.kind,
        cedarSpelling: e.cedarSpelling,
      })),
    [aliasEntries],
  );

  return (
    <div className="manifest-editor">
      <header className="manifest-editor-head">
        <h1>
          Manifest <code>{action}</code>
        </h1>
        <label className="manifest-id-field">
          Manifest id
          <input
            type="text"
            aria-label="manifest id"
            value={draft.id}
            onChange={(e) => setDraft({ ...draft, id: e.target.value })}
            placeholder="user.swap.v1"
          />
        </label>
      </header>

      <section className="manifest-requires">
        <div className="manifest-requires-head">
          <h2>Requirements</h2>
          <button
            type="button"
            onClick={() =>
              setDraft({
                ...draft,
                requires: [...draft.requires, emptyRequires()],
              })
            }
          >
            + Add requirement
          </button>
        </div>

        {draft.requires.length === 0 ? (
          <p className="manifest-empty">
            No requirements yet. Add one to begin.
          </p>
        ) : null}

        {draft.requires.map((r, ri) => (
          <RequiresEditor
            key={ri}
            action={action}
            value={r}
            aliasOptions={aliasOptions}
            onChange={(next) =>
              setDraft({
                ...draft,
                requires: draft.requires.map((x, i) => (i === ri ? next : x)),
              })
            }
            onRemove={() =>
              setDraft({
                ...draft,
                requires: draft.requires.filter((_, i) => i !== ri),
              })
            }
          />
        ))}
      </section>

      <footer className="manifest-editor-foot">
        <button type="button" onClick={onPreview} disabled={busy !== null}>
          Preview
        </button>
        <button
          type="button"
          onClick={onSave}
          disabled={busy !== null || !validation.valid}
          title={
            validation.valid
              ? undefined
              : "Resolve the highlighted fields before saving."
          }
          aria-disabled={busy !== null || !validation.valid}
        >
          Save
        </button>
        {!validation.valid ? (
          <span
            className="manifest-validation-hint"
            data-testid="manifest-validation-hint"
            role="note"
          >
            {validation.manifestIdErr ?? "Some requirements are incomplete."}
          </span>
        ) : null}
      </footer>

      {err ? (
        <div className="manifest-err">
          <strong>{err.kind}</strong>: {renderErrMessage(err.message)}
        </div>
      ) : null}
      {info ? <div className="manifest-info">{info}</div> : null}
    </div>
  );
}

interface RequiresEditorProps {
  action: string;
  value: RequiresRow;
  aliasOptions: ReadonlyArray<{
    name: string;
    kind: "scalar" | "record";
    cedarSpelling: string;
  }>;
  onChange: (next: RequiresRow) => void;
  onRemove: () => void;
}

function RequiresEditor(props: RequiresEditorProps): JSX.Element {
  const { action, value, aliasOptions, onChange, onRemove } = props;

  return (
    <div className="manifest-requires-row">
      <div className="manifest-requires-grid">
        <label>
          Requirement id
          <input
            type="text"
            aria-label="requirement id"
            value={value.id}
            onChange={(e) => onChange({ ...value, id: e.target.value })}
            placeholder="oracle-usd"
          />
        </label>
        <label>
          Method
          <input
            type="text"
            aria-label="requirement method"
            value={value.method}
            onChange={(e) => onChange({ ...value, method: e.target.value })}
            placeholder="oracle.usd_value"
          />
        </label>
        <label className="manifest-requires-optional">
          <input
            type="checkbox"
            aria-label="requirement optional"
            checked={value.optional}
            onChange={(e) =>
              onChange({ ...value, optional: e.target.checked })
            }
          />
          optional (D9 — missing data degrades verdict, doesn't fail)
        </label>
        <button
          type="button"
          className="manifest-requires-remove"
          aria-label="remove requirement"
          onClick={onRemove}
        >
          × Remove
        </button>
      </div>

      <details className="manifest-params">
        <summary>Params ({value.params.length})</summary>
        {value.params.map((p, i) => (
          <div key={i} className="manifest-param-row">
            <input
              type="text"
              aria-label="param key"
              value={p.key}
              onChange={(e) =>
                onChange({
                  ...value,
                  params: value.params.map((x, j) =>
                    j === i ? { ...x, key: e.target.value } : x,
                  ),
                })
              }
              placeholder="param key"
            />
            <SelectorPicker
              mode="params"
              action={action}
              value={p.selector}
              onChange={(next) =>
                onChange({
                  ...value,
                  params: value.params.map((x, j) =>
                    j === i ? { ...x, selector: next } : x,
                  ),
                })
              }
            />
            <button
              type="button"
              aria-label="remove param"
              onClick={() =>
                onChange({
                  ...value,
                  params: value.params.filter((_, j) => j !== i),
                })
              }
            >
              ×
            </button>
          </div>
        ))}
        <button
          type="button"
          onClick={() =>
            onChange({
              ...value,
              params: [...value.params, { key: "", selector: "" }],
            })
          }
        >
          + Add param
        </button>
      </details>

      <details className="manifest-outputs" open>
        <summary>Outputs ({value.outputs.length})</summary>
        {value.outputs.map((o, i) => (
          <OutputRow
            key={i}
            action={action}
            value={o}
            aliasOptions={aliasOptions}
            onChange={(next) =>
              onChange({
                ...value,
                outputs: value.outputs.map((x, j) => (j === i ? next : x)),
              })
            }
            onRemove={() =>
              onChange({
                ...value,
                outputs: value.outputs.filter((_, j) => j !== i),
              })
            }
          />
        ))}
        <button
          type="button"
          onClick={() =>
            onChange({
              ...value,
              outputs: [...value.outputs, emptyOutput()],
            })
          }
        >
          + Add output
        </button>
      </details>
    </div>
  );
}

// The SDK request wrapper throws `Error & {kind, message}` when the SW
// returns an `{ok: false, error}` envelope. We recover that shape here so
// the UI can show `<kind>: <message>` without re-parsing the message.
function extractErr(e: unknown): { kind: string; message: string } {
  if (e && typeof e === "object") {
    const obj = e as { kind?: unknown; message?: unknown };
    const kind = typeof obj.kind === "string" ? obj.kind : "error";
    const message =
      typeof obj.message === "string" ? obj.message : String(e);
    return { kind, message };
  }
  return { kind: "error", message: String(e) };
}

// Pattern matches the policy id conventions used in this codebase:
//   - `dashboard::<slug>` for dashboard-installed policies
//   - `user::<slug>` for user-installed via SDK
//   - `__system__` synthetic ids
//   - quoted-id-of-the-form 'X' already declared by Y (the validator's
//     duplicate_field message format).
//
// We highlight any match inside the server error message so the user can
// jump straight to the offending policy. Spec §"/manifests/:action":
// "if the message mentions a specific policy id, highlight that".
const POLICY_ID_PATTERN =
  /(\b(?:dashboard|user|__system__|engine|marketplace)::[A-Za-z0-9_.\-/]+)|(already declared by [A-Za-z0-9_.:\-/]+)/g;

function renderErrMessage(message: string): JSX.Element[] {
  const parts: JSX.Element[] = [];
  const re = new RegExp(POLICY_ID_PATTERN.source, "g");
  let lastIndex = 0;
  let key = 0;
  // matchAll walks the regex without manual stateful state, so we get a
  // simple linear scan over `message`.
  for (const m of message.matchAll(re)) {
    const idx = m.index ?? 0;
    if (idx > lastIndex) {
      parts.push(
        <span key={`t${key++}`}>{message.slice(lastIndex, idx)}</span>,
      );
    }
    const matched = m[0];
    parts.push(
      <mark key={`m${key++}`} className="policy-id-highlight">
        {matched}
      </mark>,
    );
    lastIndex = idx + matched.length;
  }
  if (lastIndex < message.length) {
    parts.push(<span key={`t${key++}`}>{message.slice(lastIndex)}</span>);
  }
  if (parts.length === 0) {
    parts.push(<span key="t0">{message}</span>);
  }
  return parts;
}
