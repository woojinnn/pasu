/**
 * Adopter fill panel — given a template PolicyIR (with holes), render an
 * auto-generated form, validate via `fillParams`, and emit the concrete IR.
 *
 * Renders one input per ParamSpec:
 *   lit:long    → number input
 *   lit:string  → text input
 *   lit:bool    → checkbox
 *   litEntity   → two text inputs (type + id)
 *   set         → textarea, comma-separated values; element type inferred
 *                 from the spec's default (string by default)
 *
 * Constraint hints are surfaced inline: min/max for numbers, enum for
 * either numbers or strings (renders as a `<select>` when present). Errors
 * from `fillParams` are shown next to the offending field by `name`.
 *
 * The panel is intentionally collapsed by default — most editing happens in
 * the blocks themselves; this is only useful when consuming a template.
 */

import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  extractParams,
  fillParams,
  type ParamError,
  type ParamFillValue,
  type ParamSpec,
  type PolicyIR,
} from "../../cedar/blocks";

export interface ParamFillPanelProps {
  template: PolicyIR | null;
  /** Called with the filled (hole-free) IR. Host typically pipes it through
   *  `blocksToText` to get Cedar text. */
  onFilled?: (filled: PolicyIR) => void;
}

export function ParamFillPanel({ template, onFilled }: ParamFillPanelProps) {
  const { t } = useTranslation("blocks");
  const specs = useMemo<ParamSpec[]>(() => {
    if (!template) return [];
    try {
      return extractParams(template);
    } catch {
      return [];
    }
  }, [template]);

  const [values, setValues] = useState<Record<string, ParamFillValue>>({});
  const [errors, setErrors] = useState<ParamError[]>([]);
  const [success, setSuccess] = useState<PolicyIR | null>(null);

  if (!template || specs.length === 0) return null;

  const setOne = (name: string, v: ParamFillValue) => {
    setValues((prev) => ({ ...prev, [name]: v }));
    setSuccess(null);
  };

  const onApply = () => {
    if (!template) return;
    const result = fillParams(template, values);
    if (result.ok) {
      setErrors([]);
      setSuccess(result.policy);
      onFilled?.(result.policy);
    } else {
      setErrors(result.errors);
      setSuccess(null);
    }
  };

  const errByName = new Map(errors.map((e) => [e.name, e]));

  return (
    <details
      open
      style={{
        background: "var(--fog-100, #fcfcfc)",
        borderTop: "1px solid var(--hairline-soft, #E5E6E3)",
      }}
    >
      <summary
        style={{
          padding: "6px 12px",
          cursor: "pointer",
          fontSize: 12,
          color: "var(--slate-500, #475569)",
        }}
      >
        {t("fill.summary", { count: specs.length })}{success ? " ✓" : ""}
      </summary>
      <div style={{ padding: "8px 12px", display: "flex", flexDirection: "column", gap: 8 }}>
        {specs.map((s) => (
          <ParamRow
            key={s.name}
            spec={s}
            value={values[s.name]}
            error={errByName.get(s.name)}
            onChange={(v) => setOne(s.name, v)}
          />
        ))}
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <button onClick={onApply} style={{ padding: "4px 12px", fontSize: 12 }}>
            {t("fill.apply")}
          </button>
          {success && (
            <span style={{ color: "var(--ok-700, #467a4a)", fontSize: 12 }}>
              ✓ {t("fill.success")}
            </span>
          )}
          {errors.length > 0 && (
            <span style={{ color: "var(--fail-700, #7F4740)", fontSize: 12 }}>
              ⚠ {t("fill.errorCount", { count: errors.length })}
            </span>
          )}
        </div>
      </div>
    </details>
  );
}

interface ParamRowProps {
  spec: ParamSpec;
  value: ParamFillValue | undefined;
  error?: ParamError;
  onChange: (v: ParamFillValue) => void;
}

function ParamRow({ spec, value, error, onChange }: ParamRowProps) {
  const { t } = useTranslation("blocks");
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
      <label style={{ fontSize: 12, fontWeight: 500, display: "flex", gap: 6, alignItems: "baseline" }}>
        <code style={{ fontFamily: "var(--ff-mono, monospace)" }}>{spec.name}</code>
        {spec.label && <span style={{ color: "var(--slate-500, #475569)" }}>— {spec.label}</span>}
        {spec.optional && <span style={chipStyle}>optional</span>}
        {spec.type && <span style={typeChipStyle}>{spec.type}</span>}
      </label>
      {renderInput(spec, value, onChange, t)}
      {error && <span style={{ fontSize: 11, color: "var(--fail-700, #7F4740)" }}>⚠ {error.message}</span>}
    </div>
  );
}

function renderInput(
  spec: ParamSpec,
  value: ParamFillValue | undefined,
  onChange: (v: ParamFillValue) => void,
  t: (key: string, opts?: Record<string, unknown>) => string,
) {
  const hasEnum = !!spec.constraints?.enum;
  switch (spec.expected) {
    case "lit:long": {
      if (hasEnum) {
        return (
          <select
            value={value === undefined ? "" : String(value)}
            onChange={(e) => onChange(Number(e.target.value))}
          >
            <option value="" disabled>
              {t("fill.select")}
            </option>
            {spec.constraints!.enum!.map((opt) => (
              <option key={String(opt)} value={String(opt)}>
                {String(opt)}
              </option>
            ))}
          </select>
        );
      }
      return (
        <input
          type="number"
          step={1}
          min={spec.constraints?.min}
          max={spec.constraints?.max}
          value={value === undefined ? "" : Number(value)}
          onChange={(e) => onChange(Number(e.target.value))}
          placeholder={t("fill.defaultPlaceholder", { value: defaultPreview(spec) })}
        />
      );
    }
    case "lit:string": {
      if (hasEnum) {
        return (
          <select
            value={value === undefined ? "" : String(value)}
            onChange={(e) => onChange(e.target.value)}
          >
            <option value="" disabled>
              {t("fill.select")}
            </option>
            {spec.constraints!.enum!.map((opt) => (
              <option key={String(opt)} value={String(opt)}>
                {String(opt)}
              </option>
            ))}
          </select>
        );
      }
      return (
        <input
          type="text"
          value={value === undefined ? "" : String(value)}
          onChange={(e) => onChange(e.target.value)}
          placeholder={t("fill.defaultPlaceholder", { value: defaultPreview(spec) })}
        />
      );
    }
    case "lit:bool":
      return (
        <input
          type="checkbox"
          checked={value === undefined ? defaultBool(spec) : Boolean(value)}
          onChange={(e) => onChange(e.target.checked)}
        />
      );
    case "litEntity": {
      const v = (value as { type: string; id: string } | undefined) ?? { type: "", id: "" };
      return (
        <div style={{ display: "flex", gap: 4 }}>
          <input
            type="text"
            placeholder={t("fill.entityTypePlaceholder")}
            value={v.type}
            onChange={(e) => onChange({ type: e.target.value, id: v.id })}
            style={{ flex: 1 }}
          />
          <input
            type="text"
            placeholder={t("fill.entityIdPlaceholder")}
            value={v.id}
            onChange={(e) => onChange({ type: v.type, id: e.target.value })}
            style={{ flex: 1 }}
          />
        </div>
      );
    }
    case "set": {
      const arr = Array.isArray(value) ? value : [];
      return (
        <textarea
          placeholder={t("fill.setPlaceholder")}
          value={arr.join(", ")}
          onChange={(e) => {
            const tokens = e.target.value
              .split(/[,\n]/)
              .map((t) => t.trim())
              .filter((t) => t.length > 0);
            onChange(tokens);
          }}
          rows={2}
          style={{ resize: "vertical", fontSize: 12 }}
        />
      );
    }
  }
}

function defaultBool(spec: ParamSpec): boolean {
  return spec.default.kind === "lit" && spec.default.litType === "bool"
    ? Boolean(spec.default.value)
    : false;
}

function defaultPreview(spec: ParamSpec): string {
  const d = spec.default;
  if (d.kind === "lit") return String(d.value);
  if (d.kind === "litEntity") return `${d.entity.type}::"${d.entity.id}"`;
  return "";
}

const chipStyle: React.CSSProperties = {
  fontSize: 10,
  padding: "1px 6px",
  background: "var(--fog-300, #f1f1ee)",
  borderRadius: 3,
};

const typeChipStyle: React.CSSProperties = {
  ...chipStyle,
  background: "var(--brand-50, #f0eafc)",
  color: "var(--brand-700, #6f4ac5)",
};
