// OutputRow — Phase 7.2 / extended in Phase 8.5.
//
// One row in a `requires[i].outputs[]` list. An output declares a Cedar
// type that gets attached to the action's `CustomContext` after install.
//
// Two render modes:
//
//   1. Legacy / free-text mode (`lockedMethod` undefined):
//      - `field` = free text (Cedar identifier)
//      - `type`  = full alias-table dropdown
//      - `from`  = generic `$.result` selector picker
//      Used by the catalog-less path (e.g. daemon down + no bundled
//      catalog) AND for the user's manually-added "extra" outputs on a
//      catalog-driven row.
//
//   2. Catalog-driven primary output (`lockedMethod` provided AND
//      `isPrimaryOutput` true):
//      - `field` = still free text (this is the user's chosen name
//        for `context.custom.<field>` — must be user-chosen).
//      - `type`  = LOCKED to `lockedMethod.returns.type`, rendered as
//        a disabled chip so the user can see what they're getting but
//        can't pick something incompatible.
//      - `from`  = LOCKED to `$.result` for record returns; for
//        scalar returns we render a dropdown of `$.result` (the
//        record itself) plus the leaf paths so the user can extract
//        a sub-value (e.g. `$.result.bps` → `Long`).
//      This is the row that gets pre-filled from the method choice.
//
//   3. Catalog-driven extra output (`lockedMethod` provided AND
//      `isPrimaryOutput` false): degrades to mode 1 — the user is
//      adding their own "side projection" of the same RPC response
//      (e.g. pulling `$.result.value` as a `decimal` alongside the
//      primary `UsdValuation` record), and we don't want to constrain
//      that.

import type { MethodCatalogEntry } from "@scopeball/sdk";
import { SelectorPicker } from "./selector-picker";

export interface OutputDraft {
  field: string;
  type: string;
  from: string;
  required: boolean;
}

export interface AliasOption {
  name: string;
  kind: "scalar" | "record";
  cedarSpelling: string;
}

export interface OutputRowProps {
  action: string;
  value: OutputDraft;
  aliasOptions: ReadonlyArray<AliasOption>;
  /**
   * Catalog entry for the parent requirement's method, or undefined
   * when the requirement is in legacy free-text mode. Drives the
   * type/from lock behaviour.
   */
  lockedMethod?: MethodCatalogEntry;
  /**
   * Whether this row is the FIRST output for its requirement. Only
   * the first one is treated as "primary" and locked to the method's
   * `returns`; additional rows the user adds via `+ Add output` are
   * side-projections and stay free-text.
   */
  isPrimaryOutput?: boolean;
  onChange: (next: OutputDraft) => void;
  onRemove: () => void;
}

export function OutputRow(props: OutputRowProps): JSX.Element {
  const { value, aliasOptions, onChange, onRemove, lockedMethod } = props;
  const isPrimary = props.isPrimaryOutput === true;
  const catalogLock = lockedMethod !== undefined && isPrimary;

  return (
    <div className="output-row">
      <label className="output-row-cell">
        <span>Field</span>
        <input
          type="text"
          aria-label="output field name"
          value={value.field}
          onChange={(e) => onChange({ ...value, field: e.target.value })}
          placeholder="e.g. usdValue"
        />
      </label>

      <label className="output-row-cell">
        <span>Type</span>
        {catalogLock ? (
          // Locked: the daemon's `returns.type` is the source of truth.
          // Disable so a wrong-type selection can't sneak in via
          // keyboard. Wrap the locked label in a faux-input so it
          // visually balances against the other slots.
          <span
            className="output-row-type-locked"
            aria-label="output type (locked to method return)"
            title={`Locked to ${lockedMethod.returns.type} by ${lockedMethod.name}`}
          >
            {lockedMethod.returns.type}
            {lockedMethod.returns.kind === "record" ? " (record)" : ""}
          </span>
        ) : (
          <select
            aria-label="output type"
            value={value.type}
            onChange={(e) => onChange({ ...value, type: e.target.value })}
          >
            <option value="">— select —</option>
            {aliasOptions.map((opt) => (
              <option key={opt.name} value={opt.name}>
                {opt.cedarSpelling}
                {opt.kind === "record" ? " (record)" : ""}
              </option>
            ))}
          </select>
        )}
      </label>

      <div className="output-row-cell output-row-cell-from">
        <span>From</span>
        {catalogLock ? (
          // For record returns, the only sensible `from` is `$.result`
          // (the whole record). For scalar returns the catalog dictates
          // the leaf path. We render a tiny dropdown of just the catalog-
          // declared options so the user can switch between the whole
          // record and named leaves when applicable. For now there's
          // only one option per method, but the shape supports growth
          // when we let a single requirement project multiple primary
          // outputs.
          <select
            aria-label="output from"
            value={value.from}
            onChange={(e) => onChange({ ...value, from: e.target.value })}
          >
            {fromCandidates(lockedMethod).map((candidate) => (
              <option key={candidate} value={candidate}>
                {candidate}
              </option>
            ))}
          </select>
        ) : (
          <SelectorPicker
            mode="result"
            action={props.action}
            value={value.from}
            onChange={(next) => onChange({ ...value, from: next })}
          />
        )}
      </div>

      <label className="output-row-cell output-row-cell-required">
        <input
          type="checkbox"
          aria-label="output required"
          checked={value.required}
          onChange={(e) =>
            onChange({ ...value, required: e.target.checked })
          }
        />
        <span>required</span>
      </label>

      <button
        type="button"
        className="output-row-remove"
        onClick={onRemove}
        aria-label="remove output"
      >
        ×
      </button>
    </div>
  );
}

/**
 * Candidate `from` paths the locked row may carry. Records expose
 * `$.result` (the whole record); scalars expose the catalog's
 * declared `from` path. Returning an array keeps the dropdown
 * extensible — once we let one method project multiple primary
 * outputs (e.g. both UsdValuation and a derived `Long`), we widen
 * this list and the dropdown grows accordingly.
 */
function fromCandidates(method: MethodCatalogEntry): string[] {
  if (method.returns.kind === "record") {
    return ["$.result"];
  }
  return [method.returns.from];
}
