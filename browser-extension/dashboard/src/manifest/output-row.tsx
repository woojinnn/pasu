// OutputRow — Phase 7.2.
//
// One row in a `requires[i].outputs[]` list. An output declares a Cedar
// type that gets attached to the action's `CustomContext` after install.
//
// Fields:
//   - `field`     — text input, must be a valid Cedar identifier (the SW
//                   validator enforces this; we don't duplicate the regex
//                   client-side, but do trim leading whitespace).
//   - `type`      — drop-down sourced from `SDK.getAliasTable()` so the
//                   list of valid Cedar types stays the one source of
//                   truth (BASE_ALIAS_TABLE on the Rust side).
//   - `from`      — selector picker rooted at `$.result` (the RPC
//                   response shape is server-defined).
//   - `required`  — toggle. Drives the D9 failure semantics at runtime.

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
  onChange: (next: OutputDraft) => void;
  onRemove: () => void;
}

export function OutputRow(props: OutputRowProps): JSX.Element {
  const { value, aliasOptions, onChange, onRemove } = props;

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
      </label>

      <div className="output-row-cell output-row-cell-from">
        <span>From</span>
        <SelectorPicker
          mode="result"
          action={props.action}
          value={value.from}
          onChange={(next) => onChange({ ...value, from: next })}
        />
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
