// Enriched-schema descriptor + lookup used to annotate `attr` nodes (type /
// source). Annotations are non-authoritative — they never affect EST round-trip
// (blocksToEst ignores them). Custom (manifest) fields live under
// `context.custom.*`; base fields elsewhere.

import type { Expr, SourceKind } from "./ir";

export interface SchemaField {
  path: string;
  type: string;
  fieldKind: string;
  source: "base" | "custom";
}

export interface SchemaDescriptor {
  [action: string]: SchemaField[];
}

/** Resolve the dotted path of an attr chain rooted at a Var, e.g.
 *  `context.custom.totalInputUsd`. Returns null if not rooted at a Var. */
export function attrPath(of: Expr, attr: string): string | null {
  const parts: string[] = [attr];
  let cur: Expr = of;
  while (cur.kind === "attr") {
    parts.unshift(cur.attr);
    cur = cur.of;
  }
  if (cur.kind === "var") {
    parts.unshift(cur.name);
    return parts.join(".");
  }
  return null;
}

/** Classify an attribute access by its path against the per-action fields.
 *  `context.custom.*` is custom by construction; otherwise resolve in the
 *  descriptor (base/custom + type), else unknown. */
export function classify(
  path: string | null,
  fields: SchemaField[] | null,
): { type?: string; source: SourceKind } {
  if (path && path.startsWith("context.custom.")) {
    const f = fields?.find((x) => x.path === path);
    return f ? { type: f.type, source: "custom" } : { source: "custom" };
  }
  const f = path ? (fields?.find((x) => x.path === path) ?? null) : null;
  return f ? { type: f.type, source: f.source } : { source: "unknown" };
}

// One entry of preview_custom_schema_json's `custom_types`
// (Rust CustomTypeDto { name: <action>, fields: Vec<CustomFieldSource> }).
export interface PreviewCustomType {
  name: string;
  fields: { field: string; cedar_type: string }[];
}

/** Build a SchemaDescriptor from the enriched-schema preview's custom types.
 *  Manifest fields land under `context.custom.<field>`. Base-field types are
 *  not introspectable via Cedar's Schema API, so base accesses resolve to
 *  `source: "unknown"` until a static base catalog is wired (follow-up).
 *  NOTE: the preview keys actions in snake_case; the caller must re-key by the
 *  same action id the policy scope uses (`action == Action::"X"`). */
export function descriptorFromCustomTypes(customTypes: PreviewCustomType[]): SchemaDescriptor {
  const out: SchemaDescriptor = {};
  for (const t of customTypes) {
    out[t.name] = t.fields.map((f) => ({
      path: `context.custom.${f.field}`,
      type: f.cedar_type,
      fieldKind: f.cedar_type.startsWith("Set<") ? "collection" : "primitive",
      source: "custom" as const,
    }));
  }
  return out;
}
