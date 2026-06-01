import { useMemo, useState, type DragEvent } from "react";

import { setDragState } from "../canvas/Canvas";
import { displayParam, fieldKindOf, glossByRole, isSupportedParam, ROLES, type Role } from "../schema";
import type { EditorAction } from "../reducer";

/**
 * Left-side palette — the 35-field V7_GLOSS catalog grouped by role.
 *
 * Click drops the field as a floating draft (asDraft + parentId=null),
 * which the user can then connect to a logic block via the canvas →
 * inspector "parent" picker (Phase 5). Search filters across both ko
 * and en labels + the dotted param name.
 *
 * Unsupported params (outside our policy-schema.json) stay in the list
 * but render with a warning tag so the user knows they'll fail the
 * compile gate. Drop is still allowed — Phase-7 builder template auth
 * needs unsupported fields to surface in the doc tree so we don't
 * silently lose intent.
 */
export interface PaletteProps {
  dispatch: (a: EditorAction) => void;
  locale: "ko" | "en";
}

const ROLE_ORDER: Role[] = ["numeric", "address", "ref", "enum", "auth", "derived", "misc"];

export function Palette({ dispatch, locale }: PaletteProps) {
  const [query, setQuery] = useState("");
  const [collapsed, setCollapsed] = useState<Set<Role>>(new Set());
  const grouped = useMemo(() => glossByRole(), []);

  const matchesQuery = (param: string): boolean => {
    if (!query.trim()) return true;
    const q = query.trim().toLowerCase();
    if (param.toLowerCase().includes(q)) return true;
    const display = displayParam(param, locale).toLowerCase();
    if (display.includes(q)) return true;
    const altLocale = locale === "ko" ? "en" : "ko";
    const altDisplay = displayParam(param, altLocale).toLowerCase();
    return altDisplay.includes(q);
  };

  const toggleRole = (role: Role) => {
    setCollapsed((cur) => {
      const next = new Set(cur);
      if (next.has(role)) next.delete(role);
      else next.add(role);
      return next;
    });
  };

  const dropAsDraft = (param: string) => {
    const fk = fieldKindOf(param);
    dispatch({
      type: "ADD_PREDICATE",
      param,
      cfg: {
        fk,
        op: defaultOpFor(fk),
        x: 120 + Math.random() * 200,
        y: 600 + Math.random() * 100,
      },
      asDraft: true,
    });
  };

  const onPaletteDragStart = (e: DragEvent<HTMLButtonElement>, param: string) => {
    const fk = fieldKindOf(param);
    // Mark the drag as a palette-origin so canvas drop zones know to
    // synthesize an ADD_PREDICATE instead of a node move.
    setDragState({ nodeId: `palette:${param}`, fromPalette: { param, fk } });
    e.dataTransfer.effectAllowed = "copy";
    try { e.dataTransfer.setData("text/plain", `palette:${param}`); } catch { /* noop */ }
  };

  return (
    <aside className="v7-palette">
      <div className="palette-head">
        <h3>필드 팔레트</h3>
        <span className="hint">{Object.values(grouped).reduce((a, b) => a + b.length, 0)}개</span>
      </div>
      <input
        type="search"
        className="palette-search"
        placeholder="검색 (param · 한글 · english)"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />

      <div className="palette-actions">
        <button onClick={() => dispatch({ type: "ADD_LOGIC", op: "AND", cfg: {} })}>+ AND</button>
        <button onClick={() => dispatch({ type: "ADD_LOGIC", op: "OR", cfg: {} })}>+ OR</button>
        <button onClick={() => dispatch({ type: "ADD_LOGIC", op: "NOT", cfg: {} })}>+ NOT</button>
      </div>

      <div className="palette-groups">
        {ROLE_ORDER.map((role) => {
          const entries = grouped[role].filter((e) => matchesQuery(e.param));
          if (entries.length === 0) return null;
          const isCollapsed = collapsed.has(role);
          const meta = ROLES[role];
          return (
            <section key={role} className={`palette-group tone-${meta.tone}`}>
              <button className="group-head" onClick={() => toggleRole(role)}>
                <span className="caret">{isCollapsed ? "▸" : "▾"}</span>
                <span className="group-label">{locale === "ko" ? meta.ko : meta.en}</span>
                <span className="group-count">{entries.length}</span>
              </button>
              {!isCollapsed && (
                <div className="group-list">
                  {entries.map((e) => (
                    <button
                      key={e.param}
                      className="palette-item"
                      onClick={() => dropAsDraft(e.param)}
                      onDragStart={(ev) => onPaletteDragStart(ev, e.param)}
                      onDragEnd={() => setDragState(null)}
                      draggable
                      title={`${e.param} — ${locale === "ko" ? e.entry.desc.ko : e.entry.desc.en}`}
                    >
                      <span className="pi-label">{locale === "ko" ? e.entry.ko : e.entry.en}</span>
                      {!isSupportedParam(e.param) && (
                        <span className="pi-warn" title="schema 미등록">⚠</span>
                      )}
                    </button>
                  ))}
                </div>
              )}
            </section>
          );
        })}
      </div>
    </aside>
  );
}

function defaultOpFor(fk: ReturnType<typeof fieldKindOf>) {
  switch (fk) {
    case "primitive.Bool": return "isTrue" as const;
    case "primitive.Long":
    case "primitive.decimal": return "lt" as const;
    case "collection": return "contains" as const;
    case "record": return "eq" as const;
    default: return "eq" as const;
  }
}
