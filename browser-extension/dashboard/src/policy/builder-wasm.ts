import init, {
  compile_policy_json,
  compile_policy_with_overlay_json,
  parse_cedar_json,
  list_actions,
  get_action_schema_json,
  get_action_schema_with_overlay_json,
  get_typed_paths_for_action_json,
} from "../wasm/policy_builder_wasm.js";
import type {
  ActionSchemaDto,
  Envelope,
  PolicyRule,
} from "./types";

/**
 * One overlay entry the builder injects on top of the bundled static
 * schema. `cedarType` mirrors the upstream alias-table spelling — both
 * scalar primitives (`"Long"`, `"String"`, `"Bool"`, `"decimal"`,
 * `"Set<String>"`, `"Set<Long>"`) and record aliases (`"UsdValuation"`,
 * `"WindowStats"`, `"Validity"`, …) are accepted; the Rust overlay
 * expands record entries into their per-leaf `FieldSpec`s via
 * `policy_builder::aliases::record_leaves`. Unknown spellings (e.g. a
 * new alias the builder hasn't been taught about yet) are dropped at
 * the TS boundary by `OVERLAY_KNOWN_TYPES` in `manifest-overlay.ts`.
 */
export interface OverlayField {
  field: string;
  cedarType: string;
}

let initPromise: Promise<unknown> | null = null;
async function ensureReady(): Promise<void> {
  if (!initPromise) initPromise = init();
  await initPromise;
}

export interface CompileResult {
  cedarText?: string;
  error?: { kind?: string; message?: string };
}

export async function compileRule(
  rule: PolicyRule,
  overlay?: readonly OverlayField[],
): Promise<CompileResult> {
  await ensureReady();
  // Mirrors `fetchActionSchema` — when no overlay is provided, take the
  // no-overlay path so callers that ignore overlay (tests, code-view
  // round-trip) don't pay the wrapping cost. When an overlay IS in play
  // the builder MUST also pass it here; otherwise the rule the user just
  // built against an overlay field would fail with `unknown_field`.
  const raw =
    overlay && overlay.length > 0
      ? compile_policy_with_overlay_json(
          JSON.stringify({ action: rule.action, rule, overlay }),
        )
      : compile_policy_json(JSON.stringify(rule));
  const env = JSON.parse(raw) as Envelope<{ cedar_text: string }>;
  if (!env.ok || !env.data) return { error: env.error ?? {} };
  return { cedarText: env.data.cedar_text };
}

export interface ParseResult {
  rule?: PolicyRule;
  error?: { kind?: string; message?: string };
}

export async function parseCedar(text: string): Promise<ParseResult> {
  await ensureReady();
  const raw = parse_cedar_json(text);
  const env = JSON.parse(raw) as Envelope<PolicyRule>;
  if (!env.ok || !env.data) return { error: env.error ?? {} };
  return { rule: env.data };
}

export async function fetchActions(): Promise<string[]> {
  await ensureReady();
  const raw = list_actions();
  const env = JSON.parse(raw) as Envelope<string[]>;
  return env.data ?? [];
}

export interface SchemaResult {
  schema?: ActionSchemaDto;
  error?: { kind?: string; message?: string };
}

/**
 * Phase 8.5 / PR 4: type-tagged selector paths for the manifest
 * editor. Each scalar entry pairs a fully-qualified selector string
 * (`$.root.chain_id`) with its Cedar primitive (`long`, `string`, …);
 * each record entry pairs a composite path (`$.action.inputToken.asset`)
 * with the Cedar alias it resolves to (`AssetRef`, `AmountConstraint`,
 * `Validity`, …). The SelectorPicker uses these to filter the
 * dropdown by the param's declared type so a `Long` slot never
 * surfaces String paths.
 */
export interface TypedPathScalar {
  path: string;
  cedarType: "long" | "string" | "bool" | "decimal" | "set_of_string" | "set_of_long";
}
export interface TypedPathRecord {
  path: string;
  cedarAlias: string;
}
export interface TypedPaths {
  action: string;
  scalars: TypedPathScalar[];
  records: TypedPathRecord[];
}

export interface TypedPathsResult {
  paths?: TypedPaths;
  error?: { kind?: string; message?: string };
}

export async function fetchTypedPaths(
  action: string,
): Promise<TypedPathsResult> {
  await ensureReady();
  const raw = get_typed_paths_for_action_json(action);
  const env = JSON.parse(raw) as Envelope<TypedPaths>;
  if (!env.ok || !env.data) return { error: env.error ?? {} };
  return { paths: env.data };
}

export async function fetchActionSchema(
  action: string,
  overlay?: readonly OverlayField[],
): Promise<SchemaResult> {
  await ensureReady();
  // Skip the overlay codepath when the caller didn't provide one. Keeps
  // the no-overlay call shape identical to the previous behaviour so any
  // caller that doesn't care about runtime-added fields (e.g. tests that
  // assert the static schema) doesn't pay the JSON-marshalling cost or
  // need to mock additional WASM exports.
  const raw =
    overlay && overlay.length > 0
      ? get_action_schema_with_overlay_json(
          JSON.stringify({ action, overlay }),
        )
      : get_action_schema_json(action);
  const env = JSON.parse(raw) as Envelope<ActionSchemaDto>;
  if (!env.ok || !env.data) return { error: env.error ?? {} };
  return { schema: env.data };
}
