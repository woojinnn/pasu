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
import type {
  AliasTableEntry,
  MethodCatalog,
  MethodCatalogEntry,
  MethodParamSpec,
  PolicyManifest,
} from "@scopeball/sdk";
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
//
// The draft stores every param value as a string (that's what the
// SelectorPicker emits). The wire shape, however, accepts any JSON
// value — so when the catalog declares a Bool param and the user
// picked the literal `true`/`false`, we serialise as a JSON boolean
// (not the string `"true"`) so the daemon's substitution layer
// doesn't reject it. Selectors (paths starting with `$.`) and
// non-Bool values pass through as-is. `methodCatalog` is consulted
// per-param to decide which branch to take; absent catalog means
// every value goes out as a string (legacy behaviour preserved for
// catalog-less rows).
function draftToManifest(
  draft: ManifestDraft,
  action: string,
  methodCatalog: MethodCatalog | null,
): PolicyManifest {
  return {
    id: draft.id,
    schema_version: 1,
    requires: draft.requires.map((r) => {
      const methodEntry = methodCatalog?.methods?.[r.method];
      const params: Record<string, unknown> = {};
      for (const p of r.params) {
        if (!p.key) continue;
        params[p.key] = serializeParamValue(p.selector, methodEntry?.params?.[p.key]);
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

/**
 * Promote a draft param value (always a string) to its wire-shape JSON
 * type. Today this only matters for Bool literals — selectors and
 * other types stay as strings because the daemon resolves them
 * downstream. Bool literals MUST become JSON booleans so
 * `optionalBoolean()` on the daemon side accepts them.
 *
 * A value that starts with `$.` is treated as a selector regardless
 * of catalog type — we never coerce a path expression, only literals.
 */
function serializeParamValue(
  raw: string,
  spec: MethodParamSpec | undefined,
): unknown {
  if (raw.startsWith("$.")) return raw;
  if (spec?.type === "Bool") {
    if (raw === "true") return true;
    if (raw === "false") return false;
    // Anything else (empty, typo) goes through as string and the
    // daemon validator surfaces the error — better than silently
    // dropping the value.
  }
  return raw;
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
  const [busy, setBusy] = useState<null | "preview" | "save" | "starter">(
    null,
  );
  const [err, setErr] = useState<{ kind: string; message: string } | null>(
    null,
  );
  const [info, setInfo] = useState<string | null>(null);
  // `null`: not yet checked (or no starter pack ships for this action).
  // `PolicyManifest`: a bundled manifest is available — render the
  // "Install starter pack" affordance.
  const [bundledStarter, setBundledStarter] = useState<PolicyManifest | null>(
    null,
  );
  // Catalog drives every dropdown in this editor (Phase 8.5). `null`
  // means "not loaded yet" — children render in legacy free-text mode
  // until it lands. `{methods: {}}` means "loaded but empty", in which
  // case we also fall back to free-text mode rather than blanking the
  // method dropdown.
  const [methodCatalog, setMethodCatalog] = useState<MethodCatalog | null>(
    null,
  );

  // Load alias table + existing manifest + bundled starter-pack manifest
  // + method catalog on mount. None of these block first paint — the
  // editor renders in degraded modes until they land.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [aliases, stored, bundled, catalog] = await Promise.all([
          client.getAliasTable(),
          client.getManifest(action),
          client.getBundledManifest(action),
          client.getMethodCatalog(),
        ]);
        if (cancelled) return;
        setAliasEntries(aliases.entries);
        if (stored.manifest) setDraft(manifestToDraft(stored.manifest));
        setBundledStarter(bundled.manifest);
        setMethodCatalog(catalog);
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
      const output = await client.previewManifest(action, draftToManifest(draft, action, methodCatalog));
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
  }, [client, action, draft, methodCatalog, navigate]);

  const onSave = useCallback(async () => {
    setBusy("save");
    setErr(null);
    setInfo(null);
    try {
      const result = await client.putManifest(action, draftToManifest(draft, action, methodCatalog));
      setInfo(`Installed. enrichedSchemaHash=${result.enrichedSchemaHash}`);
    } catch (e) {
      setErr(extractErr(e));
    } finally {
      setBusy(null);
    }
  }, [client, action, draft, methodCatalog]);

  /**
   * Pull the bundled starter-pack manifest into the editor draft.
   *
   * Phase 8 swap of behaviour: the SW used to auto-seed this on first
   * boot, which silently tied every user's storage to the shipped set
   * and broke when bundled enrichments changed. The user now imports
   * explicitly — they see what's loaded and can edit before saving.
   *
   * We REPLACE rather than merge: the starter pack is a coherent
   * "default config" snapshot, not a set of independent additions, and
   * merging would let stale partial state pollute the result. If the
   * user has unsaved work, they can cancel via the confirm dialog.
   */
  const onInstallStarterPack = useCallback(() => {
    if (!bundledStarter) return;
    const hasExistingContent =
      draft.id !== "" || draft.requires.length > 0;
    if (hasExistingContent) {
      const ok = window.confirm(
        "기존에 작성 중인 매니페스트가 starter pack으로 대체됩니다. 계속할까요?",
      );
      if (!ok) return;
    }
    setDraft(manifestToDraft(bundledStarter));
    setErr(null);
    setInfo(
      "Starter pack을 불러왔습니다. 검토 후 Save를 눌러 설치하세요.",
    );
  }, [bundledStarter, draft]);

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

      {bundledStarter && draft.requires.length === 0 ? (
        <section className="manifest-starter-pack">
          <p>
            이 action(<code>{action}</code>)에 대한 권장 enrichment 모음
            (<strong>{bundledStarter.requires.length}개</strong>)이 익스텐션에
            번들되어 있습니다. 가져온 뒤 검토하고 Save하면 설치됩니다.
          </p>
          <button
            type="button"
            className="manifest-starter-pack-install"
            onClick={onInstallStarterPack}
            disabled={busy !== null}
          >
            Install starter pack ({bundledStarter.requires.length}{" "}
            requirement
            {bundledStarter.requires.length === 1 ? "" : "s"})
          </button>
        </section>
      ) : null}

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
            methodCatalog={methodCatalog}
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
  /**
   * Hybrid catalog (bundled + daemon-dynamic). `null` during initial
   * mount, after which one of two paths render:
   *   - Catalog present → `Method` is a `<select>`, params auto-populate
   *     and lock to catalog keys, outputs lock type/from to the
   *     method's `returns`.
   *   - Catalog empty (no daemon, no bundle) → fall back to free-text
   *     mode. Better than blanking the editor and stranding the user.
   */
  methodCatalog: MethodCatalog | null;
  onChange: (next: RequiresRow) => void;
  onRemove: () => void;
}

function RequiresEditor(props: RequiresEditorProps): JSX.Element {
  const { action, value, aliasOptions, methodCatalog, onChange, onRemove } =
    props;
  const methodEntry: MethodCatalogEntry | undefined =
    methodCatalog?.methods?.[value.method];
  // Catalog-aware mode kicks in when (1) the catalog loaded with at
  // least one method, AND (2) the currently-selected method is in it.
  // A method NOT in the catalog (legacy manifest, custom typed-in
  // name, etc.) falls back to free-text so the user isn't stranded.
  const hasCatalog =
    methodCatalog !== null &&
    Object.keys(methodCatalog.methods ?? {}).length > 0;
  const isCatalogMethod = methodEntry !== undefined;

  /**
   * Switching the method swaps the whole row's params/outputs to the
   * shape the new method declares. Without this, leftover keys from
   * the previous method would still serialise into the saved manifest
   * and the daemon would reject them as `invalid_params`.
   *
   * Preserves the row's `id`, `optional` flag (those are independent
   * of method choice). Uses each param's `defaultSelector` and each
   * method's `returns` to pre-fill sensible values.
   */
  const handleMethodChange = (nextMethod: string) => {
    const nextEntry = methodCatalog?.methods?.[nextMethod];
    if (!nextEntry) {
      // Unknown method — keep params/outputs as-is so a typed legacy
      // name doesn't lose the user's existing wiring.
      onChange({ ...value, method: nextMethod });
      return;
    }
    onChange({
      ...value,
      method: nextMethod,
      params: paramsFromCatalog(nextEntry),
      outputs: outputsFromCatalog(nextEntry),
    });
  };

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
          {hasCatalog ? (
            <select
              aria-label="requirement method"
              value={value.method}
              onChange={(e) => handleMethodChange(e.target.value)}
            >
              {/* Empty option lets the user "unset" and triggers the
                  unknown-method fallback path on subsequent picks. */}
              {value.method === "" || !isCatalogMethod ? (
                <option value={value.method}>
                  {value.method === ""
                    ? "— select method —"
                    : `${value.method} (legacy / not in catalog)`}
                </option>
              ) : null}
              {Object.entries(methodCatalog!.methods).map(([name, entry]) => (
                <option key={name} value={name}>
                  {name}
                  {entry.origin !== "bundled" ? ` (${entry.origin})` : ""}
                </option>
              ))}
            </select>
          ) : (
            <input
              type="text"
              aria-label="requirement method"
              value={value.method}
              onChange={(e) => onChange({ ...value, method: e.target.value })}
              placeholder="oracle.usd_value"
            />
          )}
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

      {methodEntry?.description ? (
        <p className="manifest-method-description">{methodEntry.description}</p>
      ) : null}

      <details className="manifest-params" open={isCatalogMethod}>
        <summary>Params ({value.params.length})</summary>
        {value.params.map((p, i) => {
          const spec = methodEntry?.params?.[p.key];
          return (
            <div key={i} className="manifest-param-row">
              {isCatalogMethod ? (
                <span
                  className="manifest-param-key-locked"
                  title={spec?.description ?? p.key}
                >
                  {p.key}
                  {spec?.required === false ? " (optional)" : ""}
                </span>
              ) : (
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
              )}
              {spec?.enum_ && spec.enum_.length > 0 ? (
                // Closed-set enum (e.g. `source: "coingecko" | …`) —
                // render as <select> so users can't typo a value the
                // daemon will reject.
                <select
                  aria-label="param value"
                  value={p.selector}
                  onChange={(e) =>
                    onChange({
                      ...value,
                      params: value.params.map((x, j) =>
                        j === i ? { ...x, selector: e.target.value } : x,
                      ),
                    })
                  }
                >
                  {p.selector === "" ||
                  !(spec.enum_ as readonly string[]).includes(p.selector) ? (
                    <option value={p.selector}>
                      {p.selector === ""
                        ? "— select —"
                        : `${p.selector} (not in enum)`}
                    </option>
                  ) : null}
                  {spec.enum_.map((v) => (
                    <option key={v} value={v}>
                      {v}
                    </option>
                  ))}
                </select>
              ) : (
                <SelectorPicker
                  mode="params"
                  action={action}
                  /* PR 4: when the catalog declared the param's type,
                     pass it down so the picker can offer a flat list of
                     compatible paths instead of every $.action.* leaf.
                     Without this, a user could wire `amount` (String)
                     to `$.root.chain_id` (Long) and the daemon would
                     only reject at install/runtime. */
                  requiredType={spec?.type}
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
              )}
              {/* Catalog mode locks the param set — no remove button
                  for declared keys; legacy/free-text mode keeps it. */}
              {isCatalogMethod ? null : (
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
              )}
            </div>
          );
        })}
        {isCatalogMethod ? null : (
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
        )}
      </details>

      <details className="manifest-outputs" open>
        <summary>Outputs ({value.outputs.length})</summary>
        {value.outputs.map((o, i) => (
          <OutputRow
            key={i}
            action={action}
            value={o}
            aliasOptions={aliasOptions}
            // For catalog methods, the first output is "primary" —
            // pre-filled from the method's `returns`; we pass the
            // method entry so OutputRow knows which type/from slot
            // to lock vs let the user freely edit.
            lockedMethod={isCatalogMethod ? methodEntry : undefined}
            isPrimaryOutput={i === 0}
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

/**
 * Build the params list from a catalog entry. Each param key gets a
 * row with the `defaultSelector` (or the enum `default`) preloaded —
 * lets the user pick a method and immediately have a usable manifest
 * row without clicking through every param.
 *
 * Required params come first to match the catalog's declared order;
 * optional ones (currently only `source` on `oracle.usd_value`) come
 * after so they sit at the bottom of the list.
 */
function paramsFromCatalog(entry: MethodCatalogEntry): ParamRow[] {
  const keys = Object.keys(entry.params);
  return keys.map<ParamRow>((key) => ({
    key,
    selector: paramInitialValue(entry.params[key]),
  }));
}

function paramInitialValue(spec: MethodParamSpec): string {
  if (spec.defaultSelector !== undefined) return spec.defaultSelector;
  if (spec.default !== undefined) return String(spec.default);
  return "";
}

/**
 * Build the initial outputs list for a catalog entry. We always
 * create exactly one output row aligned with the method's `returns`
 * — the user can add more later via `+ Add output` (e.g. to extract
 * a sub-leaf of a record return into its own scalar slot).
 */
function outputsFromCatalog(entry: MethodCatalogEntry): OutputDraft[] {
  return [
    {
      field: defaultFieldName(entry),
      type: entry.returns.type,
      from: entry.returns.kind === "record" ? "$.result" : entry.returns.from,
      required: false,
    },
  ];
}

/**
 * Guess a starter `outputs[].field` name from the method name. E.g.
 * `oracle.usd_value` → `oracleUsdValue` (camelCase). User edits this
 * to whatever fits their domain (`totalInputUsd`, `swapValueUsd`,
 * etc.) before saving. We pick a name rather than leaving blank so
 * the row reads as wiring-on-arrival.
 */
function defaultFieldName(entry: MethodCatalogEntry): string {
  const parts = entry.name.split(/[.\-_]/g).filter(Boolean);
  if (parts.length === 0) return "value";
  return (
    parts[0] +
    parts
      .slice(1)
      .map((p) => p.charAt(0).toUpperCase() + p.slice(1))
      .join("")
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
