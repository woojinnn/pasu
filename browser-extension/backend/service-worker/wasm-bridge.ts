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
  // v3 install entry. Stores the raw v3 manifest in `DECLARATIVE_V3_STATE`
  // for the v3 route entry to consume.
  // Contract: `crates/policy-engine-wasm/src/declarative_exports.rs`.
  declarative_install_v3_json(bundle_json: string): string;
  // v3 orchestrator route entry. Resolves (chain_id, to, selector) through
  // the engine-internal bridge and emits the `Action` tree.
  declarative_route_request_v3_json(input_json: string): string;
  // v3 typed-data (EIP-712 sign) route entry. Keys on the typed-data triple
  // `(chain_id, verifying_contract, primary_type)` (+ optional witness_type);
  // decodes the raw EIP-712 `message` to the same ActionBody tree.
  declarative_route_typed_data_v3_json(input_json: string): string;
  // Stateless policy-RPC plan + evaluate (v2 ActionBody model).
  // Contract: `crates/policy-engine-wasm/src/action_eval_exports.rs`.
  // Returns `{ ok, data: { planned: [...] } }` / `{ ok: false, error }`.
  plan_action_rpc_v2_json(input_json: string): string;
  // Replays host results into context and aggregates each matching bundle's verdict.
  // ALWAYS returns `ok: true` — every fault becomes a `Fail` verdict.
  // Returns `{ ok, data: { verdict: VerdictDto } }`.
  evaluate_action_v2_json(input_json: string): string;
  // Diagnostic-only: lower the action and return the exact lowered Cedar context.
  // No effect on the verdict path.
  // Returns `{ ok, data: { principal, actionUid, resource, context } }`.
  debug_lowered_context_v2_json(input_json: string): string;
  // Manifest-driven schema preview + alias table.
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
  // Cedar text↔EST conversion. Contract: `crates/policy-engine-wasm/src/cedar_exports.rs`.
  policy_text_to_est_json(text: string): string;
  est_json_to_policy_text(est_json: string): string;
  // One simulation step: (state, action, ctx) → (delta, next_state).
  // Contract: `crates/policy-engine-wasm/src/sim_step_exports.rs`.
  simulate_step_json(input_json: string): string;
  // Denial diagnosis: run Cedar probes against the materialized context.
  // Contract: `crates/policy-engine-wasm/src/diagnosis_exports.rs`.
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
 * Wire shape for `declarative_route_request_v3_json`.
 *
 * `(chain_id, to, selector)` form the callkey plus the `ActionMeta` fields.
 * All numeric fields are base-10 decimal strings — passing JS `number` would
 * lose precision for uint256 values.
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
   * Current gas price as a base-10 decimal string. Defaults to `"0"`.
   * The WASM wraps this in a stub `LiveField` for the gas source.
   */
  gas_price?: string;
  /** `tx.from` — "0x" + 40 hex. */
  submitter: string;
  /** Unix epoch seconds at which the Action was submitted. */
  submitted_at: number;
  /** Sequential transaction nonce of `submitter`. Defaults to `0`. */
  nonce?: number;
  /**
   * Optional block.timestamp — distinct from `submitted_at`. Used for
   * deadline / validity mapping.
   */
  block_timestamp?: number;
}

/**
 * Result of a successful `declarative_route_request_v3_json` call.
 *
 * `actions` is the JSON-serialised `Vec<policy_transition::action::Action>`.
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
 * Result envelope from the manifest-map install path.
 *
 * Present when the caller passes `manifests` as a `{ [action]: manifest }` map —
 * the WASM install path composes the enriched schema and returns these fields.
 * Absent when the caller passes the legacy `Vec<PolicyManifest>` array shape.
 */
export interface InstallPoliciesOutput {
  enrichedSchemaHash: string;
  addedCustomFields: Record<string, unknown[]>;
}

/**
 * Install Cedar policies into the WASM engine.
 *
 * `manifests` accepts two shapes:
 * - Map `{ [action]: manifest }` → composes the enriched schema; the returned
 *   object carries `enrichedSchemaHash` + `addedCustomFields`. All callers
 *   that want the enriched schema must use this shape.
 * - Array `Vec<PolicyManifest>` → legacy shape; returns `null`.
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
 * Install a v3 declarative bundle into `DECLARATIVE_V3_STATE` so subsequent
 * `declarative_route_request_v3_json` calls find it via the callkey bridge.
 * Re-installing the same bundle is idempotent (overwrites the entry).
 *
 * The caller must stringify the bundle exactly as received from the registry —
 * `bundle_sha256` integrity depends on byte stability.
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
 * Preview the enriched cedarschema produced by a single action's manifest.
 * Returns the custom-context list, the generated cedarschema text, a diff
 * against the currently-installed action, and a hash of the previewed schema.
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
 * v3 orchestrator route entry.
 *
 * Resolves `(chain_id, to, selector)` through the engine-side bridge populated
 * at install time and produces the `Action` tree.
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
 * Wire shape for `declarative_route_typed_data_v3_json`.
 *
 * Instead of `(to, selector, calldata)` the WASM keys on the typed-data triple
 * `(chain_id, verifying_contract, primary_type)`, plus the raw EIP-712 `message`.
 * `domain_name` is not part of the routing key — EIP-2612 token Permits carry
 * the token name there, causing collisions if it were keyed on.
 */
export interface DeclarativeRouteTypedDataV3Input {
  chainId: number;
  /** "0x" + 40 hex. Case-insensitive on the engine side. */
  verifyingContract: string;
  /** EIP-712 `primaryType` discriminator (may contain a `:` segment). */
  primaryType: string;
  /**
   * Optional 4th routing-key component — the EIP-712 `witness` field's struct
   * type for Permit2 `permitWitnessTransferFrom` payloads, which otherwise
   * all collide on `(chainId, Permit2, "PermitWitnessTransferFrom")`.
   * Kept verbatim (exact EIP-712 type name). `undefined` for non-witness payloads.
   */
  witnessType?: string | undefined;
  /** Optional EIP-712 `domain.name` — audit/display only, not part of the routing key. */
  domainName?: string | undefined;
  /** Raw EIP-712 `message` object — the manifest `$args.*` decode root. */
  message: unknown;
  /** Signer address — "0x" + 40 hex. */
  submitter: string;
  /** Unix epoch seconds at which the signature was requested. */
  submittedAt: number;
}

/**
 * v3 typed-data (EIP-712 sign) route entry.
 *
 * Returns a non-throwing `{ ok, data?, error? }` envelope so the SW sig-router
 * treats a route miss as a transparent fall-through (`null`) rather than catching
 * an `EngineError`. The caller must supply snake_case wire keys matching the Rust DTO.
 */
export async function declarativeRouteTypedDataV3(
  input: DeclarativeRouteTypedDataV3Input,
): Promise<{
  ok: boolean;
  data?: { actions: unknown[]; decoder_id: string };
  error?: { kind: string; message: string };
}> {
  // Honor the non-throwing contract. A WASM-layer fault (init failure, serde
  // hiccup, etc.) must surface as `{ ok: false }` so the sig-router treats
  // it as a transparent miss, not a thrown error that bypasses the orchestrator.
  try {
    const exports = await load();
    const raw = exports.declarative_route_typed_data_v3_json(
      JSON.stringify({
        chain_id: input.chainId,
        verifying_contract: input.verifyingContract,
        primary_type: input.primaryType,
        // Omitted when undefined (JSON.stringify drops undefined values) so the
        // Rust DTO's `#[serde(default)]` yields `None` and the bridge key stays a 3-tuple.
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
 * v2 ActionBody PLAN phase.
 *
 * Lowers the decoded `ActionBody` + `ActionMeta` and plans the v2 policy-RPC
 * calls the host must dispatch. Returns the `planned` calls keyed by
 * `call_id` (`<manifest_id>::<spec_id>`); the host feeds raw results back to
 * {@link evaluateActionV2}. Stateless — `manifests` arrive inline per call.
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
 * v2 ActionBody EVALUATE phase.
 *
 * Re-lowers the action, replays the host's raw `results` into `context.custom.*`,
 * evaluates every matching bundle's Cedar policy, and aggregates verdicts by
 * deny-overrides. The WASM always returns `ok: true` — every fault becomes a
 * `Fail` verdict (`__system__` or `__engine::<kind>`).
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
  // Surface the lowered Cedar context for debugging — diagnostic-only, no
  // effect on the verdict above.
  try {
    const lowered = unwrap<{
      principal?: unknown;
      actionUid?: unknown;
      resource?: unknown;
      context?: unknown;
    }>(exports.debug_lowered_context_v2_json(JSON.stringify(input)));
    const body = input.action as { domain?: unknown; action?: unknown };
    console.debug("[Dambi] wasm.lowered-context", {
      domain: body?.domain,
      action: body?.action,
      actionUid: lowered.actionUid,
      principal: lowered.principal,
      resource: lowered.resource,
      context: lowered.context,
    });
  } catch (err) {
    console.debug("[Dambi] wasm.lowered-context (failed)", err);
  }
  console.debug("[Dambi] wasm.evaluate-action-v2", {
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
