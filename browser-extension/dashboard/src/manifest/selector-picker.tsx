// Selector picker — Phase 7.1, evolved through Phase 8.5 (PR 3+4).
//
// Manifests reference data via dollar-rooted selectors (`$.root.chain_id`,
// `$.action.inputToken.amount.value`, `$.context.swapMode`,
// `$.result.usd_value`). The picker presents one of three views depending
// on what its parent knows about the slot:
//
//   1. **Typed slot** (`requiredType` provided) — flat list of paths
//      the WASM declares as compatible with that Cedar type. A
//      `Long` slot doesn't surface String paths; an `AssetRef` slot
//      only shows the two `$.action.*.asset` composite paths. Locks
//      the user out of the cross-type wiring that PR 1~3 left open.
//
//   2. **Untyped params slot** (no `requiredType`) — full $.root +
//      $.action drill-down tree. Used for legacy / catalog-less
//      requirements where we don't know the param's declared type.
//
//   3. **Result slot** (`mode="result"`) — `$.result.*` free-text
//      input. The RPC response shape is server-defined and unknown
//      at design time.
//
// In every mode the user can still TYPE a custom path into the top
// input — the typed-filter list is an opinionated short-list, not a
// straitjacket.

import { useEffect, useMemo, useState } from "react";
import {
  fetchActionSchema,
  fetchTypedPaths,
  type TypedPaths,
} from "../policy/builder-wasm";
import type { FieldDto } from "../policy/types";
import "./selector-picker.css";

export interface SelectorPickerProps {
  /** Which selector roots are valid in this slot. */
  mode: "params" | "result";
  /** Action name (snake_case) — used to look up the action envelope shape. */
  action: string;
  /** Current selector value, e.g. `"$.root.chain_id"` or `""`. */
  value: string;
  /** Called with the new selector string. */
  onChange: (next: string) => void;
  /**
   * Catalog-declared Cedar type the picker should filter to. Scalars
   * match by cedar_type spelling (`Long`/`String`/`Bool`/`decimal`/
   * `Set<String>`); records match by alias (`AssetRef`/`UsdValuation`/
   * `Validity`/...). When absent, the picker falls back to the
   * untyped tree view so legacy / unknown-catalog cases still work.
   *
   * `params` mode only — result-mode pickers never receive this prop
   * because RPC return shapes aren't declared client-side.
   */
  requiredType?: string;
}

// --- Base schema snapshots (hand-coded from spec §"Selector roots") -------

/**
 * `$.root.*` is the same on every action — it's the calldata-derived
 * `RootInput` (manifest.rs:38). Five public fields, one optional.
 * Used by the untyped tree view; the typed view pulls from
 * `fetchTypedPaths` which includes the same set.
 */
const ROOT_FIELDS: ReadonlyArray<string> = [
  "chain_id",
  "from",
  "to",
  "value_wei",
  "block_timestamp",
];

// --- Tree shape (built from FieldDto[] at runtime) ----------------------

interface PathTreeNode {
  segment: string;
  children: Map<string, PathTreeNode>;
}

function buildPathTree(fields: ReadonlyArray<FieldDto>): PathTreeNode {
  const root: PathTreeNode = { segment: "", children: new Map() };
  for (const f of fields) {
    if (f.isCustom) continue;
    const segs = f.path.split(".");
    let node = root;
    for (const seg of segs) {
      let child = node.children.get(seg);
      if (!child) {
        child = { segment: seg, children: new Map() };
        node.children.set(seg, child);
      }
      node = child;
    }
  }
  return root;
}

/**
 * Filter a `TypedPaths` payload down to the entries that match a
 * required type spelling.
 *
 * Scalars are matched case-insensitively against `cedarType`. Records
 * are matched against `cedarAlias` directly (alias names are
 * PascalCase by convention; comparing as-is keeps the dashboard
 * mapping symmetric with the daemon).
 *
 * Returns an array (possibly empty). The caller distinguishes between
 * "filter not active" (typed-paths not loaded yet) and "filter
 * matched zero paths" via the typed-paths state, not this function's
 * return. An empty array here is a real signal: "the action has no
 * paths of this type" (e.g. swap exposes no Bool field), and the
 * picker shows an explanatory empty state rather than silently
 * degrading to the untyped tree.
 */
function filterTypedPaths(
  paths: TypedPaths,
  requiredType: string,
): string[] {
  const wanted = requiredType.trim();
  if (wanted.length === 0) return [];

  const scalarMap: Record<string, string> = {
    Long: "long",
    String: "string",
    Bool: "bool",
    decimal: "decimal",
    "Set<String>": "set_of_string",
    "Set<Long>": "set_of_long",
  };
  const scalarKey = scalarMap[wanted];

  const out = new Set<string>();
  if (scalarKey !== undefined) {
    for (const s of paths.scalars) {
      if (s.cedarType === scalarKey) out.add(s.path);
    }
  } else {
    // Record alias — match against catalog's record list.
    for (const r of paths.records) {
      if (r.cedarAlias === wanted) out.add(r.path);
    }
  }
  return Array.from(out).sort();
}

// --- Component ------------------------------------------------------------

type ExpandedRoot = "root" | "action" | "context" | "result" | null;

export function SelectorPicker(props: SelectorPickerProps): JSX.Element {
  const { mode, action, value, onChange, requiredType } = props;
  const [expanded, setExpanded] = useState<ExpandedRoot>(null);
  const [schemaFields, setSchemaFields] = useState<ReadonlyArray<FieldDto>>(
    [],
  );
  // Typed-paths cache — only fetched when a `requiredType` was
  // supplied. Empty array on failure so the rest of the picker can
  // fall back to the untyped tree.
  const [typedPaths, setTypedPaths] = useState<TypedPaths | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (mode !== "params") return;
    fetchActionSchema(action)
      .then((res) => {
        if (cancelled) return;
        if (res.schema) setSchemaFields(res.schema.fields);
      })
      .catch(() => {
        /* see comment below — schema fetch failures degrade silently */
      });
    return () => {
      cancelled = true;
    };
  }, [action, mode]);

  useEffect(() => {
    let cancelled = false;
    if (mode !== "params" || !requiredType) return;
    fetchTypedPaths(action)
      .then((res) => {
        if (cancelled) return;
        if (res.paths) setTypedPaths(res.paths);
      })
      .catch(() => {
        /* fail soft — untyped tree still renders */
      });
    return () => {
      cancelled = true;
    };
  }, [action, mode, requiredType]);

  const tree = useMemo(() => buildPathTree(schemaFields), [schemaFields]);

  // Typed-slot short-list. `null` ONLY when:
  //   - no requiredType prop (untyped slot — show tree), or
  //   - typed-paths fetch hasn't landed yet (loading — show tree as
  //     graceful pre-load state).
  // An EMPTY array (typed-mode "no compatible paths") is distinct: we
  // stay in typed mode and surface an explanatory empty state instead
  // of degrading to the tree. That tree fallback was confusing when
  // the slot's type genuinely has zero matches (e.g. a Bool param on
  // a swap action whose calldata exposes no Bool field).
  const matchingPaths = useMemo<string[] | null>(() => {
    if (!requiredType || !typedPaths) return null;
    return filterTypedPaths(typedPaths, requiredType);
  }, [requiredType, typedPaths]);

  function toggle(root: Exclude<ExpandedRoot, null>) {
    setExpanded((prev) => (prev === root ? null : root));
  }

  function pickRootChild(field: string) {
    onChange(`$.root.${field}`);
    setExpanded(null);
  }
  function pickPath(prefix: "$.action" | "$.context", suffix: string) {
    onChange(suffix === "" ? prefix : `${prefix}.${suffix}`);
    setExpanded(null);
  }

  // ── Typed-slot render ────────────────────────────────────────────
  // When a typed-paths short-list is available, we render it INSTEAD
  // of the tree. The user can still type freely in the top input as
  // an escape hatch (e.g. a custom selector the static schema doesn't
  // surface).
  //
  // The text input doubles as a live filter — typing `$.ro` narrows
  // the list to paths containing that substring (case-insensitive).
  // When zero paths match the typed string, we revert to showing the
  // full list with a "no matches" note so the user can still see
  // their options instead of staring at an empty dropdown.
  if (mode === "params" && matchingPaths !== null) {
    return (
      <TypedPickerView
        requiredType={requiredType!}
        matchingPaths={matchingPaths}
        value={value}
        onChange={onChange}
      />
    );
  }

  return (
    <div className="selector-picker">
      <input
        type="text"
        className="selector-picker-input"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={
          mode === "params" ? "$.root.* | $.action.* | $.context.*" : "$.result.*"
        }
        aria-label="Selector path"
      />

      {mode === "params" ? (
        <ul className="selector-picker-tree">
          <li>
            <button
              type="button"
              className="selector-root"
              onClick={() => toggle("root")}
              aria-expanded={expanded === "root"}
            >
              $.root
            </button>
            {expanded === "root" ? (
              <ul className="selector-picker-children">
                {ROOT_FIELDS.map((f) => (
                  <li key={f}>
                    <button
                      type="button"
                      className="selector-leaf"
                      onClick={() => pickRootChild(f)}
                    >
                      {f}
                    </button>
                  </li>
                ))}
              </ul>
            ) : null}
          </li>

          <li>
            <button
              type="button"
              className="selector-root"
              onClick={() => toggle("action")}
              aria-expanded={expanded === "action"}
            >
              $.action
            </button>
            {expanded === "action" ? (
              <SubtreeView
                tree={tree}
                prefix=""
                onPick={(suffix) => pickPath("$.action", suffix)}
              />
            ) : null}
          </li>

          <li>
            <button
              type="button"
              className="selector-root"
              onClick={() => toggle("context")}
              aria-expanded={expanded === "context"}
            >
              $.context
            </button>
            {expanded === "context" ? (
              <SubtreeView
                tree={tree}
                prefix=""
                onPick={(suffix) => pickPath("$.context", suffix)}
              />
            ) : null}
          </li>
        </ul>
      ) : (
        <ul className="selector-picker-tree">
          <li>
            <button
              type="button"
              className="selector-root"
              onClick={() => toggle("result")}
              aria-expanded={expanded === "result"}
            >
              $.result
            </button>
            {expanded === "result" ? (
              <ul className="selector-picker-children">
                <li className="selector-fallback">
                  <label>
                    $.result path
                    <input
                      type="text"
                      aria-label="$.result path"
                      placeholder="$.result.usd_value"
                      onChange={(e) => onChange(e.target.value)}
                    />
                  </label>
                </li>
              </ul>
            ) : null}
          </li>
        </ul>
      )}
    </div>
  );
}

/**
 * Typed-slot view: replaces the untyped tree when the param's
 * `requiredType` is known. Three sub-renders:
 *
 *   - `Bool` empty state → `true` / `false` literal buttons. Bool
 *     params (e.g. `approval.cover_inputs.allowances_cover_inputs`)
 *     don't have a calldata path to wire from on most actions, so we
 *     offer the only sensible alternative — pick a literal — directly
 *     as buttons instead of pretending a typed selector list applies.
 *
 *   - Other empty state → text input + an explanatory hint. Lets
 *     power users type a custom selector when the catalog declares a
 *     type the action's calldata can't satisfy.
 *
 *   - Non-empty matching paths → text input + COLLAPSIBLE list. The
 *     list stays hidden when the current value is already a valid
 *     pick (saves vertical space on a many-param form) and expands
 *     when the user wants to switch.
 */
function TypedPickerView({
  requiredType,
  matchingPaths,
  value,
  onChange,
}: {
  requiredType: string;
  matchingPaths: readonly string[];
  value: string;
  onChange: (next: string) => void;
}): JSX.Element {
  const empty = matchingPaths.length === 0;

  // Bool literal picker — only shown when the action has no Bool
  // path. Two clicks: `true` or `false`. The manifest serializer
  // (draftToManifest) coerces these to JSON booleans so the daemon
  // receives the actual bool literal, not the string.
  if (empty && requiredType === "Bool") {
    return (
      <div className="selector-picker selector-picker-bool">
        <div className="selector-bool-buttons">
          <button
            type="button"
            className={`selector-bool-option${
              value === "true" ? " selector-bool-option-active" : ""
            }`}
            onClick={() => onChange("true")}
          >
            true
          </button>
          <button
            type="button"
            className={`selector-bool-option${
              value === "false" ? " selector-bool-option-active" : ""
            }`}
            onClick={() => onChange("false")}
          >
            false
          </button>
        </div>
        <p className="selector-typed-hint">
          <code>Bool</code> param: pick a literal. The manifest stores
          this as a JSON boolean, not a selector.
        </p>
      </div>
    );
  }

  // Non-Bool empty state — only the input + hint, no list to render.
  if (empty) {
    return (
      <div className="selector-picker selector-picker-typed">
        <input
          type="text"
          className="selector-picker-input"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={`type: ${requiredType} — no compatible action paths; type a literal or custom selector`}
          aria-label="Selector path"
        />
        <p className="selector-typed-hint">
          The action exposes no path of type <code>{requiredType}</code>.
          Type a literal value or a custom selector in the input above.
        </p>
      </div>
    );
  }

  return (
    <TypedPickerWithList
      requiredType={requiredType}
      matchingPaths={matchingPaths}
      value={value}
      onChange={onChange}
    />
  );
}

/**
 * The main typed-list renderer with collapsible behaviour. Split out
 * so the open/closed state owns its own scope and doesn't leak into
 * the empty/Bool branches above.
 *
 * Open by default when the current value isn't yet a valid pick
 * (fresh row, after method swap that doesn't preserve selection).
 * Collapses to a compact "(current) ▾ change" affordance once the
 * user picks something or already had a valid selection.
 */
function TypedPickerWithList({
  requiredType,
  matchingPaths,
  value,
  onChange,
}: {
  requiredType: string;
  matchingPaths: readonly string[];
  value: string;
  onChange: (next: string) => void;
}): JSX.Element {
  // `isValid` follows the value over re-renders (filter strings,
  // method swaps) without needing a state sync — when the value
  // becomes invalid (e.g. user clears the input), the open-state
  // useEffect below reopens the list.
  const isValid = matchingPaths.includes(value);
  const [open, setOpen] = useState(!isValid);

  useEffect(() => {
    // Auto-sync open state with validity whenever the value or the
    // path-set changes — opens for invalid values (user is composing),
    // closes for valid ones (the slot is settled). The effect ONLY
    // fires on dependency changes, so a chevron click (which only
    // mutates `open`) preserves the user's manual toggle. Two
    // important consequences:
    //   - Method swap: paramsFromCatalog injects a new (valid)
    //     selector → value changes → effect closes the list, so the
    //     freshly-populated form doesn't render N expanded blocks.
    //   - User edits to an invalid value (clears, types a custom
    //     path that's not in matchingPaths) → effect re-opens so
    //     they can scan the typed list again.
    setOpen(!matchingPaths.includes(value));
  }, [value, matchingPaths]);

  // Filter logic: substring match only fires when the value is a
  // "filter query" (user is mid-typing, no valid pick yet). Once the
  // value matches a path exactly (`isValid` true), suppress the
  // filter so opening the list shows EVERY option — the user wants
  // to browse alternatives, not see the same item repeated.
  const trimmed = value.trim().toLowerCase();
  const isFilterActive = trimmed !== "" && !isValid;
  const filtered = isFilterActive
    ? matchingPaths.filter((p) => p.toLowerCase().includes(trimmed))
    : matchingPaths;
  const filterFallback = isFilterActive && filtered.length === 0;
  const visible = filterFallback ? matchingPaths : filtered;

  function handlePick(p: string) {
    onChange(p);
    setOpen(false);
  }

  return (
    <div className="selector-picker selector-picker-typed">
      <div className="selector-typed-input-row">
        <input
          type="text"
          className="selector-picker-input"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={`type: ${requiredType} — start typing to filter`}
          aria-label="Selector path"
        />
        <button
          type="button"
          className="selector-typed-toggle"
          onClick={() => setOpen((prev) => !prev)}
          aria-expanded={open}
          aria-label={open ? "collapse options" : "show options"}
          title={open ? "Collapse options" : `Show ${matchingPaths.length} options`}
        >
          {open ? "▴" : "▾"}
        </button>
      </div>
      {open ? (
        <ul className="selector-typed-list">
          {visible.map((p) => (
            <li key={p}>
              <button
                type="button"
                className={`selector-typed-option${
                  p === value ? " selector-typed-option-active" : ""
                }`}
                onClick={() => handlePick(p)}
              >
                {p}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      <p className="selector-typed-hint">
        {open ? (
          filterFallback ? (
            <>
              No path contains <code>{value.trim()}</code>. Showing all{" "}
              {matchingPaths.length} path
              {matchingPaths.length === 1 ? "" : "s"} compatible with{" "}
              <code>{requiredType}</code>.
            </>
          ) : (
            <>
              {filtered.length} of {matchingPaths.length} path
              {matchingPaths.length === 1 ? "" : "s"} compatible with{" "}
              <code>{requiredType}</code>. Type above to filter or override.
            </>
          )
        ) : (
          <>
            Compatible with <code>{requiredType}</code>. Click ▾ to switch.
          </>
        )}
      </p>
    </div>
  );
}

function SubtreeView({
  tree,
  prefix,
  onPick,
}: {
  tree: PathTreeNode;
  prefix: string;
  onPick: (suffix: string) => void;
}): JSX.Element {
  if (tree.children.size === 0) {
    return (
      <ul className="selector-picker-children">
        <li className="selector-fallback">
          <label>
            Custom path
            <input
              type="text"
              aria-label="custom selector suffix"
              placeholder="e.g. inputToken.asset.address"
              onChange={(e) =>
                onPick(e.target.value.replace(/^\.+/, ""))
              }
            />
          </label>
        </li>
      </ul>
    );
  }
  return (
    <ul className="selector-picker-children">
      {Array.from(tree.children.values()).map((child) => (
        <SubtreeNode
          key={child.segment}
          node={child}
          prefix={prefix === "" ? child.segment : `${prefix}.${child.segment}`}
          onPick={onPick}
        />
      ))}
    </ul>
  );
}

function SubtreeNode({
  node,
  prefix,
  onPick,
}: {
  node: PathTreeNode;
  prefix: string;
  onPick: (suffix: string) => void;
}): JSX.Element {
  const [open, setOpen] = useState(false);
  const hasChildren = node.children.size > 0;

  return (
    <li className="selector-subtree-node">
      <div className="selector-subtree-row">
        {hasChildren ? (
          <button
            type="button"
            className="selector-chevron"
            onClick={(e) => {
              e.stopPropagation();
              setOpen((prev) => !prev);
            }}
            aria-expanded={open}
            aria-label={open ? "collapse" : "expand"}
          >
            {open ? "▾" : "▸"}
          </button>
        ) : (
          <span className="selector-chevron-spacer" />
        )}
        <button
          type="button"
          className="selector-leaf"
          onClick={() => onPick(prefix)}
        >
          {node.segment}
        </button>
      </div>
      {hasChildren && open ? (
        <ul className="selector-picker-children selector-subtree-children">
          {Array.from(node.children.values()).map((child) => (
            <SubtreeNode
              key={child.segment}
              node={child}
              prefix={`${prefix}.${child.segment}`}
              onPick={onPick}
            />
          ))}
        </ul>
      ) : null}
    </li>
  );
}
