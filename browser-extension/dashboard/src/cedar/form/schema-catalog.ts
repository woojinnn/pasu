/**
 * Runtime accessor over the codegen'd Cedar field catalog
 * ({@link SCHEMA_CATALOG} in `schema-catalog.generated.ts`).
 *
 * The catalog is the schema's source of truth for: which leaf fields a given
 * action's `context` exposes, each field's Cedar type, and the exact `has`
 * presence-guards an optional field needs. The form picker
 * ({@link fieldsForTrigger}) and the IR builder ({@link formToIr}) both read it,
 * so the fields a policy offers — and the guards it emits — match the schema the
 * engine validates against.
 */

import type { FieldKind } from "../../editor-v9/gloss/paths";
import { SCHEMA_CATALOG, type RawField } from "./schema-catalog.generated";
import type { FormTrigger } from "./model";

/** A `<of> has <attr>` presence check for one optional step. */
export interface CatalogGuard {
  of: string;
  attr: string;
}

export interface CatalogField {
  /** Dotted path rooted at `context`. */
  path: string;
  /** Raw Cedar type ("String" | "Long" | "decimal" | "Bool" | "Set"). */
  cedarType: string;
  fieldKind: FieldKind;
  /** True when any step in the path is optional (needs `has` before use). */
  optional: boolean;
  /** Presence guards to AND before comparing — one per optional step. */
  guards: CatalogGuard[];
}

function kindOf(cedarType: string): FieldKind {
  switch (cedarType) {
    case "Long":
      return "primitive.Long";
    case "decimal":
      return "primitive.decimal";
    case "Bool":
      return "primitive.Bool";
    case "Set":
      return "collection";
    default:
      return "primitive.String";
  }
}

function expand(raw: RawField): CatalogField {
  const [path, cedarType, guards] = raw;
  return {
    path,
    cedarType,
    fieldKind: kindOf(cedarType),
    optional: !!guards && guards.length > 0,
    guards: (guards ?? []).map(([of, attr]) => ({ of, attr })),
  };
}

/** `{ entityType: "Amm::Action", id: "Swap" }` → catalog key `"Amm::Swap"`.
 *  `any` (and unknown actions) fall back to the `"*"` union key. */
export function catalogKey(trigger: FormTrigger): string {
  if (trigger.kind !== "actionEq") return "*";
  const ns = trigger.entityType.split("::")[0];
  const key = `${ns}::${trigger.id}`;
  return key in SCHEMA_CATALOG ? key : "*";
}

/** Every comparable leaf field for the trigger's action (or the any-action
 *  union). Empty only if the generated catalog is somehow missing. */
export function catalogFor(trigger: FormTrigger): CatalogField[] {
  return (SCHEMA_CATALOG[catalogKey(trigger)] ?? []).map(expand);
}

/** The presence guards a given path needs under the trigger's action. Empty for
 *  required fields, custom fields (guarded separately), or paths not in the
 *  catalog. */
export function guardsForPath(trigger: FormTrigger, path: string): CatalogGuard[] {
  const raw = SCHEMA_CATALOG[catalogKey(trigger)] ?? [];
  const hit = raw.find((r) => r[0] === path);
  return hit && hit[2] ? hit[2].map(([of, attr]) => ({ of, attr })) : [];
}

/** Whether a path is a known schema field for the trigger's action. */
export function isKnownPath(trigger: FormTrigger, path: string): boolean {
  const raw = SCHEMA_CATALOG[catalogKey(trigger)] ?? [];
  return raw.some((r) => r[0] === path);
}
