/**
 * Sidebar that lists every parameter (hole) currently in the workspace.
 *
 * Reads `extractParams(policyIR)` from cedar/blocks and renders one row per
 * spec. Each row shows: name, label, type hint, expected shape, optional
 * flag. Clicking a row scrolls Blockly to the block (useful when the canvas
 * is large) — wires through the `onJump` prop so the parent can centerOnBlock.
 *
 * Phase E intentionally read-only on the metadata: editing label / type /
 * constraints lives on the hole block itself (LABEL / TYPE fields are
 * already editable; constraints are author-set via Phase E.next or directly
 * in block.data). The sidebar is the inventory + nav aid, not the edit form.
 *
 * Scales: for N>50 holes, the list will scroll; for N>500, virtualisation
 * via @tanstack/react-virtual is a small follow-up (deps already present).
 */

import { useTranslation } from "react-i18next";

import type { ParamSpec, PolicyIR } from "../../cedar/blocks";
import { extractParams } from "../../cedar/blocks";

export interface ParamSidebarProps {
  policy: PolicyIR | null;
  /** Called with a hole `name` when the user clicks a row. The host scrolls
   *  to the corresponding block. */
  onJump?: (name: string) => void;
}

export function ParamSidebar({ policy, onJump }: ParamSidebarProps) {
  const { t } = useTranslation("blocks");

  if (!policy) {
    return (
      <div style={paneStyle}>
        <div style={headerStyle}>{t("sidebar.title")}</div>
        <div style={emptyStyle}>{t("sidebar.emptyWorkspace")}</div>
      </div>
    );
  }

  let specs: ParamSpec[] = [];
  let extractError: string | null = null;
  try {
    specs = extractParams(policy);
  } catch (e) {
    extractError = e instanceof Error ? e.message : String(e);
  }

  return (
    <div style={paneStyle}>
      <div style={headerStyle}>
        {t("sidebar.title")}
        <span style={countStyle}>{specs.length}</span>
      </div>
      {extractError && (
        <div style={errorStyle}>⚠ {extractError}</div>
      )}
      {!extractError && specs.length === 0 && (
        <div style={emptyStyle}>{t("sidebar.emptyHint")}</div>
      )}
      <ul style={listStyle}>
        {specs.map((s) => (
          <li
            key={s.name}
            style={rowStyle}
            onClick={() => onJump?.(s.name)}
            title={t("sidebar.rowTitle")}
          >
            <div style={{ display: "flex", gap: 6, alignItems: "baseline" }}>
              <span style={nameStyle}>{s.name}</span>
              {s.optional && <span style={chipStyle}>optional</span>}
            </div>
            <div style={metaStyle}>
              <span>{s.label ? s.label : <em style={dimStyle}>{t("sidebar.noLabel")}</em>}</span>
              {s.type && <span style={typeChipStyle}>{s.type}</span>}
              <span style={dimStyle}>{t(`sidebar.expected.${EXPECTED_KEY[s.expected]}`)}</span>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}

const EXPECTED_KEY: Record<ParamSpec["expected"], string> = {
  "lit:long": "long",
  "lit:string": "string",
  "lit:bool": "bool",
  litEntity: "entity",
  set: "set",
  attr: "attr",
};

const paneStyle: React.CSSProperties = {
  width: 240,
  borderLeft: "1px solid var(--hairline-soft, #E5E6E3)",
  background: "var(--fog-100, #fcfcfc)",
  display: "flex",
  flexDirection: "column",
  overflow: "hidden",
};

const headerStyle: React.CSSProperties = {
  padding: "8px 12px",
  fontSize: 12,
  fontWeight: 600,
  color: "var(--slate-700, #334155)",
  borderBottom: "1px solid var(--hairline-soft, #E5E6E3)",
  display: "flex",
  gap: 6,
  alignItems: "center",
};

const countStyle: React.CSSProperties = {
  marginLeft: "auto",
  fontSize: 11,
  fontWeight: 400,
  color: "var(--slate-500, #475569)",
};

const listStyle: React.CSSProperties = {
  listStyle: "none",
  padding: 0,
  margin: 0,
  overflow: "auto",
  flex: 1,
};

const rowStyle: React.CSSProperties = {
  padding: "8px 12px",
  borderBottom: "1px solid var(--hairline-soft, #E5E6E3)",
  cursor: "pointer",
  display: "flex",
  flexDirection: "column",
  gap: 4,
};

const nameStyle: React.CSSProperties = {
  fontFamily: "var(--ff-mono, monospace)",
  fontWeight: 600,
  fontSize: 13,
};

const metaStyle: React.CSSProperties = {
  display: "flex",
  flexWrap: "wrap",
  gap: 6,
  alignItems: "center",
  fontSize: 11,
  color: "var(--slate-600, #475569)",
};

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

const dimStyle: React.CSSProperties = {
  color: "var(--slate-500, #475569)",
};

const errorStyle: React.CSSProperties = {
  padding: "8px 12px",
  fontSize: 12,
  color: "var(--fail-700, #7F4740)",
};

const emptyStyle: React.CSSProperties = {
  padding: "12px",
  fontSize: 12,
  color: "var(--slate-500, #475569)",
};
