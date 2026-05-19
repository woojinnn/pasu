import Browser from "webextension-polyfill";
import init, * as wasmExports from "../wasm/policy_engine_wasm";
import {
  parsePolicyRpcPlan,
  parseVerdict,
  type EvaluatePolicyRpcInputDto,
  type PlanPolicyRpcInputDto,
  type PolicyRpcPlanDto,
  type VerdictDto,
} from "./wasm-bridge.types";

export { WasmDecodeError } from "./wasm-bridge.types";
export type { VerdictDto } from "./wasm-bridge.types";

interface WasmExports {
  install_policies_json(input: string): string;
  evaluate_policy_rpc_json(input_json: string): string;
  plan_policy_rpc_json(input_json: string): string;
  route_request_json(input_json: string): string;
  preview_custom_schema_json(input_json: string): string;
  preview_installed_schema_json(): string;
  get_alias_table_json(): string;
}

/**
 * One ActionEnvelope emitted by the new (Phase 5) pipeline.
 *
 * `action` is the snake_case discriminator (e.g. "swap", "permit",
 * "approve") and `fields` is the variant-specific payload.
 *
 * Field-level typing is intentionally deferred to a follow-up so this
 * minimal binding stays small. Callers that need to introspect specific
 * variants should add per-action zod-style guards as needed.
 */
export interface RouteEnvelope {
  category: string;
  action: string;
  fields: unknown;
}

interface OkEnvelope<T> {
  ok: true;
  data: T;
}
interface ErrEnvelope {
  ok: false;
  error: { kind: string; message: string };
}
type Envelope<T> = OkEnvelope<T> | ErrEnvelope;

export class EngineError extends Error {
  constructor(
    readonly kind: string,
    message: string,
  ) {
    super(`${kind}: ${message}`);
    this.name = "EngineError";
  }
}

let cachedExports: WasmExports | null = null;
let inflightLoad: Promise<WasmExports> | null = null;

const WASM_BG_URL = Browser.runtime.getURL("wasm/policy_engine_wasm_bg.wasm");

async function load(): Promise<WasmExports> {
  if (cachedExports) return cachedExports;
  if (inflightLoad) return inflightLoad;
  inflightLoad = (async () => {
    await init({ module_or_path: WASM_BG_URL });
    cachedExports = wasmExports as unknown as WasmExports;
    return cachedExports;
  })();
  return inflightLoad;
}

function unwrap<T>(json: string): T {
  const parsed = JSON.parse(json) as Envelope<T>;
  if (parsed.ok === true) return parsed.data;
  throw new EngineError(parsed.error.kind, parsed.error.message);
}

/**
 * Result envelope from the manifest-map install path (Phase 5/6).
 *
 * Present when the caller passes `manifests` as a `{ [action]: manifest }`
 * map — the WASM install path then composes the enriched schema and
 * returns these fields. Absent when the caller passes the legacy
 * `Vec<PolicyManifest>` shape (the install path skips `compose_enriched`
 * and returns a `null` data envelope).
 */
export interface InstallPoliciesOutput {
  enrichedSchemaHash: string;
  addedCustomFields: Record<string, unknown[]>;
}

/**
 * Install Cedar policies into the WASM engine.
 *
 * **Phase 6 / carry-over E:** `manifests` accepts both the legacy
 * `Vec<PolicyManifest>` (array) and the new `{ [action]: manifest }`
 * map shape. They are NOT equivalent:
 *
 * - Map shape → composes the enriched schema and the returned object
 *   carries `enrichedSchemaHash` + `addedCustomFields`. **All new
 *   Phase-6 callers (the manifest store, atomic-install, dev-seed,
 *   dashboard SDK) must use this shape.**
 * - Array shape → legacy, preserves the pre-Phase-5 install. Returns
 *   `null` for the install output. Only the legacy
 *   `policies-loader.ts` aggregator still uses it.
 *
 * Returns `null` when WASM returned the legacy null envelope, otherwise
 * the populated [`InstallPoliciesOutput`].
 */
export async function installPolicies(input: {
  schema_text: string;
  policy_set: { id: string; text: string }[];
  manifests?: readonly unknown[] | Record<string, unknown>;
}): Promise<InstallPoliciesOutput | null> {
  const exports = await load();
  const raw = unwrap<unknown>(exports.install_policies_json(JSON.stringify(input)));
  if (raw === null || raw === undefined) return null;
  if (
    typeof raw === "object" &&
    typeof (raw as { enrichedSchemaHash?: unknown }).enrichedSchemaHash === "string"
  ) {
    const r = raw as {
      enrichedSchemaHash: string;
      addedCustomFields?: Record<string, unknown[]>;
    };
    return {
      enrichedSchemaHash: r.enrichedSchemaHash,
      addedCustomFields: r.addedCustomFields ?? {},
    };
  }
  return null;
}

/**
 * Phase 5 pipeline entry. Takes a raw RPC payload and returns the list of
 * `ActionEnvelope` values that the new Decoder/Mapper/CallAdapter or
 * SignAdapter stack produces.
 *
 * Input shape: `{ method, params, chain_id, block_timestamp? }`.
 * Throws `EngineError("route_failed", ...)` when no adapter matched.
 */
export async function routeRequest(input: {
  method: string;
  params: unknown;
  chain_id: number;
  block_timestamp?: number;
}): Promise<RouteEnvelope[]> {
  const exports = await load();
  const raw = unwrap<unknown>(
    exports.route_request_json(JSON.stringify(input)),
  );
  if (!Array.isArray(raw)) {
    throw new EngineError(
      "invalid_route_response",
      `expected ActionEnvelope[] from WASM, got ${typeof raw}`,
    );
  }
  return raw.map((entry, idx) => {
    if (
      typeof entry !== "object" ||
      entry === null ||
      typeof (entry as { category?: unknown }).category !== "string" ||
      typeof (entry as { action?: unknown }).action !== "string"
    ) {
      throw new EngineError(
        "invalid_route_envelope",
        `route_request_json[${idx}] missing required fields`,
      );
    }
    const obj = entry as { category: string; action: string; fields?: unknown };
    return { category: obj.category, action: obj.action, fields: obj.fields };
  });
}

export async function planPolicyRpc(
  input: PlanPolicyRpcInputDto,
): Promise<PolicyRpcPlanDto> {
  const exports = await load();
  const startedAtMs = Date.now();
  const raw = unwrap<unknown>(exports.plan_policy_rpc_json(JSON.stringify(input)));
  const plan = parsePolicyRpcPlan(raw);
  console.debug("[Scopeball] wasm.plan", {
    requestId: input.request_id,
    method: input.raw_request.method,
    chainId: input.raw_request.chain_id,
    manifestCount: input.manifests.length,
    durationMs: Date.now() - startedAtMs,
    manifestSetHash: plan.manifest_set_hash,
    schemaHash: plan.schema_hash,
    envelopeCount: plan.envelopes.length,
    calls: plan.calls.map((c) => ({ id: c.id, method: c.method, params: c.params })),
    diagnostics: plan.diagnostics,
  });
  return plan;
}

export interface PreviewCustomSchemaOutput {
  customTypes: { name: string; fields: unknown[] }[];
  enrichedSchemaText: string;
  diff: { added: unknown[]; removed: unknown[]; changed: unknown[] };
  schemaHash: string;
}

export interface PreviewInstalledSchemaOutput {
  schema_text: string;
  schema_hash: string;
  added_fields: unknown[];
  customContexts: Record<string, unknown[]>;
  schemaHash: string;
}

export interface AliasTableEntry {
  name: string;
  kind: "scalar" | "record";
  cedarSpelling: string;
}

/**
 * Preview the enriched cedarschema produced by a single action's
 * manifest (Phase 6 / D14). Returns the full custom-context list, the
 * generated cedarschema text, a diff against any currently-installed
 * action, and a hash of the previewed schema.
 */
export async function previewCustomSchema(input: {
  action: string;
  manifest: unknown;
}): Promise<PreviewCustomSchemaOutput> {
  const exports = await load();
  return unwrap<PreviewCustomSchemaOutput>(
    exports.preview_custom_schema_json(JSON.stringify(input)),
  );
}

/**
 * Read back the currently-installed enriched cedarschema + per-action
 * custom-context fields. Used by the dashboard schema viewer to show
 * users what their installed manifests have added on top of the base
 * cedarschema.
 */
export async function previewInstalledSchema(): Promise<PreviewInstalledSchemaOutput> {
  const exports = await load();
  return unwrap<PreviewInstalledSchemaOutput>(
    exports.preview_installed_schema_json(),
  );
}

/**
 * Return the base alias table — the set of cedarschema types and
 * records that ship with the engine and that manifest authors can
 * reference in their `outputs[].type` fields.
 */
export async function getAliasTable(): Promise<{ entries: AliasTableEntry[] }> {
  const exports = await load();
  return unwrap<{ entries: AliasTableEntry[] }>(exports.get_alias_table_json());
}

export async function evaluatePolicyRpc(
  input: EvaluatePolicyRpcInputDto,
): Promise<VerdictDto> {
  const exports = await load();
  const startedAtMs = Date.now();
  const raw = unwrap<unknown>(
    exports.evaluate_policy_rpc_json(JSON.stringify(input)),
  );
  const verdict = parseVerdict(raw);
  console.debug("[Scopeball] wasm.evaluate", {
    requestId: input.plan.request_id,
    planCallCount: input.plan.calls.length,
    rpcResultCount: input.rpc_response.results.length,
    manifestCount: input.manifests.length,
    durationMs: Date.now() - startedAtMs,
    verdict: verdict.kind,
    matched:
      verdict.matched?.map((m) => ({
        id: m.policy_id,
        severity: m.severity,
      })) ?? [],
  });
  return verdict;
}
