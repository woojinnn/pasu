import Browser from "webextension-polyfill";
import init, * as wasmExports from "../wasm/policy_engine_wasm";
import type {
  Action as ActionDto,
  EvalContext as EvalContextDto,
  StateDelta as StateDeltaDto,
  WalletState as WalletStateDto,
} from "../wasm/policy_engine_wasm";
import {
  parseVerdict,
  type EvaluateActionV2InputDto,
  type PlanActionRpcV2InputDto,
  type PlannedCallV2Dto,
  type VerdictDto,
} from "./wasm-bridge.types";

export { WasmDecodeError } from "./wasm-bridge.types";
export type {
  ActionBundleInputDto,
  ActionTxInputDto,
  EvaluateActionV2InputDto,
  PlanActionRpcV2InputDto,
  PlannedCallV2Dto,
  VerdictDto,
} from "./wasm-bridge.types";

interface WasmExports {
  install_policies_json(input: string): string;
  // M2 — v3 install entry. Stores the raw v3 manifest
  // (`type: "adapter_action"`, `schema_version: "3"`, hierarchical
  // `emit.body`) in `DECLARATIVE_V3_STATE` for the v3 route entry to consume.
  // Contract documented in
  // `crates/policy-engine-wasm/src/declarative_exports.rs`.
  declarative_install_v3_json(bundle_json: string): string;
  // Phase 4B — v3 orchestrator route entry. Resolves (chain_id, to, selector)
  // through the engine-internal bridge populated at install time, then emits
  // the PDF FSM `policy_transition::action::Action` tree.
  declarative_route_request_v3_json(input_json: string): string;
  // Phase A.1 — v3 typed-data (EIP-712 sign) route entry. Keys on the
  // typed-data triple `(chain_id, verifying_contract, primary_type)` (+ optional
  // witness_type) the install bridged; decodes the raw EIP-712 `message` to the
  // same ActionBody tree as `declarative_route_request_v3_json`.
  declarative_route_typed_data_v3_json(input_json: string): string;
  // Phase 1 (v2 ActionBody model) — stateless policy-RPC plan + evaluate.
  // Contract: `crates/policy-engine-wasm/src/action_eval_exports.rs`.
  // `plan_action_rpc_v2_json` lowers the action + plans its policy-RPC calls
  // (`{ ok, data: { planned: [...] } }` / `{ ok: false, error }`).
  plan_action_rpc_v2_json(input_json: string): string;
  // `evaluate_action_v2_json` replays the host results into context and
  // aggregates each matching bundle's verdict. ALWAYS returns `ok: true` —
  // every fault becomes a `Fail` verdict (`__system__` / `__engine::*`).
  // (`{ ok, data: { verdict: VerdictDto } }`).
  evaluate_action_v2_json(input_json: string): string;
  // origin/main — manifest-driven schema preview + alias table.
  preview_custom_schema_json(input_json: string): string;
  preview_installed_schema_json(): string;
  field_catalog_json(): string;
  get_alias_table_json(): string;
  // Editor / Simulation page exports — schema-less Cedar parse +
  // Authorizer over ad-hoc requests. `apps/web` posts message via
  // dashboard-bridge → service-worker calls these. Contract:
  // `crates/policy-engine-wasm/src/cedar_exports.rs`.
  validate_policy_text(text: string): string;
  test_policy_text(text: string, request_json: string): string;
  simulate_policy_sequence(steps_json: string, policies_json: string): string;
  // Cedar text↔EST (block-IR engine). Contract:
  // `crates/policy-engine-wasm/src/cedar_exports.rs`.
  policy_text_to_est_json(text: string): string;
  est_json_to_policy_text(est_json: string): string;
  // Simulation step — one (state, action, ctx) → (delta, next_state). Contract:
  // `crates/policy-engine-wasm/src/sim_step_exports.rs`. The host owns the
  // per-tx loop and feeds `next_state` back as `state` on the next call.
  simulate_step_json(input_json: string): string;
  // Denial diagnosis: run Cedar probes against the materialized context and
  // return which probe ids were true / errored. Contract:
  // `crates/policy-engine-wasm/src/diagnosis_exports.rs`.
  run_diagnosis_probes_v2_json(input_json: string): string;
}

/**
 * Result of a successful `declarative_install_v3_json` call. `decoder_id` is
 * the `declarative.<path>` key the engine uses to route lookups; `bundle_id`
 * is the `<path>@<version>` identifier from the bundle JSON, retained for
 * audit / debug surfaces.
 */
export interface DeclarativeInstallResult {
  decoder_id: string;
  bundle_id: string;
}

/**
 * Phase 4B — wire shape for `declarative_route_request_v3_json`.
 *
 * `(chain_id, to, selector)` form the callkey, plus the meta fields the
 * `policy_transition::action::ActionMeta` carries (`value` / `gas_limit` /
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
 * `actions` is the JSON-serialised `Vec<policy_transition::action::Action>`
 * the WASM produced. Phase 4B emits a single-element vec whose body is the
 * `Unknown` stub; Phase 4D fills in the real `ActionBody` per registry-v2
 * manifest emit-rule.
 *
 * `decoder_id` echoes the matched bundle's declarative decoder id.
 */
export interface DeclarativeRouteRequestV3Result {
  actions: Record<string, unknown>[];
  decoder_id: string;
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
 * M2 — install a v3 declarative bundle (`type: "adapter_action"`,
 * `schema_version: "3"`, hierarchical `emit.body`) into the engine's
 * `DECLARATIVE_V3_STATE` so subsequent `declarative_route_request_v3_json`
 * calls find it via the callkey `(chain_id, to, selector)` bridge.
 *
 * Re-installing the same bundle is idempotent on the engine side — it
 * overwrites the bridge entry + bundle map — so callers don't have to dedupe.
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
 * Resolves `(chain_id, to, selector)` through the engine-side bridge table
 * populated at install time by `declarative_install_v3_json`, then produces
 * the PDF FSM `policy_transition::action::Action` tree. The wire boundary is
 * locked at Phase 4B; the Rust stub currently returns a single
 * `ActionBody::Unknown` so the SW + Cedar path can already exercise the v3
 * type — manifest lookup + emit-rule decoding lands in Phase 4D.
 *
 * Error semantics:
 *   - `EngineError("invalid_input_json", …)` — malformed wire payload.
 *     The caller built a bad request and must fix it.
 *   - `EngineError("input_too_large", …)` — JSON exceeded the WASM input
 *     budget. Caller should split / shorten the request.
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
 * the JSON-serialised `Vec<policy_transition::action::Action>`; `decoder_id`
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

/**
 * Phase 1 (v2 ActionBody model) — PLAN phase.
 *
 * Lowers the decoded `ActionBody` + `ActionMeta` and plans the v2 policy-RPC
 * calls the host must dispatch. Returns the `planned` calls keyed by
 * `call_id` (`<manifest_id>::<spec_id>`); the host fetches each and feeds the
 * raw results back to {@link evaluateActionV2}.
 *
 * Stateless: the `manifests` arrive inline per call rather than via an install
 * step. The WASM returns `{ ok, data: { planned: [...] } }`; this wrapper
 * unwraps the envelope and returns `data.planned`.
 *
 * Error semantics:
 *   - `EngineError("invalid_input_json", …)` — malformed wire payload.
 *   - `EngineError("plan_failed" | "unsupported_action", …)` — the action
 *     could not be lowered / planned.
 */
export async function planActionRpcV2(
  input: PlanActionRpcV2InputDto,
): Promise<PlannedCallV2Dto[]> {
  const exports = await load();
  const { planned } = unwrap<{ planned: PlannedCallV2Dto[] }>(
    exports.plan_action_rpc_v2_json(JSON.stringify(input)),
  );
  return planned;
}

/**
 * Phase 1 (v2 ActionBody model) — EVALUATE phase.
 *
 * Re-lowers the action, replays the host's raw `results` into
 * `context.custom.*`, then evaluates every matching bundle's Cedar policy and
 * aggregates the per-bundle verdicts by deny-overrides.
 *
 * The WASM ALWAYS returns `ok: true` — every fault becomes a `Fail` verdict
 * carrying a synthetic `__system__` (missing required RPC result) or
 * `__engine::<kind>` matched policy, so there is no `ok: false` / `EngineError`
 * path here. The envelope nests the verdict under `data.verdict`; this wrapper
 * runs that through {@link parseVerdict}.
 */
export async function evaluateActionV2(
  input: EvaluateActionV2InputDto,
): Promise<VerdictDto> {
  const exports = await load();
  const startedAtMs = Date.now();
  const { verdict: rawVerdict } = unwrap<{ verdict: unknown }>(
    exports.evaluate_action_v2_json(JSON.stringify(input)),
  );
  const verdict = parseVerdict(rawVerdict);
  console.debug("[Scopeball] wasm.evaluate-action-v2", {
    chainId: input.tx.chain_id,
    from: input.tx.from,
    to: input.tx.to,
    bundleCount: input.bundles.length,
    resultCount: Object.keys(input.results).length,
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

/** Run denial-diagnosis probes; returns the raw `{ ok, data: { true_ids, error_ids } }`
 *  envelope JSON STRING from WASM (the dashboard re-parses it). `inputJson` is the
 *  serialized `{ action, meta, tx, bundles, results, probes }` built by the
 *  dashboard's `runDiagnosisProbes`. Backs the `run-diagnosis-probes` SW op. */
export async function runDiagnosisProbesV2(inputJson: string): Promise<string> {
  const exports = await load();
  return exports.run_diagnosis_probes_v2_json(inputJson);
}

// ── Cedar editor exports (apps/web dashboard) ───────────────────────────

/** Cedar parse-check. JSON shape matches `crates/policy-engine-wasm
 *  /src/cedar_exports.rs::ValidateResp`. */
export async function validatePolicyText(text: string): Promise<string> {
  const exports = await load();
  return exports.validate_policy_text(text);
}

/** Cedar Authorizer over a single ad-hoc request. The arguments are
 *  passed through as JSON strings; the caller owns the shape (matches
 *  `cedar_exports.rs::CedarRequestInput`). Return is a JSON-serialized
 *  `TestResp`. */
export async function testPolicyText(text: string, requestJson: string): Promise<string> {
  const exports = await load();
  return exports.test_policy_text(text, requestJson);
}

/** Fan-out: N steps × M policies → JSON `SequenceResp`. */
export async function simulatePolicySequence(
  stepsJson: string,
  policiesJson: string,
): Promise<string> {
  const exports = await load();
  return exports.simulate_policy_sequence(stepsJson, policiesJson);
}

/** Cedar text → EST JSON. Returns the raw wasm JSON string
 *  `{ ok, policies: [{ id, est }] }` | `{ ok:false, error }`. */
export async function policyTextToEst(text: string): Promise<string> {
  const exports = await load();
  return exports.policy_text_to_est_json(text);
}

/** EST JSON → Cedar text. Returns the raw wasm JSON string
 *  `{ ok, text }` | `{ ok:false, error }`. */
export async function estToPolicyText(estJson: string): Promise<string> {
  const exports = await load();
  return exports.est_json_to_policy_text(estJson);
}

/** Per-action typed field catalog for block-editor annotations:
 *  `{ [actionId]: { path, type, fieldKind, source }[] }`, keyed by the
 *  policy-facing action id. Display metadata only (non-authoritative). */
export interface FieldCatalog {
  [action: string]: { path: string; type: string; fieldKind: string; source: string }[];
}

export async function fieldCatalog(): Promise<FieldCatalog> {
  const exports = await load();
  return unwrap<FieldCatalog>(exports.field_catalog_json());
}

// ── simulation step ────────────────────────────────────────────────────────

export interface SimulateStepInput {
  state: WalletStateDto;
  action: ActionDto;
  ctx: EvalContextDto;
}

export interface SimulateStepOutput {
  delta: StateDeltaDto;
  next_state: WalletStateDto;
}

/**
 * One simulation step: feed `(state, action, ctx)`, get back `(delta,
 * next_state)`. Caller owns the loop and substitutes `next_state` as the
 * `state` of the following call. The WASM keeps no state across calls — the
 * triple `(state, action, ctx)` fully determines the output, so a buggy step
 * is reproduced by re-submitting the same input.
 *
 * For multicall actions, pass each inner `Action` from
 * `declarativeRouteRequestV3` in order; this entry does not split a
 * multicall.
 *
 * Throws `EngineError` with kind:
 *   - `invalid_input` (JSON parse / size)
 *   - `apply_failed` (reducer rejected the action — bad state / unsupported)
 *   - `apply_delta_failed` (invariant violation when composing the delta)
 */
export async function simulateStep(
  input: SimulateStepInput,
): Promise<SimulateStepOutput> {
  const exports = await load();
  return unwrap<SimulateStepOutput>(
    exports.simulate_step_json(JSON.stringify(input)),
  );
}
