import init, {
  compile_policy_json,
  parse_cedar_json,
  list_actions,
  get_action_schema_json,
} from "../wasm/policy_builder_wasm.js";
import type {
  ActionSchemaDto,
  Envelope,
  PolicyRule,
} from "./types";

let initPromise: Promise<unknown> | null = null;
async function ensureReady(): Promise<void> {
  if (!initPromise) initPromise = init();
  await initPromise;
}

export interface CompileResult {
  cedarText?: string;
  error?: { kind?: string; message?: string };
}

export async function compileRule(rule: PolicyRule): Promise<CompileResult> {
  await ensureReady();
  const raw = compile_policy_json(JSON.stringify(rule));
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

export async function fetchActionSchema(
  action: string,
): Promise<SchemaResult> {
  await ensureReady();
  const raw = get_action_schema_json(action);
  const env = JSON.parse(raw) as Envelope<ActionSchemaDto>;
  if (!env.ok || !env.data) return { error: env.error ?? {} };
  return { schema: env.data };
}
