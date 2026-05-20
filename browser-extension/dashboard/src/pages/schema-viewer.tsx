// Schema viewer — Phase 7.3.
//
// `/schema` renders the currently-installed enriched cedarschema as a
// tree-ish view of one action's context. Selection happens via the URL
// query param `?action=<snake_case>`; the left rail mirrors
// `REGISTERED_ACTIONS` from `crates/policy-engine/src/schema/action_name.rs`
// (34 entries, kept in sync manually — see the unit test in that file
// for the canonical length).
//
// Data shape: `getEnrichedSchema()` returns the legacy snake-case fields
// (`schema_text`, `schema_hash`, `added_fields`) plus the manifest-driven
// additions in camelCase: `customContexts: Record<action, CustomFieldSource[]>`
// and `schemaHash`. We display `schemaHash` in the hash badge per the
// design spec (D13) and parse base fields out of `schema_text` for the
// "base fields" pane.
//
// Diff overlay: the plan calls for a green/red/amber diff overlay when
// the page is reached from the Preview button in `/manifests/:action`.
// The manifest editor calls `previewManifest` but does NOT persist the
// draft anywhere the viewer can read, so a real diff would need either
// a draft-persistence pass in the SW or a navigation-state hand-off
// from manifest-editor. This v1 deliberately omits the overlay and
// surfaces a note when `?fromPreview=true` is set; the full diff is
// tracked as a Phase-7 follow-up.

import { useEffect, useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import type { EnrichedSchemaOutput, PreviewManifestOutput } from "@scopeball/sdk";
import { useExtension } from "../sdk-context";
import { PREVIEW_HANDOFF_KEY } from "./manifest-editor";
import "./schema-viewer.css";

interface PreviewHandoff {
  action: string;
  output: PreviewManifestOutput;
  savedAtMs: number;
}

// Consume + clear the manifest-editor → schema-viewer hand-off slot.
// Returns the parsed payload when present and matching the requested
// action, else null. The slot is cleared whether or not the action
// matched so a stale draft for a different action doesn't survive.
function consumePreviewHandoff(action: string): PreviewHandoff | null {
  try {
    const raw = sessionStorage.getItem(PREVIEW_HANDOFF_KEY);
    if (!raw) return null;
    sessionStorage.removeItem(PREVIEW_HANDOFF_KEY);
    const parsed = JSON.parse(raw) as Partial<PreviewHandoff>;
    if (!parsed || parsed.action !== action || !parsed.output) return null;
    return parsed as PreviewHandoff;
  } catch {
    return null;
  }
}

// Mirror of `crates/policy-engine/src/schema/action_name.rs::REGISTERED_ACTIONS`.
// Keep ordering identical to the Rust source so the rail matches the
// engine's iteration order.
const REGISTERED_ACTIONS: readonly string[] = [
  "swap",
  "add_liquidity",
  "remove_liquidity",
  "mint_liquidity_nft",
  "burn_liquidity_nft",
  "increase_liquidity",
  "decrease_liquidity",
  "initialize_pool",
  "donate",
  "supply",
  "withdraw",
  "borrow",
  "repay",
  "liquidate",
  "flash_loan",
  "set_authorization",
  "sign_authorization",
  "revoke",
  "stake",
  "request_unstake",
  "claim_unstake",
  "restake",
  "request_restake_withdrawal",
  "claim_restake_withdrawal",
  "wrap",
  "unwrap",
  "approve",
  "set_approval_for_all",
  "transfer",
  "permit",
  "claim_rewards",
  "sign_message",
  "delegate",
  "vote",
];

// Convert `snake_case` to `PascalCase`. Matches the engine's helper in
// `schema::action_name::snake_to_pascal` so we can find `<Action>Context`
// blocks inside the raw cedarschema text.
function snakeToPascal(snake: string): string {
  return snake
    .split("_")
    .filter((part) => part.length > 0)
    .map((part) => part[0].toUpperCase() + part.slice(1))
    .join("");
}

interface BaseField {
  name: string;
  cedarType: string;
  optional: boolean;
}

// Best-effort base-context field extractor. Given the raw enriched
// cedarschema, find the `type <ActionPascal>Context = { ... };` block
// and pull out its top-level fields. We strip the
// `custom?: <X>CustomContext` bridge field — that's the join point to
// the custom section, not a user-visible base field.
//
// The parser is intentionally simple: cedarschema field syntax inside an
// action context type is `name(?): TypeName,` separated by commas with
// optional trailing whitespace/newlines. Nested records aren't expected
// at the top level of an action context, so we don't recurse.
function parseBaseFields(schemaText: string, actionPascal: string): BaseField[] {
  const blockRe = new RegExp(
    "type\\s+" + actionPascal + "Context\\s*=\\s*\\{([\\s\\S]*?)\\}\\s*;",
    "m",
  );
  const block = schemaText.match(blockRe);
  if (!block) return [];
  const body = block[1];
  const fields: BaseField[] = [];
  for (const line of body.split(/[,\n]/)) {
    const trimmed = line.trim().replace(/,$/, "");
    if (trimmed.length === 0) continue;
    const fieldMatch = trimmed.match(
      /^([A-Za-z_][A-Za-z0-9_]*)\s*(\?)?\s*:\s*([^\s,]+)/,
    );
    if (!fieldMatch) continue;
    const name = fieldMatch[1];
    const optional = fieldMatch[2] === "?";
    const cedarType = fieldMatch[3];
    if (name === "custom") continue; // bridge to <Action>CustomContext
    fields.push({ name, cedarType, optional });
  }
  return fields;
}

// Inner shape of `customContexts[action]` per
// `crates/policy-engine/src/schema/fragment.rs::CustomFieldSource`. The
// SDK declares this as `unknown[]` so we narrow here at the call site.
interface CustomFieldSource {
  field: string;
  cedar_type: string;
  source_method: string;
  source_requirement_id: string;
  source_from: string;
  requirement_optional: boolean;
}

function asCustomFields(arr: unknown[] | undefined): CustomFieldSource[] {
  if (!Array.isArray(arr)) return [];
  return arr.filter((row): row is CustomFieldSource => {
    if (!row || typeof row !== "object") return false;
    const o = row as Record<string, unknown>;
    return typeof o.field === "string" && typeof o.cedar_type === "string";
  });
}

export function SchemaViewer(): JSX.Element {
  const { client } = useExtension();
  const [searchParams] = useSearchParams();
  const action = searchParams.get("action") ?? "swap";
  const fromPreview = searchParams.get("fromPreview") === "true";

  const [schema, setSchema] = useState<EnrichedSchemaOutput | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [showRaw, setShowRaw] = useState(false);
  // Phase 7 carry-over J: when the user reached this page through the
  // ManifestEditor's "Preview" button we read the stashed
  // `PreviewManifestOutput` out of sessionStorage and overlay it on
  // top of the currently-installed schema. The slot is consumed (and
  // cleared) once per navigation so a back-and-forth doesn't repeat.
  const [preview, setPreview] = useState<PreviewHandoff | null>(null);

  useEffect(() => {
    if (!fromPreview) return;
    setPreview(consumePreviewHandoff(action));
  }, [fromPreview, action]);

  useEffect(() => {
    let cancelled = false;
    setErr(null);
    void (async () => {
      try {
        const out = await client.getEnrichedSchema();
        if (!cancelled) setSchema(out);
      } catch (e) {
        if (!cancelled) {
          setErr(e instanceof Error ? e.message : String(e));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // `fromPreview` reloads the schema when arriving from the manifest
    // editor's Preview action — the SW may have a freshly composed
    // schema by then.
  }, [client, fromPreview]);

  const actionPascal = useMemo(() => snakeToPascal(action), [action]);
  // When a Preview hand-off is present we prefer its `enrichedSchemaText`
  // so the base-fields parser sees the schema the draft manifest WOULD
  // produce. Falls back to the installed `schema?.schema_text` otherwise.
  const effectiveSchemaText = useMemo(() => {
    if (preview?.output.enrichedSchemaText) {
      return preview.output.enrichedSchemaText;
    }
    return schema?.schema_text ?? "";
  }, [preview, schema]);
  const baseFields = useMemo(
    () =>
      effectiveSchemaText
        ? parseBaseFields(effectiveSchemaText, actionPascal)
        : [],
    [effectiveSchemaText, actionPascal],
  );
  // Preview customTypes is keyed by snake_case action name. When the
  // hand-off matches the selected action, render its fields; otherwise
  // fall through to the installed schema's `customContexts`.
  const customFields = useMemo(() => {
    if (preview) {
      const match = preview.output.customTypes.find(
        (entry) => entry.name === action,
      );
      if (match) return asCustomFields(match.fields as unknown[]);
    }
    return asCustomFields(
      schema?.customContexts?.[action] as unknown[] | undefined,
    );
  }, [preview, schema, action]);

  // Fix P (D14): when in preview mode, compute the diff between the
  // previewed `customTypes[action]` and the installed
  // `customContexts[action]` — categorise each field as `added`,
  // `removed`, `changed`, or `same` so the row renderer can paint a
  // badge. Removed rows appear in addition to `customFields` so the
  // user sees what would disappear.
  const installedCustomFields = useMemo(
    () =>
      asCustomFields(schema?.customContexts?.[action] as unknown[] | undefined),
    [schema, action],
  );
  type DiffKind = "added" | "removed" | "changed" | "same";
  const diffRows = useMemo(() => {
    if (!preview) return null;
    const installedByField = new Map<string, CustomFieldSource>();
    for (const f of installedCustomFields) installedByField.set(f.field, f);
    const previewByField = new Map<string, CustomFieldSource>();
    for (const f of customFields) previewByField.set(f.field, f);

    const rows: Array<{ field: CustomFieldSource; kind: DiffKind }> = [];
    for (const f of customFields) {
      const installed = installedByField.get(f.field);
      if (!installed) rows.push({ field: f, kind: "added" });
      else if (installed.cedar_type !== f.cedar_type)
        rows.push({ field: f, kind: "changed" });
      else rows.push({ field: f, kind: "same" });
    }
    for (const f of installedCustomFields) {
      if (!previewByField.has(f.field)) rows.push({ field: f, kind: "removed" });
    }
    return rows;
  }, [preview, customFields, installedCustomFields]);

  const diffBadge = (kind: DiffKind): string => {
    switch (kind) {
      case "added":
        return "+";
      case "removed":
        return "−";
      case "changed":
        return "~";
      default:
        return "";
    }
  };

  return (
    <div className="schema-viewer">
      <header className="schema-viewer-head">
        <h1>
          Enriched cedarschema
          {preview ? (
            <span
              className="schema-viewer-draft-pill"
              data-testid="schema-viewer-draft-pill"
            >
              Draft preview
            </span>
          ) : null}
        </h1>
        <div
          className="schema-hash-badge"
          data-testid="schema-hash-badge"
          title={
            preview
              ? "schemaHash — SHA-256 of the previewed (unsaved) enriched schema"
              : "enrichedSchemaHash — SHA-256 of the installed enriched schema"
          }
        >
          Hash:{" "}
          <code>
            {preview
              ? preview.output.schemaHash
              : schema?.schemaHash ?? "—"}
          </code>
        </div>
      </header>

      <div className="schema-viewer-body">
        <nav className="schema-rail" aria-label="actions">
          {REGISTERED_ACTIONS.map((a) => {
            const selected = a === action;
            return (
              <Link
                key={a}
                to={`/schema?action=${encodeURIComponent(a)}`}
                className={
                  "schema-rail-link" + (selected ? " selected" : "")
                }
                aria-current={selected ? "page" : undefined}
              >
                {a}
              </Link>
            );
          })}
        </nav>

        <main className="schema-pane">
          <div className="schema-pane-head">
            <h2>
              <code>{actionPascal}Context</code>
            </h2>
            <button
              type="button"
              className="schema-raw-toggle"
              onClick={() => setShowRaw((v) => !v)}
            >
              {showRaw ? "Tree view" : "Raw Cedar"}
            </button>
          </div>

          {err ? <div className="schema-err">Failed to load: {err}</div> : null}

          {fromPreview && preview ? (
            <div
              className="schema-from-preview"
              data-testid="schema-from-preview-banner"
            >
              <strong>Draft preview from your unsaved manifest.</strong>{" "}
              The schema below reflects the previewed manifest for{" "}
              <code>{preview.action}</code>; it is NOT yet installed in
              the engine.
            </div>
          ) : fromPreview ? (
            <div className="schema-from-preview">
              Showing the currently-installed schema. No unsaved draft
              was found.
            </div>
          ) : null}

          {showRaw ? (
            <pre className="schema-raw" data-testid="schema-raw-pre">
              {preview?.output.enrichedSchemaText ?? schema?.schema_text ?? ""}
            </pre>
          ) : (
            <>
              <section className="schema-section schema-section-base">
                <h3>Base fields</h3>
                {baseFields.length === 0 ? (
                  <p className="schema-empty">
                    No base fields parsed for{" "}
                    <code>{actionPascal}Context</code>.
                  </p>
                ) : (
                  <ul className="schema-field-list">
                    {baseFields.map((f) => (
                      <li
                        key={f.name}
                        className="schema-field schema-field-base"
                        data-testid="base-field-row"
                      >
                        <span className="schema-field-name base">
                          {f.name}
                          {f.optional ? "?" : ""}
                        </span>
                        <span className="schema-field-sep">:</span>
                        <span className="schema-field-type base">
                          {f.cedarType}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </section>

              <section className="schema-section schema-section-custom">
                <h3>Custom fields</h3>
                {diffRows ? (
                  diffRows.length === 0 ? (
                    <p className="schema-empty">
                      No manifest-derived fields for this action.
                    </p>
                  ) : (
                    <ul className="schema-field-list">
                      {diffRows.map(({ field: f, kind }) => {
                        const tooltip =
                          "source_method=" + f.source_method +
                          ", source_requirement_id=" + f.source_requirement_id +
                          ", requirement_optional=" + String(f.requirement_optional);
                        const badge = diffBadge(kind);
                        return (
                          <li
                            key={f.field}
                            className={
                              "schema-field schema-field-custom" +
                              (kind === "same" ? "" : ` schema-field-${kind}`)
                            }
                            data-testid="custom-field-row"
                            data-diff={kind}
                            title={tooltip}
                          >
                            {badge ? (
                              <span
                                className={`schema-diff-badge schema-diff-badge-${kind}`}
                                aria-label={kind}
                                data-testid={`diff-badge-${kind}`}
                              >
                                {badge}
                              </span>
                            ) : null}
                            <span className="schema-field-name custom">
                              {f.field}
                            </span>
                            <span className="schema-field-sep">:</span>
                            <span className="schema-field-type custom">
                              {f.cedar_type}
                            </span>
                            <span className="schema-field-meta">
                              via <code>{f.source_method}</code>
                              {f.requirement_optional ? " (optional)" : ""}
                            </span>
                          </li>
                        );
                      })}
                    </ul>
                  )
                ) : customFields.length === 0 ? (
                  <p className="schema-empty">
                    No manifest-derived fields for this action.
                  </p>
                ) : (
                  <ul className="schema-field-list">
                    {customFields.map((f) => {
                      const tooltip =
                        "source_method=" + f.source_method +
                        ", source_requirement_id=" + f.source_requirement_id +
                        ", requirement_optional=" + String(f.requirement_optional);
                      return (
                        <li
                          key={f.field}
                          className="schema-field schema-field-custom"
                          data-testid="custom-field-row"
                          title={tooltip}
                        >
                          <span className="schema-field-name custom">
                            {f.field}
                          </span>
                          <span className="schema-field-sep">:</span>
                          <span className="schema-field-type custom">
                            {f.cedar_type}
                          </span>
                          <span className="schema-field-meta">
                            via <code>{f.source_method}</code>
                            {f.requirement_optional ? " (optional)" : ""}
                          </span>
                        </li>
                      );
                    })}
                  </ul>
                )}
              </section>
            </>
          )}
        </main>
      </div>
    </div>
  );
}
