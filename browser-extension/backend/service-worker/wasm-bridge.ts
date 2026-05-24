import Browser from "webextension-polyfill";
import init, * as wasmExports from "../wasm/policy_engine_wasm";
import {
  parsePolicyRpcPlan,
  parseVerdict,
  type EvaluatePolicyRpcInputDto,
  type PlanPolicyRpcInputDto,
  type PolicyRpcPlanDto,
  type PolicyRpcResponseDto,
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
  // Phase 6 — orchestrator route entry. Resolves
  // (chain_id, to, selector) through the engine-internal bridge populated
  // at install time, then runs the matching declarative mapper.
  declarative_route_request_json(input_json: string): string;
  // multicall_recurse child-callkey planner. Decodes the outer multicall in
  // WASM and returns the inner sub-call callkeys the host must fetch+install
  // before declarative_route_request_json.
  declarative_plan_children_json(input_json: string): string;
  // Phase 7A — evaluate Cedar policies against caller-supplied envelopes.
  // Skips the route → plan stages so the declarative pipeline can drive
  // verdicts directly from its post-processed envelopes.
  evaluate_with_envelopes_json(input_json: string): string;
  // origin/main — manifest-driven schema preview + alias table.
  preview_custom_schema_json(input_json: string): string;
  preview_installed_schema_json(): string;
  get_alias_table_json(): string;
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
 * Phase 6 — orchestrator route entry wire shape.
 *
 * `(chain_id, to, selector)` form the callkey for the engine-side bridge
 * lookup. `calldata` is the raw `"0x"`-prefixed transaction calldata; the
 * WASM route entry decodes it internally using the bridge-resolved bundle's
 * `abi_fragment.abi` (replaces the prior TS-side viem decode step).
 */
export interface DeclarativeRouteRequestInput {
  chain_id: number;
  /** "0x" + 40 hex. Case-insensitive on the engine side. */
  to: string;
  /** "0x" + 8 hex. Case-insensitive on the engine side. */
  selector: string;
  ctx: {
    chain_id: number;
    from: string;
    to: string;
    value_wei?: string;
    block_timestamp?: number;
  };
  /** "0x"-prefixed raw calldata. Decoded inside WASM via the bundle ABI. */
  calldata: string;
}

/**
 * Result of a successful `declarative_route_request_json` call. `decoder_id`
 * is the bundle id the bridge resolved (`declarative.<path>`); the
 * orchestrator surfaces it in audit telemetry so we can tell which
 * marketplace adapter handled a given tx.
 */
export interface DeclarativeRouteRequestResult {
  envelopes: Record<string, unknown>[];
  decoder_id: string;
}

/**
 * One child callkey from `declarative_plan_children_json`. Mirrors
 * `DeclarativeChildCallKeyDto` in `crates/policy-engine-wasm/src/dto.rs`.
 * `to` echoes the outer `to` (self-multicall); `selector` is `"0x"` + 8 hex.
 */
export interface DeclarativeChildCallKey {
  chain_id: number;
  to: string;
  selector: string;
}

/**
 * Result of `declarative_plan_children_json`. `children` is empty when the
 * outer bundle is not `multicall_recurse` (or no bundle is mounted for the
 * callkey) — the caller then skips the child-prefetch pass.
 */
export interface DeclarativePlanChildrenResult {
  children: DeclarativeChildCallKey[];
  decoder_id: string;
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
 * Phase 6 — orchestrator route entry.
 *
 * Resolves `(chain_id, to, selector)` through the engine-side bridge table
 * populated at install time by `declarative_install_json`, then runs the
 * matching mapper against the caller-supplied `decoded` call.
 *
 * Error semantics (mirrored straight from the WASM contract):
 *   - `EngineError("no_declarative_mapper", …)` — no bundle is mounted for
 *     this callkey. The orchestrator MUST treat this as a non-fatal miss
 *     and fall through to the static Tier B pipeline.
 *   - `EngineError("map_failed", …)` — bundle matched but the declarative
 *     interpreter rejected the decoded call (malformed args, type
 *     mismatch). This IS a fault.
 *   - `EngineError("invalid_input_json", …)` — the caller built a bad
 *     wire payload. Treat as a fault.
 */
export async function declarativeRouteRequest(
  input: DeclarativeRouteRequestInput,
): Promise<DeclarativeRouteRequestResult> {
  const exports = await load();
  return unwrap<DeclarativeRouteRequestResult>(
    exports.declarative_route_request_json(JSON.stringify(input)),
  );
}

/**
 * Plan the child callkeys of a `multicall_recurse` outer call.
 *
 * The WASM `WasmChildResolver` is synchronous and can only resolve a child
 * sub-call whose bundle is already mounted. This entry decodes the outer
 * multicall calldata in WASM and returns one `(chain_id, to, selector)` per
 * inner sub-call so the orchestrator can fetch+install them first.
 *
 * Returns `{ children: [], decoder_id: "" }` for a non-`multicall_recurse`
 * bundle or when no bundle is mounted. Throws `EngineError("decode_failed" |
 * "invalid_calldata" | "invalid_input_json", …)` on a malformed outer call;
 * callers treat a throw as best-effort and continue to
 * `declarativeRouteRequest`.
 */
export async function declarativePlanChildren(
  input: DeclarativeRouteRequestInput,
): Promise<DeclarativePlanChildrenResult> {
  const exports = await load();
  return unwrap<DeclarativePlanChildrenResult>(
    exports.declarative_plan_children_json(JSON.stringify(input)),
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

/**
 * Wire shape for `evaluateWithEnvelopes`. Mirrors
 * `crates/policy-engine-wasm/src/dto.rs::EvaluateWithEnvelopesInputDto`.
 *
 * `envelopes` are the post-processed (Phase 7E enriched) ActionEnvelopes the
 * declarative pipeline already produced — the WASM lowers them into Cedar
 * requests directly, skipping `route_request_json` / `plan_policy_rpc_json`.
 *
 * `rpc_response` carries `policy-rpc` results when manifests declare any
 * `requires`; pass `{ request_id, results: [] }` for pipelines that do not
 * need RPC enrichment (e.g. permit-only policies). The WASM still runs the
 * projection step against this response, so manifests that DO require RPC
 * data must supply matching results or the verdict will fail closed via
 * `__engine::projection_failed`.
 */
export interface EvaluateWithEnvelopesInput {
  envelopes: readonly Record<string, unknown>[];
  from: string;
  to: string;
  value_wei: string;
  chain_id: number;
  block_timestamp: number;
  manifests: readonly unknown[];
  rpc_response: PolicyRpcResponseDto;
}

/**
 * Phase 7A entry — evaluate Cedar policies against caller-supplied envelopes.
 *
 * The declarative pipeline produces envelopes via `declarativeRouteRequest`
 * (then post-processes them through `enrichEnvelopeAssets` to fill in
 * AssetRef `symbol`/`decimals`). Handing those enriched envelopes here lets
 * the declarative path drive Cedar verdicts — i.e. the static
 * `evaluatePolicyRpc` path is no longer the sole verdict driver.
 *
 * The WASM enforces:
 *   * Installed policies' `manifest_set_hash` matches `manifests` arg.
 *   * Installed `schema_hash` matches the schema derived from `manifests`.
 *   * RPC projection succeeds (or the response is empty when no calls).
 *
 * Failures surface as a synthetic `Fail` verdict whose `matched[0]` is
 * `__engine::<kind>` — matching the same pattern `evaluatePolicyRpc` uses
 * for engine-side faults.
 */
export async function evaluateWithEnvelopes(
  input: EvaluateWithEnvelopesInput,
): Promise<VerdictDto> {
  const exports = await load();
  const startedAtMs = Date.now();
  const inputJson = JSON.stringify(input);
  // MCP debug: dump WASM input + raw response when matched contains
  // __engine::invalid_input_json to surface the underlying serde error.
  const rawResponseStr = exports.evaluate_with_envelopes_json(inputJson);
  if (rawResponseStr.includes("invalid_input_json")) {
    console.warn("[Scopeball-MCP-debug] evaluate input (full):", inputJson);
    console.warn(
      "[Scopeball-MCP-debug] evaluate raw response (full):",
      rawResponseStr,
    );
    console.warn(
      "[Scopeball-MCP-debug] envelopes detail:",
      JSON.parse(JSON.stringify(input.envelopes)),
    );
  }
  const raw = unwrap<unknown>(rawResponseStr);
  const verdict = parseVerdict(raw);
  console.debug("[Scopeball] wasm.evaluate-with-envelopes", {
    chainId: input.chain_id,
    from: input.from,
    to: input.to,
    envelopeCount: input.envelopes.length,
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
