import { useMemo } from "react";
import type { FieldDto, OperatorDto, Predicate } from "../policy/types";
import { normalizeDecimalForDisplay } from "../policy/normalize-decimal";
import { normalizeLongForDisplay } from "../policy/normalize-long";
import {
  isEvmAddressField,
  normalizeAddressForDisplay,
} from "../policy/normalize-address";
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
  const tree = useMemo(() => buildPathTree(fields), [fields]);
  const field = fields.find((f) => f.path === predicate.field);
  const operators: OperatorDto[] = field?.operators ?? [];
  const op = operators.find((o) => o.id === predicate.op);
  const isCustomSelected = field?.isCustom === true;

  const handleFieldPath = (path: string) => {
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

  return (
    <div
      className={`predicate-row${isCustomSelected ? " predicate-row-custom" : ""}`}
    >
      <CascadingFieldPicker
        tree={tree}
        selectedPath={predicate.field}
        onSelect={handleFieldPath}
      />

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
        field={field}
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

// ── Cascading field picker ──────────────────────────────────────────────────

interface FieldNode {
  segment: string;
  field?: FieldDto;
  // Insertion-ordered map of child segment name -> child node. We rely on
  // insertion order (== schema declaration order) so the dropdowns surface
  // fields in the same order swap.rs declares them, not alphabetically.
  children: Map<string, FieldNode>;
  isCustom: boolean;
}

/**
 * Paths the builder doesn't surface in the cascading picker. The raw
 * `inputToken.amount.value` / `outputToken.amount.value` Strings are
 * hidden because the same `Input → Amount → Value` cascade slot now
 * routes to `inputAmountNano` / `outputAmountNano` (Long, scale=9) via
 * `CASCADE_DISPLAY_OVERRIDE` below. Hiding the raw wei field keeps the
 * slot unambiguous instead of showing both String and Long siblings
 * with diverging operator sets.
 *
 * Fields still exist in the action schema (so CodeView can reference
 * them if a power user explicitly types the raw path), but they don't
 * appear in any builder dropdown.
 */
const HIDDEN_FROM_BUILDER_PATHS: ReadonlySet<string> = new Set([
  "inputToken.amount.value",
  "outputToken.amount.value",
]);

/**
 * Map of `realPath` → `displayPath` for fields the builder pretends live
 * elsewhere in the cascade tree than their actual Cedar location.
 *
 * `inputAmountNano` is a top-level custom field
 * (`context.custom.inputAmountNano`), but users naturally look for "amount"
 * policies under `Input → Amount → ...`, not in a separate top-level
 * entry. Reparenting it to `inputToken.amount.value` puts it where the
 * intuition leads while leaving the real Cedar emit path untouched — the
 * predicate still references `inputAmountNano` and emits
 * `context.custom.inputAmountNano <op> <scaled-long>`.
 */
const CASCADE_DISPLAY_OVERRIDE: Record<string, string> = {
  inputAmountNano: "inputToken.amount.value",
  outputAmountNano: "outputToken.amount.value",
};

function toDisplayPath(realPath: string): string {
  return CASCADE_DISPLAY_OVERRIDE[realPath] ?? realPath;
}

function buildPathTree(fields: FieldDto[]): FieldNode {
  const root: FieldNode = {
    segment: "",
    children: new Map(),
    isCustom: false,
  };
  for (const f of fields) {
    if (HIDDEN_FROM_BUILDER_PATHS.has(f.path)) continue;
    // Walk by the *display* path so reparented fields (e.g.
    // inputAmountNano → inputToken.amount.value) land where the user
    // expects them. The leaf still carries the original FieldDto, so
    // `predicate.field` stays on the real Cedar path when this leaf is
    // chosen.
    const displayPath = toDisplayPath(f.path);
    const segs = displayPath.split(".");
    let node = root;
    for (let i = 0; i < segs.length; i++) {
      const seg = segs[i];
      let child = node.children.get(seg);
      if (!child) {
        // Provisional value; fixed by the post-pass below once the full
        // tree shape is known. Picking it from the first field we happen
        // to land on is wrong when the iteration order interleaves
        // custom and base children of the same parent (e.g. the
        // reparented inputAmountNano sorts before inputToken.* in the
        // BTreeMap-ordered field list).
        child = {
          segment: seg,
          children: new Map(),
          isCustom: f.isCustom === true,
        };
        node.children.set(seg, child);
      }
      node = child;
    }
    node.field = f;
  }
  // Repair top-level `isCustom` so the optgroup split reflects the actual
  // subtree contents. A node belongs to the "커스텀 필드" group only when
  // every leaf under it is manifest-enriched; mixed subtrees (e.g.
  // `inputToken`, which has base leaves *and* the reparented
  // inputAmountNano) stay in "기본 필드".
  //
  // Deeper levels render without optgroup, so they're left as the
  // first-seen flag for now — leaves consult `field.isCustom` directly
  // for row styling, so accuracy at non-leaf interior levels doesn't
  // affect anything user-visible.
  for (const top of root.children.values()) {
    top.isCustom = subtreeIsAllCustom(top);
  }
  return root;
}

function subtreeIsAllCustom(node: FieldNode): boolean {
  if (node.field) {
    return node.field.isCustom === true;
  }
  for (const child of node.children.values()) {
    if (!subtreeIsAllCustom(child)) return false;
  }
  return true;
}

function CascadingFieldPicker({
  tree,
  selectedPath,
  onSelect,
}: {
  tree: FieldNode;
  selectedPath: string;
  onSelect: (path: string) => void;
}) {
  // The tree is keyed by display segments, but the caller hands us the
  // real predicate.field path. Translate up front so cascade navigation
  // and the tree share a coordinate system.
  const segments = selectedPath ? toDisplayPath(selectedPath).split(".") : [];

  // Each iteration renders one select. We descend the tree using the
  // currently-selected segments; once we reach a leaf (the chosen node has no
  // children), we stop.
  const selects: Array<{
    depth: number;
    node: FieldNode; // parent whose children populate this select
    currentSeg: string;
  }> = [];
  let cursor: FieldNode | undefined = tree;
  let depth = 0;
  while (cursor && cursor.children.size > 0) {
    const currentSeg = segments[depth] ?? "";
    selects.push({ depth, node: cursor, currentSeg });
    cursor = cursor.children.get(currentSeg);
    depth += 1;
  }

  return (
    <div className="pr-field-group">
      {selects.map(({ depth: d, node, currentSeg }) => {
        const options = Array.from(node.children.values());
        return (
          <select
            key={d}
            className="pr-field"
            value={currentSeg}
            onChange={(e) =>
              onSelect(buildPathOn(tree, segments, d, e.target.value))
            }
            title={currentSeg}
          >
            {currentSeg === "" ? (
              <option value="" disabled>
                — field —
              </option>
            ) : null}
            {d === 0
              ? renderTopLevelOptions(options)
              : options.map((o) => (
                  <option key={o.segment} value={o.segment} title={o.segment}>
                    {labelFor(o)}
                  </option>
                ))}
          </select>
        );
      })}
    </div>
  );
}

/// Top-level select splits children into base vs custom optgroups so the
/// "manifest enrichment depends on a `has` guard" distinction stays visible.
/// Deeper selects share a single parent, so no grouping is meaningful there.
function renderTopLevelOptions(options: FieldNode[]) {
  const base = options.filter((o) => !o.isCustom);
  const custom = options.filter((o) => o.isCustom);
  if (custom.length === 0) {
    return options.map((o) => (
      <option key={o.segment} value={o.segment} title={o.segment}>
        {labelFor(o)}
      </option>
    ));
  }
  return (
    <>
      <optgroup label="기본 필드 (calldata)">
        {base.map((o) => (
          <option key={o.segment} value={o.segment} title={o.segment}>
            {labelFor(o)}
          </option>
        ))}
      </optgroup>
      <optgroup label="커스텀 필드 (manifest enrichment)">
        {custom.map((o) => (
          <option key={o.segment} value={o.segment} title={o.segment}>
            {labelFor(o)}
          </option>
        ))}
      </optgroup>
    </>
  );
}

/// Rebuild a full leaf path given a new selection at `changedDepth`. We try
/// to preserve the sub-tree the user had selected (so e.g. switching `Input →
/// Token → Type` to `Output` lands on `Output → Token → Type` when that path
/// exists), and fall back to drilling into the first child until we hit a
/// leaf when the old subtree doesn't exist under the new parent.
///
/// `oldSegments` and the returned segments live in *display* coordinates
/// (the tree's keying); we convert back to the real Cedar path at the end
/// because callers store the result in `predicate.field`, which is the
/// schema's authoritative path.
function buildPathOn(
  tree: FieldNode,
  oldSegments: string[],
  changedDepth: number,
  newValue: string,
): string {
  const out: string[] = [...oldSegments.slice(0, changedDepth), newValue];
  let cursor: FieldNode | undefined = tree;
  for (const s of out) {
    cursor = cursor?.children.get(s);
    if (!cursor) return out.join(".");
  }

  // Walk old tail down the new node while it still matches.
  const oldTail = oldSegments.slice(changedDepth + 1);
  for (const s of oldTail) {
    const next: FieldNode | undefined = cursor?.children.get(s);
    if (!next) break;
    cursor = next;
    out.push(s);
  }

  // Still not on a leaf? Descend first child until we are.
  while (cursor && !cursor.field && cursor.children.size > 0) {
    const first = cursor.children.values().next().value as FieldNode;
    out.push(first.segment);
    cursor = first;
  }

  // At a leaf the FieldDto's real path is authoritative (it differs from
  // the joined display segments when the field is reparented).
  if (cursor?.field) {
    return cursor.field.path;
  }
  return out.join(".");
}

/// Segment-name → display label. Used for both intermediate nodes
/// (`inputToken` → "Input") and leaves (`kind` → "Type"). The dotted-path
/// segment retains its canonical name in the `title` attribute on each
/// `<option>` so power users hovering still see the raw path.
const SEGMENT_LABELS: Record<string, string> = {
  // top-level base
  swapMode: "Swap direction",
  recipient: "Recipient",
  feeBps: "Fee (bps)",
  inputToken: "Input",
  outputToken: "Output",
  validity: "Validity",

  // top-level custom
  inputAmountNano: "Input amount (token-native)",
  outputAmountNano: "Output amount (token-native)",
  effectiveRateVsOracleBps: "Slippage vs oracle (bps)",
  recipientIsContract: "Recipient is a contract",
  totalInputFractionOfPortfolioBps: "Input ÷ portfolio (bps)",
  validityDeltaSec: "Time to deadline (sec)",
  totalInputUsd: "Input value (USD)",
  totalMinOutputUsd: "Min output value (USD)",
  windowStats: "24h stats",

  // intermediates
  asset: "Token",
  amount: "Amount",

  // leaves
  kind: "Type",
  address: "Contract address",
  tokenId: "NFT ID",
  symbol: "Symbol",
  decimals: "Decimals",
  value: "Value",
  expiresAt: "Deadline",
  source: "Source",
  staleSec: "Staleness (sec)",
  asOfTs: "Oracle timestamp",
  sources: "Oracle sources",
  swapVolumeUsd24h: "Volume (USD)",
  swapCount24h: "Count",
};

function labelFor(node: FieldNode): string {
  return SEGMENT_LABELS[node.segment] ?? node.segment;
}

// ── Value input + per-field placeholder ─────────────────────────────────────

function ValueInput({
  arity,
  value,
  field,
  onChange,
}: {
  arity: "one" | "many" | "none";
  value: string | string[] | null;
  field?: FieldDto;
  onChange: (next: string | string[] | null) => void;
}) {
  if (arity === "none") {
    return <div className="pr-value pr-value-none">(no operand)</div>;
  }

  // Enum-constrained fields render as <select>/checkbox group. The WASM
  // validator also rejects out-of-set literals as `disallowed_value`, so
  // this is a UX affordance — the safety net is in the validator.
  if (field?.allowedValues && field.allowedValues.length > 0) {
    if (arity === "many") {
      const selected = Array.isArray(value) ? value : [];
      const toggle = (v: string) => {
        const next = selected.includes(v)
          ? selected.filter((s) => s !== v)
          : [...selected, v];
        onChange(next);
      };
      return (
        <div className="pr-value pr-value-checkboxes">
          {field.allowedValues.map((v) => (
            <label key={v} className="pr-checkbox">
              <input
                type="checkbox"
                checked={selected.includes(v)}
                onChange={() => toggle(v)}
              />
              <span>{v}</span>
            </label>
          ))}
        </div>
      );
    }
    const current = typeof value === "string" ? value : "";
    return (
      <select
        className="pr-value"
        value={current}
        onChange={(e) => onChange(e.target.value)}
      >
        {current === "" ? <option value="">— select —</option> : null}
        {field.allowedValues.map((v) => (
          <option key={v} value={v}>
            {v}
          </option>
        ))}
      </select>
    );
  }

  const placeholder = field ? placeholderFor(field, arity) : "";

  if (arity === "many") {
    const text = Array.isArray(value) ? value.join(", ") : "";
    return (
      <input
        className="pr-value"
        type="text"
        placeholder={placeholder}
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
  // Per-type display normalizers fire on blur — keystroke-time would
  // eat in-progress input (typing `1.` would snap to `1.0`, prepending
  // `0x` mid-paste, etc.). All three families share the same lift:
  // strip near-miss shapes (`100.0` → `100`, `1` → `1.0`, `abc…` → `0xabc…`)
  // so the user sees the canonical form before Compile.
  const normalizeForBlur = pickOneArityNormalizer(field);
  return (
    <input
      className="pr-value"
      type="text"
      placeholder={placeholder}
      value={text}
      onChange={(e) => onChange(e.target.value)}
      onBlur={
        normalizeForBlur
          ? (e) => {
              const normalized = normalizeForBlur(e.target.value);
              if (normalized !== e.target.value) onChange(normalized);
            }
          : undefined
      }
    />
  );
}

/**
 * Decide which display normalizer (if any) the single-operand input
 * should run on blur, based on the field's Cedar type and pattern.
 *   - decimal type            → integer ↔ fractional shape lift
 *   - long type               → drop `.0+` fractional zeros
 *   - EVM address pattern     → prepend `0x`, fold uppercase prefix
 *   - other (incl. scaled
 *     long, generic string)   → no normalization (raw value passed
 *                              through as-is; downstream validators
 *                              report on the user's literal text)
 *
 * Returns `undefined` to mean "don't attach an onBlur handler at all"
 * so React doesn't pay the listener cost on every keystroke for
 * fields that wouldn't benefit.
 */
function pickOneArityNormalizer(
  field: FieldDto | undefined,
): ((raw: string) => string) | undefined {
  if (!field) return undefined;
  if (field.type === "decimal") return normalizeDecimalForDisplay;
  // Scaled longs (e.g. `inputAmountNano`) already accept decimal
  // shapes via `scale_decimal_to_long` — skip the integer normalizer
  // so the user keeps fractional precision.
  if (field.type === "long" && (field.scale === undefined || field.scale === null)) {
    return normalizeLongForDisplay;
  }
  if (field.type === "string" && isEvmAddressField(field.pattern)) {
    return normalizeAddressForDisplay;
  }
  return undefined;
}

/// Format hint shown as the `<input>` placeholder. Disappears on focus,
/// reappears when empty. Path-specific hints take precedence over generic
/// type-based ones so users see the most specific cue available (e.g. wei
/// units on amount fields beats "integer" from the Long type).
function placeholderFor(
  field: FieldDto,
  arity: "one" | "many" | "none",
): string {
  if (arity === "none") return "";
  const p = field.path;
  const t = field.type;

  if (arity === "many") {
    if (p === "recipient" || p.endsWith(".asset.address"))
      return "0x..., 0x..., 0x...";
    if (p.endsWith(".asset.symbol")) return "USDC, WETH, ...";
    if (p.endsWith(".sources")) return "chainlink, pyth, ...";
    return "value, value, value";
  }

  // Path-specific (single operand)
  if (p === "recipient") return "0x... (40 hex)";
  if (p.endsWith(".asset.address")) return "0x... (contract)";
  if (p.endsWith(".asset.symbol")) return "USDC, WETH, ...";
  if (p.endsWith(".asset.tokenId")) return "NFT ID (e.g. 1234)";
  if (p.endsWith(".asset.decimals")) return "0–30";
  if (p.endsWith(".amount.value")) return "wei (e.g. 1000000000000000000)";
  if (p === "feeBps") return "bps (0–10000)";
  if (p === "validity.expiresAt") return "unix seconds";
  if (p.endsWith(".asOfTs")) return "unix seconds";
  if (p.endsWith(".staleSec") || p === "validityDeltaSec") return "seconds";
  if (p.endsWith("Bps")) return "bps (integer)";
  if (p === "windowStats.swapCount24h") return "count";

  // Scaled-Long fields (token-native amounts): user types the DEX-UI value.
  // The WASM compiler rescales by 10^scale into the underlying Long literal.
  if (field.scale !== undefined && field.scale !== null) {
    return "e.g. 0.5 (token-native, any token)";
  }

  // Type-based fallback
  if (t === "decimal") return "USD (e.g. 1000.00)";
  if (t === "long") return "integer";
  return "";
}

function arityToEmpty(arity: "one" | "many" | "none"): Predicate["value"] {
  if (arity === "none") return null;
  if (arity === "many") return [];
  return "";
}
