/**
 * FieldCombobox — a searchable, grouped field picker for the form's condition
 * rows. Replaces the flat 35-item native <select> with: type-to-filter, category
 * grouping (role) with colour dots, a one-line description per field, a
 * type/unit chip, and a "보강" badge for enrichment fields. Pure local component,
 * no deps. Keyboard: type to filter, ↑/↓ to move, Enter to pick, Esc to close.
 */
import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";

import { ROLE_COLOUR, ROLE_LABEL_KO, type FieldKind, type Role } from "../../../editor-v9/gloss/paths";
import type { FieldOption } from "../../../cedar/form";

const ROLE_ORDER: Role[] = ["address", "ref", "numeric", "enum", "auth", "derived"];

function roleColor(role: Role): string {
  return `hsl(${ROLE_COLOUR[role]} 60% 52%)`;
}

/** The TYPE chip — one fixed vocabulary, independent of unit (Rule 2). The
 *  unit (USD / bp / 토큰 / …) is rendered as a separate pill, never here. */
function typeChip(f: FieldOption): string {
  const k: FieldKind = f.fieldKind;
  switch (k) {
    case "primitive.Bool":
      return "참/거짓";
    case "primitive.Long":
      return "숫자";
    case "primitive.decimal":
      return "소수";
    case "primitive.String":
      return f.role === "address" ? "주소" : "문자";
    case "ref":
      return "참조";
    case "collection":
      return "목록";
    case "record":
      return "레코드";
  }
}

export function FieldCombobox({
  value,
  fields,
  onChange,
}: {
  value: string;
  fields: FieldOption[];
  onChange: (path: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [active, setActive] = useState(0);
  const rootRef = useRef<HTMLDivElement>(null);

  const selected = fields.find((f) => f.path === value);
  const advancedCount = useMemo(() => fields.filter((f) => f.advanced).length, [fields]);

  // Flat filtered list (drives keyboard nav) + grouped view (drives render).
  // Engine-internal "advanced" fields stay hidden until the user expands them
  // or starts searching (a query searches everything).
  const flat = useMemo(() => {
    const needle = q.trim().toLowerCase();
    const match = (f: FieldOption) =>
      !needle ||
      f.label.toLowerCase().includes(needle) ||
      f.path.toLowerCase().includes(needle) ||
      (f.desc?.toLowerCase().includes(needle) ?? false);
    const kept = fields.filter(
      (f) => match(f) && (!f.advanced || showAdvanced || needle.length > 0 || f.path === value),
    );
    // Stable order: by role, then prominent-before-advanced, then original.
    return kept.sort(
      (a, b) =>
        ROLE_ORDER.indexOf(a.role) - ROLE_ORDER.indexOf(b.role) ||
        Number(a.advanced ?? false) - Number(b.advanced ?? false),
    );
  }, [fields, q, showAdvanced, value]);

  const groups = useMemo(() => {
    const m = new Map<Role, FieldOption[]>();
    for (const f of flat) {
      const arr = m.get(f.role) ?? [];
      arr.push(f);
      m.set(f.role, arr);
    }
    return ROLE_ORDER.filter((r) => m.has(r)).map((r) => ({ role: r, items: m.get(r)! }));
  }, [flat]);

  useEffect(() => setActive(0), [q, open, showAdvanced]);

  // Close on outside click.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const pick = (path: string) => {
    onChange(path);
    setOpen(false);
    setQ("");
  };

  const onKeyDown = (e: ReactKeyboardEvent) => {
    if (e.key === "Escape") return setOpen(false);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(i + 1, flat.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (flat[active]) pick(flat[active].path);
    }
  };

  let flatIdx = -1;
  return (
    <div className="fc" ref={rootRef}>
      <button type="button" className={`fc-btn${selected ? "" : " empty"}`} onClick={() => setOpen((o) => !o)}>
        {selected ? (
          <>
            <span className="fc-dot" style={{ background: roleColor(selected.role) }} />
            <span className="fc-btn-label">{selected.label}</span>
            {selected.source === "custom" && <span className="fc-badge">보강</span>}
          </>
        ) : (
          "필드 선택…"
        )}
        <span className="fc-caret">▾</span>
      </button>

      {open && (
        <div className="fc-pop">
          <input
            className="fc-search"
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="필드 검색…"
          />
          <div className="fc-list">
            {groups.length === 0 && <div className="fc-none">일치하는 필드 없음</div>}
            {groups.map((g) => (
              <div key={g.role} className="fc-group">
                <div className="fc-group-h">
                  <span className="fc-dot" style={{ background: roleColor(g.role) }} />
                  {ROLE_LABEL_KO[g.role]}
                </div>
                {g.items.map((f) => {
                  flatIdx += 1;
                  const idx = flatIdx;
                  return (
                    <button
                      type="button"
                      key={f.path}
                      className={`fc-opt${idx === active ? " active" : ""}${f.path === value ? " sel" : ""}`}
                      onMouseEnter={() => setActive(idx)}
                      onClick={() => pick(f.path)}
                    >
                      <div className="fc-opt-top">
                        <span className="fc-opt-label">{f.label}</span>
                        {f.source === "custom" && <span className="fc-badge">보강</span>}
                        {f.unit && <span className="fc-unit">{f.unit}</span>}
                        <span className="fc-chip">{typeChip(f)}</span>
                      </div>
                      {f.desc && <div className="fc-opt-desc">{f.desc}</div>}
                    </button>
                  );
                })}
              </div>
            ))}
            {advancedCount > 0 && !q.trim() && (
              <button
                type="button"
                className="fc-adv"
                onClick={() => setShowAdvanced((s) => !s)}
              >
                {showAdvanced ? "▾ 고급 필드 숨기기" : `▸ 고급 필드 보기 (${advancedCount})`}
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
