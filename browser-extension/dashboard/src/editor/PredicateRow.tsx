import type { FieldDto, OperatorDto, Predicate } from "../policy/types";
import "./PredicateRow.css";

interface PredicateRowProps {
  predicate: Predicate;
  fields: FieldDto[];
  onChange: (next: Predicate) => void;
  onRemove: () => void;
}

export function PredicateRow({
  predicate,
  fields,
  onChange,
  onRemove,
}: PredicateRowProps) {
  const field = fields.find((f) => f.path === predicate.field);
  const operators: OperatorDto[] = field?.operators ?? [];
  const op = operators.find((o) => o.id === predicate.op);

  const handleField = (path: string) => {
    const nextField = fields.find((f) => f.path === path);
    // Reset op + value when field changes — operators may not overlap
    // between cedar types (e.g. Long has gt/lt, SetOfString has contains).
    const firstOp = nextField?.operators[0];
    const nextOp = firstOp?.id ?? "";
    onChange({
      field: path,
      op: nextOp,
      value: arityToEmpty(firstOp?.arity ?? "one"),
    });
  };

  const handleOp = (opId: string) => {
    const nextOp = operators.find((o) => o.id === opId);
    onChange({
      ...predicate,
      op: opId,
      value: arityToEmpty(nextOp?.arity ?? "one"),
    });
  };

  // Group fields by base vs custom so users see at a glance that the second
  // set is manifest-enriched (context.custom.*) and therefore subject to a
  // runtime `has` guard. We use <optgroup> rather than two selects so keyboard
  // navigation stays intact and a missing isCustom (older WASM build) still
  // renders flat. The Korean labels match the rest of the BuilderView copy.
  const baseFields = fields.filter((f) => !f.isCustom);
  const customFields = fields.filter((f) => f.isCustom);
  const selected = fields.find((f) => f.path === predicate.field);
  const isCustomSelected = selected?.isCustom === true;

  return (
    <div
      className={`predicate-row${isCustomSelected ? " predicate-row-custom" : ""}`}
    >
      <select
        className="pr-field"
        value={predicate.field}
        onChange={(e) => handleField(e.target.value)}
      >
        <option value="">— field —</option>
        {customFields.length === 0 ? (
          fields.map((f) => (
            <option key={f.path} value={f.path}>
              {f.label ? `${f.label} (${f.path})` : f.path}
            </option>
          ))
        ) : (
          <>
            <optgroup label="기본 필드 (calldata)">
              {baseFields.map((f) => (
                <option key={f.path} value={f.path}>
                  {f.label ? `${f.label} (${f.path})` : f.path}
                </option>
              ))}
            </optgroup>
            <optgroup label="커스텀 필드 (manifest enrichment)">
              {customFields.map((f) => (
                <option key={f.path} value={f.path}>
                  {f.label ? `${f.label} (${f.path})` : f.path}
                </option>
              ))}
            </optgroup>
          </>
        )}
      </select>

      <select
        className="pr-op"
        value={predicate.op}
        onChange={(e) => handleOp(e.target.value)}
        disabled={operators.length === 0}
      >
        {operators.length === 0 ? <option value="">—</option> : null}
        {operators.map((o) => (
          <option key={o.id} value={o.id}>
            {o.label}
          </option>
        ))}
      </select>

      <ValueInput
        arity={op?.arity ?? "one"}
        value={predicate.value}
        onChange={(v) => onChange({ ...predicate, value: v })}
      />

      <button
        type="button"
        className="pr-remove"
        onClick={onRemove}
        title="조건 삭제"
        aria-label="조건 삭제"
      >
        ×
      </button>
    </div>
  );
}

function ValueInput({
  arity,
  value,
  onChange,
}: {
  arity: "one" | "many" | "none";
  value: string | string[] | null;
  onChange: (next: string | string[] | null) => void;
}) {
  if (arity === "none") {
    return (
      <div className="pr-value pr-value-none">(no operand)</div>
    );
  }
  if (arity === "many") {
    // Comma-separated entry. Empty string → empty array.
    const text = Array.isArray(value) ? value.join(", ") : "";
    return (
      <input
        className="pr-value"
        type="text"
        placeholder="comma, separated, values"
        value={text}
        onChange={(e) => {
          const raw = e.target.value;
          const arr = raw
            .split(",")
            .map((s) => s.trim())
            .filter((s) => s.length > 0);
          onChange(arr);
        }}
      />
    );
  }
  const text = typeof value === "string" ? value : "";
  return (
    <input
      className="pr-value"
      type="text"
      value={text}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

function arityToEmpty(arity: "one" | "many" | "none"): Predicate["value"] {
  if (arity === "none") return null;
  if (arity === "many") return [];
  return "";
}
