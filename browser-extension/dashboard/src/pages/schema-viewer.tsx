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
import type { EnrichedSchemaOutput } from "@scopeball/sdk";
import { useExtension } from "../sdk-context";
import "./schema-viewer.css";

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
  const baseFields = useMemo(
    () =>
      schema?.schema_text ? parseBaseFields(schema.schema_text, actionPascal) : [],
    [schema, actionPascal],
  );
  const customFields = useMemo(
    () => asCustomFields(schema?.customContexts?.[action] as unknown[] | undefined),
    [schema, action],
  );

  return (
    <div className="schema-viewer">
      <header className="schema-viewer-head">
        <h1>Enriched cedarschema</h1>
        <div
          className="schema-hash-badge"
          data-testid="schema-hash-badge"
          title="enrichedSchemaHash — SHA-256 of the installed enriched schema"
        >
          Hash: <code>{schema?.schemaHash ?? "—"}</code>
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

          {fromPreview ? (
            <div className="schema-from-preview">
              Showing the currently-installed schema. Diff overlay vs.
              draft manifests is a Phase-7 follow-up.
            </div>
          ) : null}

          {showRaw ? (
            <pre className="schema-raw" data-testid="schema-raw-pre">
              {schema?.schema_text ?? ""}
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
                {customFields.length === 0 ? (
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
