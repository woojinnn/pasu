// Selector picker — Phase 7.1 of the manifest-driven cedarschema feature.
//
// Manifests reference data via dollar-rooted selectors (`$.root.chain_id`,
// `$.action.inputToken.amount`, `$.context.swapMode`, `$.result.usd_value`).
// The picker presents a small tree built from a HAND-CODED snapshot of
// the base schema (spec §"Selector roots") so the UI stays stable while
// the user is mid-edit, regardless of any draft custom fields.
//
// Two modes:
//
//   - `params` — for `requires[].params[k]` values. Roots: `$.root`,
//     `$.action`, `$.context`. The Rust validator (manifest_fragment.rs
//     rule 9) also accepts `$.params` but that's only meaningful when
//     authoring a wrapper requirement; we leave it out of the v1 UI.
//
//   - `result` — for `outputs[].from` selectors. The RPC response shape
//     is server-defined and unknown to the dashboard, so we expose a
//     `$.result` root with a free-text suffix input instead of a tree.
//
// The leaves emit dotted-path strings via `onChange`; the parent stores
// the raw string in the manifest exactly as authored.

import { useMemo, useState } from "react";
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
}

// --- Base schema snapshots (hand-coded from spec §"Selector roots") -------

/**
 * `$.root.*` is the same on every action — it's the calldata-derived
 * `RootInput` (manifest.rs:38). Five public fields, one optional.
 */
const ROOT_FIELDS: ReadonlyArray<string> = [
  "chain_id",
  "from",
  "to",
  "value_wei",
  "block_timestamp",
];

/**
 * `$.action.*` is action-specific. We hand-code the public field names of
 * the most common action envelopes here; unknown actions fall through to
 * a free-text input. Keep this in sync with
 * `schema/policy-schema/actions/**` when adding a new action — the picker
 * will still function for unlisted actions via the free-text fallback,
 * just without autocompletion.
 *
 * Field names here are the *envelope* shape (camelCase), matching the
 * Cedar `<Action>Context` types. We deliberately surface only the
 * top-level field names; users can append `.amount` etc. via the
 * free-text edit input downstream.
 */
const ACTION_FIELDS: Readonly<Record<string, ReadonlyArray<string>>> = {
  swap: ["swapMode", "inputToken", "outputToken", "recipient", "validity", "feeBps"],
  supply: ["asset", "amount", "onBehalfOf"],
  withdraw: ["asset", "amount", "to"],
  borrow: ["asset", "amount", "interestRateMode", "onBehalfOf"],
  repay: ["asset", "amount", "interestRateMode", "onBehalfOf"],
  add_liquidity: ["pool", "amount0Desired", "amount1Desired", "amount0Min", "amount1Min", "recipient"],
  remove_liquidity: ["pool", "liquidity", "amount0Min", "amount1Min", "recipient"],
  stake: ["asset", "amount"],
  wrap: ["amount"],
};

/** `$.context.*` mirrors `$.action.*` for base fields. */
function contextFieldsFor(action: string): ReadonlyArray<string> {
  return ACTION_FIELDS[action] ?? [];
}

// --- Component ------------------------------------------------------------

type ExpandedRoot = "root" | "action" | "context" | "result" | null;

export function SelectorPicker(props: SelectorPickerProps): JSX.Element {
  const { mode, action, value, onChange } = props;
  const [expanded, setExpanded] = useState<ExpandedRoot>(null);

  const actionFields = useMemo(
    () => (ACTION_FIELDS[action] ?? []) as ReadonlyArray<string>,
    [action],
  );

  function toggle(root: Exclude<ExpandedRoot, null>) {
    setExpanded((prev) => (prev === root ? null : root));
  }

  function pickRootChild(field: string) {
    onChange(`$.root.${field}`);
    setExpanded(null);
  }
  function pickActionChild(field: string) {
    onChange(`$.action.${field}`);
    setExpanded(null);
  }
  function pickContextChild(field: string) {
    onChange(`$.context.${field}`);
    setExpanded(null);
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
              <ul className="selector-picker-children">
                {actionFields.length > 0 ? (
                  actionFields.map((f) => (
                    <li key={f}>
                      <button
                        type="button"
                        className="selector-leaf"
                        onClick={() => pickActionChild(f)}
                      >
                        {f}
                      </button>
                    </li>
                  ))
                ) : (
                  <li className="selector-fallback">
                    <label>
                      Custom $.action path
                      <input
                        type="text"
                        aria-label="custom $.action path"
                        placeholder="$.action.foo"
                        onChange={(e) => onChange(e.target.value)}
                      />
                    </label>
                  </li>
                )}
              </ul>
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
              <ul className="selector-picker-children">
                {contextFieldsFor(action).length > 0 ? (
                  contextFieldsFor(action).map((f) => (
                    <li key={f}>
                      <button
                        type="button"
                        className="selector-leaf"
                        onClick={() => pickContextChild(f)}
                      >
                        {f}
                      </button>
                    </li>
                  ))
                ) : (
                  <li className="selector-fallback">
                    <label>
                      Custom $.context path
                      <input
                        type="text"
                        aria-label="custom $.context path"
                        placeholder="$.context.foo"
                        onChange={(e) => onChange(e.target.value)}
                      />
                    </label>
                  </li>
                )}
              </ul>
            ) : null}
          </li>
        </ul>
      ) : (
        // result mode — RPC response shape is server-defined, so we only
        // expose a free-text input rooted at $.result.
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
