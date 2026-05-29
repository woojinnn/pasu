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
  // M2 — v3 install entry. Stores the raw v3 manifest
  // (`type: "adapter_action"`, `schema_version: "3"`, hierarchical
  // `emit.body`) in `DECLARATIVE_V3_STATE` for the v3 route entry to
  // consume.
  declarative_install_v3_json(bundle_json: string): string;
  // Phase 4B — v3 orchestrator route entry. Emits the PDF FSM
  // `simulation_reducer::action::Action` tree via the registry-v2 manifest
  // lookup + emit-rule decode pipeline.
  declarative_route_request_v3_json(input_json: string): string;
  // Phase A.1 — v3 typed-data (EIP-712 sign) route entry. Same emit-rule
  // decode pipeline as `declarative_route_request_v3_json`, keyed on the
  // typed-data triple `(chain_id, verifying_contract, primary_type)` the
  // install populated in the typed_data bridge. `message` is the raw
  // EIP-712 message object the manifest `$args.*` placeholders resolve over.
  declarative_route_typed_data_v3_json(input_json: string): string;
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
 * Result of a successful `declarative_install_v3_json` call. `decoder_id` is
 * the bundle id the v3 install stored against the callkey bridge; `bundle_id`
 * is the `<path>@<version>` identifier from the bundle JSON, retained for
 * audit / debug surfaces. Both fields carry the same value in v3 (the bundle
 * id is the canonical key — there is no separate `declarative.<path>` minting).
 */
export interface DeclarativeInstallResult {
  decoder_id: string;
  bundle_id: string;
}

/**
 * Phase 4B — wire shape for `declarative_route_request_v3_json`.
 *
 * Same callkey as the v1 entry, plus the meta fields the new
 * `simulation_reducer::action::ActionMeta` carries (`value` / `gas_limit` /
 * `gas_price` / `submitter` / `submitted_at` / `nonce`). All numeric fields
 * are passed as base-10 decimal strings — the WASM converts them to
 * `U256`/`u64` internally; passing JS `number` would lose precision for
 * uint256 values.
 *
 * `selector` and `block_timestamp` are reserved for Phase 4D's registry-v2
 * manifest lookup. They must be supplied; the Phase 4B stub does not read
 * them but the wire shape is locked so adding the lookup later is purely a
 * Rust-side change.
 */
export interface DeclarativeRouteRequestV3Input {
  chain_id: number;
  /** "0x" + 40 hex. Case-insensitive on the engine side. */
  to: string;
  /** "0x" + 8 hex. Case-insensitive on the engine side. */
  selector: string;
  /** Raw "0x"-prefixed calldata. */
  calldata: string;
  /** `msg.value` as a base-10 decimal string. Defaults to `"0"`. */
  value?: string;
  /** Declared gas limit as a base-10 decimal string. Defaults to `"0"`. */
  gas_limit?: string;
  /**
   * Current gas price as a base-10 decimal string. Defaults to `"0"`. The
   * WASM wraps this in a stub `LiveField` whose source is Pyth
   * `gas/eip155:<chain_id>` — Phase 5+ replaces this with a Sync
   * Orchestrator hookup.
   */
  gas_price?: string;
  /** `tx.from` — "0x" + 40 hex. */
  submitter: string;
  /** Unix epoch seconds at which the Action was submitted. */
  submitted_at: number;
  /** Sequential transaction nonce of `submitter`. Defaults to `0`. */
  nonce?: number;
  /**
   * Optional block.timestamp — distinct from `submitted_at`. Reserved for
   * Phase 4D's deadline / validity mapping.
   */
  block_timestamp?: number;
}

/**
 * Result of a successful `declarative_route_request_v3_json` call.
 *
 * `actions` is the JSON-serialised `Vec<simulation_reducer::action::Action>`
 * the WASM produced. Phase 4B emits a single-element vec whose body is the
 * `Unknown` stub; Phase 4D fills in the real `ActionBody` per registry-v2
 * manifest emit-rule.
 *
 * `decoder_id` echoes the matched bundle's declarative decoder id when a
 * manifest matched (`""` when no match — the Phase 4B stub never matches).
 */
export interface DeclarativeRouteRequestV3Result {
  actions: Record<string, unknown>[];
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
  const raw = unwrap<unknown>(
    exports.install_policies_json(JSON.stringify(input)),
  );
  if (raw === null || raw === undefined) return null;
  if (
    typeof raw === "object" &&
    typeof (raw as { enrichedSchemaHash?: unknown }).enrichedSchemaHash ===
      "string"
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
  const raw = unwrap<unknown>(
    exports.plan_policy_rpc_json(JSON.stringify(input)),
  );
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
    calls: plan.calls.map((c) => ({
      id: c.id,
      method: c.method,
      params: c.params,
    })),
    diagnostics: plan.diagnostics,
  });
  return plan;
}

/**
 * M2 — install a v3 declarative bundle (`type: "adapter_action"`,
 * `schema_version: "3"`, hierarchical `emit.body`) into the engine's
 * `DECLARATIVE_V3_STATE` so subsequent `declarative_route_request_v3_json`
 * calls find it via the same callkey `(chain_id, to, selector)` bridge.
 *
 * Error semantics:
 *   - `EngineError("invalid_bundle_json", …)` — payload is not valid JSON
 *     once stringified on the SW side. The caller built a bad request and
 *     must fix it.
 *   - `EngineError("missing_id", …)` — bundle has no `id` string.
 *   - `EngineError("invalid_match", …)` — `match` is missing or its
 *     `BundleMatch` deserialisation failed inside WASM.
 *
 * The caller MUST stringify the bundle exactly as it received it from the
 * registry (no re-canonicalisation) — `bundle_sha256` integrity downstream
 * depends on byte stability.
 */
export async function declarativeInstallV3(
  bundleJson: string,
): Promise<DeclarativeInstallResult> {
  const exports = await load();
  return unwrap<DeclarativeInstallResult>(
    exports.declarative_install_v3_json(bundleJson),
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
 * Phase 4B — v3 orchestrator route entry.
 *
 * Produces the PDF FSM `simulation_reducer::action::Action` tree via the
 * registry-v2 manifest lookup + emit-rule decode pipeline.
 *
 * Error semantics:
 *   - `EngineError("invalid_input_json", …)` — malformed wire payload.
 *     The caller built a bad request and must fix it.
 *   - `EngineError("input_too_large", …)` — JSON exceeded the WASM input
 *     budget. Caller should split / shorten the request.
 *   - `EngineError("no_declarative_v3_mapper", …)` — no bundle is mounted
 *     for the callkey. The orchestrator MUST treat this as a non-fatal
 *     miss and fall through to the static Tier B pipeline.
 */
export async function declarativeRouteRequestV3(
  input: DeclarativeRouteRequestV3Input,
): Promise<DeclarativeRouteRequestV3Result> {
  const exports = await load();
  return unwrap<DeclarativeRouteRequestV3Result>(
    exports.declarative_route_request_v3_json(JSON.stringify(input)),
  );
}

/**
 * Phase A.1 — wire shape for `declarative_route_typed_data_v3_json`.
 *
 * The typed-data analogue of {@link DeclarativeRouteRequestV3Input}: instead
 * of `(to, selector, calldata)` the WASM keys on the typed-data triple
 * `(chain_id, verifying_contract, primary_type)` the install bridged, plus
 * the raw EIP-712 `message` object the manifest `$args.*` placeholders read.
 *
 * `domain_name` is optional — EIP-2612 token Permits carry the token name as
 * `domain.name`, so it can't be part of the routing key; the WASM only uses
 * it for audit / display. `submitted_at` is unix-epoch seconds.
 */
export interface DeclarativeRouteTypedDataV3Input {
  chainId: number;
  /** "0x" + 40 hex. Case-insensitive on the engine side. */
  verifyingContract: string;
  /** EIP-712 `primaryType` discriminator (may contain a `:` segment). */
  primaryType: string;
  /**
   * Optional 4th routing-key component (T1) — the EIP-712 `witness` field's
   * struct type for Permit2 `permitWitnessTransferFrom` payloads (UniswapX
   * intent orders etc.), which otherwise all collide on
   * `(chainId, Permit2, "PermitWitnessTransferFrom")`. Kept VERBATIM (the exact
   * EIP-712 type name). `undefined` for non-witness payloads → the WASM bridge
   * key keeps its 3-tuple shape. Typed `string | undefined` so callers can
   * forward a derived value straight through under `exactOptionalPropertyTypes`.
   */
  witnessType?: string | undefined;
  /**
   * Optional EIP-712 `domain.name` — audit only, not part of the key. Typed
   * as `string | undefined` (not just optional) so callers can forward
   * `typedData.domain.name` straight through under `exactOptionalPropertyTypes`.
   */
  domainName?: string | undefined;
  /** Raw EIP-712 `message` object — the manifest `$args.*` decode root. */
  message: unknown;
  /** Signer address — "0x" + 40 hex. */
  submitter: string;
  /** Unix epoch seconds at which the signature was requested. */
  submittedAt: number;
}

/**
 * Phase A.1 — v3 typed-data (EIP-712 sign) route entry.
 *
 * Mirrors {@link declarativeRouteRequestV3} but returns the WASM envelope
 * in a non-throwing `{ ok, data?, error? }` shape so the SW sig-router can
 * treat a `route_failed` / `no_declarative_v3_mapper` miss as a transparent
 * fall-through (`null`) rather than catching an `EngineError`. `actions` is
 * the JSON-serialised `Vec<simulation_reducer::action::Action>`; `decoder_id`
 * is the matched bundle id (`""` on no match).
 *
 * The caller marshals the snake_case wire keys
 * (`chain_id, verifying_contract, primary_type, domain_name, message,
 * submitter, submitted_at`) the Rust DTO expects.
 */
export async function declarativeRouteTypedDataV3(
  input: DeclarativeRouteTypedDataV3Input,
): Promise<{
  ok: boolean;
  data?: { actions: unknown[]; decoder_id: string };
  error?: { kind: string; message: string };
}> {
  // T5 review fix — honor the non-throwing contract. A WASM-layer fault
  // (init failure, a non-JSON panic string from the export, or a serde
  // hiccup) must surface as a `{ ok: false }` envelope so the SW
  // sig-router treats it as a transparent miss, NOT as a thrown
  // `EngineError` that would bubble past the orchestrator's try/catch.
  try {
    const exports = await load();
    const raw = exports.declarative_route_typed_data_v3_json(
      JSON.stringify({
        chain_id: input.chainId,
        verifying_contract: input.verifyingContract,
        primary_type: input.primaryType,
        // T1 — 4th routing-key component. Omitted from the JSON when undefined
        // (JSON.stringify drops undefined values), so the Rust DTO's
        // `#[serde(default)]` yields `None` and the bridge key stays a 3-tuple.
        witness_type: input.witnessType,
        domain_name: input.domainName,
        message: input.message,
        submitter: input.submitter,
        submitted_at: input.submittedAt,
      }),
    );
    const parsed = JSON.parse(raw) as Envelope<{
      actions: unknown[];
      decoder_id: string;
    }>;
    if (parsed.ok === true) {
      return { ok: true, data: parsed.data };
    }
    return { ok: false, error: parsed.error };
  } catch (err) {
    return {
      ok: false,
      error: { kind: "parse_failed", message: String(err) },
    };
  }
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
 * The declarative pipeline produces envelopes via `declarativeRouteRequestV3`.
 * Handing those envelopes here lets the declarative path drive Cedar verdicts
 * — i.e. the static `evaluatePolicyRpc` path is no longer the sole verdict
 * driver.
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
