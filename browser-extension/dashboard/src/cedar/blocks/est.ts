// Minimal EST (Cedar JSON policy format) shapes + helpers. EST is navigated by
// key; these keep the converter readable. Shapes confirmed against real
// `Policy::to_json()` output (see __tests__/fixtures/est-corpus.json).

export type EstExpr = Record<string, any>;

export interface EstClause {
  kind: "when" | "unless";
  body: EstExpr;
}

export interface EstPolicy {
  effect: "permit" | "forbid";
  principal: Record<string, any>;
  action: Record<string, any>;
  resource: Record<string, any>;
  conditions: EstClause[];
  annotations?: Record<string, string>;
}

/** The single operator key of an EST expr node (e.g. ">", ".", "&&"), or null
 *  for multi/zero-key nodes (`Value`/`Var` are single-key and handled directly). */
export function opKey(node: EstExpr): string | null {
  const keys = Object.keys(node);
  return keys.length === 1 ? keys[0] : null;
}
