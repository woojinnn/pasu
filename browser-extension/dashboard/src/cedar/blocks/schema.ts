// Enriched-schema descriptor used to annotate `attr` nodes (type / source).
// Types only here; the lookup functions (`attrPath`, `classify`) are added in
// the schema-annotation phase. Annotations are non-authoritative — they never
// affect EST round-trip.

export interface SchemaField {
  path: string;
  type: string;
  fieldKind: string;
  source: "base" | "custom";
}

export interface SchemaDescriptor {
  [action: string]: SchemaField[];
}
