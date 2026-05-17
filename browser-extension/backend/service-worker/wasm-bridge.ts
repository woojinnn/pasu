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
  // Phase 1B — declarative adapter pipeline.
  // Contract documented in
  // `crates/policy-engine-wasm/src/declarative_exports.rs`.
  declarative_install_json(bundle_json: string): string;
  declarative_lookup_json(input_json: string): string;
}

/**
 * Result of a successful `declarative_install_json` call. `decoder_id` is the
 * `declarative.<path>` key the engine uses to route lookups; `bundle_id` is the
 * `<path>@<version>` identifier from the bundle JSON, retained for audit /
 * debug surfaces.
 */
export interface DeclarativeInstallResult {
  decoder_id: string;
  bundle_id: string;
}

/**
 * Wire shape consumed by `declarative_lookup_json`. The
 * `decoded.value.kind` discriminator mirrors `DecodedValueDto` in
 * `crates/policy-engine-wasm/src/dto.rs`.
 */
export interface DeclarativeLookupInput {
  decoder_id: string;
  ctx: {
    chain_id: number;
    from: string;
    to: string;
    value_wei?: string;
    block_timestamp?: number;
  };
  decoded: {
    decoder_id: string;
    function_signature: string;
    args: Array<{
      name: string;
      abi_type: string;
      value: unknown;
    }>;
  };
}

/**
 * Per Phase 1A, `envelopes` are the JSON-serialised `Vec<ActionEnvelope>` that
 * `DeclarativeMapper::map` produces. We surface them as opaque records so the
 * bridge does not couple to the variant-specific schema yet — downstream
 * consumers (policy-rpc / Cedar) parse them against the action schema.
 */
export interface DeclarativeLookupResult {
  envelopes: Record<string, unknown>[];
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

export async function installPolicies(input: {
  schema_text: string;
  policy_set: { id: string; text: string }[];
  manifests?: readonly unknown[];
}): Promise<void> {
  const exports = await load();
  unwrap<unknown>(exports.install_policies_json(JSON.stringify(input)));
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
    calls: plan.calls.map((c) => ({ id: c.id, method: c.method })),
    diagnostics: plan.diagnostics,
  });
  return plan;
}

/**
 * Phase 1B — install a declarative adapter bundle into the engine. The
 * bundle JSON must conform to `ADAPTER_MARKETPLACE_ARCHITECTURE.md` §4.1; the
 * engine returns the `declarative.<path>` decoder id keyed off the bundle.
 *
 * Re-installing the same bundle is idempotent on the engine side — the
 * mapper is replaced in the process-local registry — so callers don't have
 * to dedupe.
 */
export async function installDeclarativeBundle(
  bundleJson: string,
): Promise<DeclarativeInstallResult> {
  const exports = await load();
  return unwrap<DeclarativeInstallResult>(
    exports.declarative_install_json(bundleJson),
  );
}

/**
 * Phase 1B — run an installed declarative mapper against a decoded call.
 *
 * The input shape is forwarded verbatim to the WASM contract; see
 * `crates/policy-engine-wasm/src/declarative_exports.rs` for the DTO
 * definitions. Throws `EngineError("decoder_id_not_installed", ...)` when
 * the lookup id is unknown — the caller is expected to fetch + install the
 * bundle via `installDeclarativeBundle` before retrying.
 */
export async function declarativeMap(
  input: DeclarativeLookupInput,
): Promise<DeclarativeLookupResult> {
  const exports = await load();
  return unwrap<DeclarativeLookupResult>(
    exports.declarative_lookup_json(JSON.stringify(input)),
  );
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
