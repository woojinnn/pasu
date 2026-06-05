type JsonRecord = Record<string, unknown>;

const VERDICT_KINDS = ["pass", "fail", "warn"] as const;
const POLICY_SEVERITIES = ["deny", "warn"] as const;
const POLICY_REQUEST_ORIGINS = ["action", "tx", "engine_error"] as const;

export type Severity = (typeof POLICY_SEVERITIES)[number];
export type Origin = (typeof POLICY_REQUEST_ORIGINS)[number];

export type VerdictDto = PassVerdictDto | WarnVerdictDto | FailVerdictDto;

export interface PolicyRpcCallDto {
  readonly id: string;
  readonly method: string;
  readonly params: unknown;
}

export interface PolicyRpcBatchRequestDto {
  readonly request_id: string;
  readonly calls: readonly PolicyRpcCallDto[];
}

export interface PolicyRpcResponseDto {
  readonly request_id: string;
  readonly results: readonly unknown[];
}

// ── v2 (ActionBody-model) policy-RPC DTOs ──────────────────────────────────
// Mirror `crates/policy-engine-wasm/src/action_eval_exports.rs`. The v2 model
// is stateless: manifests + bundles arrive inline per call. `chain_id` here is
// a CAIP-2 STRING (e.g. `"eip155:1"`), NOT a number.

/**
 * Tx-level routing fields for the v2 exports. Mirrors the Rust `TxInput`
 * (`action_eval_exports.rs`). `chain_id` is the CAIP-2 string.
 */
export interface ActionTxInputDto {
  readonly chain_id: string;
  readonly from: string;
  readonly to: string;
}

/**
 * Input to `plan_action_rpc_v2_json`. Mirrors the Rust `PlanActionInput`.
 *
 * `action` is the snake_case-tagged `ActionBody` (e.g. `{ amm: {...} }`) and
 * `meta` is the `ActionMeta`; both are kept opaque here (the bridge does not
 * model the variant schema — downstream decode produces them).
 */
export interface PlanActionRpcV2InputDto {
  readonly manifests: readonly unknown[];
  readonly action: unknown;
  readonly meta: unknown;
  readonly tx: ActionTxInputDto;
  /**
   * Host-resolved per-token decimals (lowercase `0x` address → decimals), used
   * by the WASM lowering to fill each fungible amount's `amountNano` `Long`
   * sibling. Omitted ⇒ no nano fields are emitted.
   */
  readonly token_decimals?: Readonly<Record<string, number>>;
  /**
   * Host-resolved per-asset venue leverage (decimal-string `asset_index` →
   * effective leverage), used by the WASM lowering to fill the HL order
   * `leverage` `Long` field from the SW's `activeAssetData` lookup. Omitted ⇒
   * the field is not emitted (a `context has leverage` policy stays dormant).
   */
  readonly account_leverage?: Readonly<Record<string, number>>;
}

/**
 * One planned v2 policy-RPC call. Serializable mirror of the Rust
 * `PlannedCallDto`. `call_id` is `<manifest_id>::<spec_id>`; `outputs` are the
 * opaque projection rules rooted at `$.result`.
 */
export interface PlannedCallV2Dto {
  readonly manifest_id: string;
  readonly call_id: string;
  readonly method: string;
  readonly params: unknown;
  readonly outputs: readonly unknown[];
  readonly optional: boolean;
}

/**
 * One installed bundle for `evaluate_action_v2_json`. Mirrors the Rust
 * `BundleInput` — `{ policy, manifest }`, NO `id` field. `manifest` is kept
 * opaque (validated inside WASM against `ManifestV2`).
 */
export interface ActionBundleInputDto {
  readonly policy: string;
  readonly manifest: unknown;
}

/**
 * Input to `evaluate_action_v2_json`. Mirrors the Rust `EvaluateActionInput`.
 *
 * `results` is the host's raw results keyed by `call_id` — each value is the
 * UNWRAPPED `$.result` payload (NOT the `{ id, ok, result }` envelope). The
 * planned set that drives the SystemFail gate derives from `bundles[].manifest`
 * inside WASM; there is deliberately no top-level `manifests` field.
 */
export interface EvaluateActionV2InputDto {
  readonly action: unknown;
  readonly meta: unknown;
  readonly tx: ActionTxInputDto;
  readonly bundles: readonly ActionBundleInputDto[];
  readonly results: Readonly<Record<string, unknown>>;
  /**
   * Host-resolved per-token decimals (see
   * {@link PlanActionRpcV2InputDto.token_decimals}).
   */
  readonly token_decimals?: Readonly<Record<string, number>>;
  /**
   * Host-resolved per-asset venue leverage (see
   * {@link PlanActionRpcV2InputDto.account_leverage}).
   */
  readonly account_leverage?: Readonly<Record<string, number>>;
}

export interface PassVerdictDto {
  readonly kind: "pass";
  readonly matched?: undefined;
}

export interface WarnVerdictDto {
  readonly kind: "warn";
  readonly matched: readonly MatchedPolicyDto[];
}

export interface FailVerdictDto {
  readonly kind: "fail";
  readonly matched: readonly MatchedPolicyDto[];
}

export interface MatchedPolicyDto {
  readonly policy_id: string;
  readonly reason: string | null;
  readonly severity: Severity;
  readonly origin: Origin;
}

export class WasmDecodeError extends Error {
  constructor(
    message: string,
    readonly export_name: string,
    readonly value_preview?: unknown,
  ) {
    super(message);
    this.name = "WasmDecodeError";
  }
}

export function parseVerdict(value: unknown): VerdictDto {
  const record = requireRecord(value, "evaluate", "$");
  const kind = requireOneOf(
    record,
    "kind",
    "$.kind",
    "evaluate",
    VERDICT_KINDS,
  );

  if (kind === "pass") {
    if (hasOwn(record, "matched")) {
      fail(
        "evaluate",
        "$.matched",
        "expected field to be absent",
        record.matched,
      );
    }
    return { kind };
  }

  return {
    kind,
    matched: parseArrayField(
      record,
      "matched",
      "$.matched",
      "evaluate",
      parseMatchedPolicy,
    ),
  };
}

function parseMatchedPolicy(
  value: unknown,
  path: string,
  exportName: string,
): MatchedPolicyDto {
  const record = requireRecord(value, exportName, path);
  return {
    policy_id: requireString(
      record,
      "policy_id",
      `${path}.policy_id`,
      exportName,
    ),
    reason: requireNullableString(
      record,
      "reason",
      `${path}.reason`,
      exportName,
    ),
    severity: requireOneOf(
      record,
      "severity",
      `${path}.severity`,
      exportName,
      POLICY_SEVERITIES,
    ),
    origin: requireOneOf(
      record,
      "origin",
      `${path}.origin`,
      exportName,
      POLICY_REQUEST_ORIGINS,
    ),
  };
}

function parseArrayField<T>(
  record: JsonRecord,
  key: string,
  path: string,
  exportName: string,
  parseItem: (value: unknown, path: string, exportName: string) => T,
): readonly T[] {
  const value = requireField(record, key, path, exportName);
  if (!Array.isArray(value)) {
    fail(exportName, path, "expected array", value);
  }
  return value.map((item, index) =>
    parseItem(item, `${path}[${index}]`, exportName),
  );
}

function requireRecord(
  value: unknown,
  exportName: string,
  path: string,
): JsonRecord {
  if (isRecord(value)) return value;
  fail(exportName, path, "expected object", value);
}

function requireField(
  record: JsonRecord,
  key: string,
  path: string,
  exportName: string,
): unknown {
  if (hasOwn(record, key)) return record[key];
  fail(exportName, path, "missing required field", record);
}

function requireString(
  record: JsonRecord,
  key: string,
  path: string,
  exportName: string,
): string {
  const value = requireField(record, key, path, exportName);
  if (typeof value === "string") return value;
  fail(exportName, path, "expected string", value);
}

function requireNullableString(
  record: JsonRecord,
  key: string,
  path: string,
  exportName: string,
): string | null {
  const value = requireField(record, key, path, exportName);
  if (value === null || typeof value === "string") return value;
  fail(exportName, path, "expected string or null", value);
}

function requireOneOf<const T extends readonly string[]>(
  record: JsonRecord,
  key: string,
  path: string,
  exportName: string,
  allowed: T,
): T[number] {
  const value = requireString(record, key, path, exportName);
  if ((allowed as readonly string[]).includes(value)) return value as T[number];
  fail(exportName, path, `expected one of ${allowed.join(", ")}`, value);
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasOwn(record: JsonRecord, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(record, key);
}

function fail(
  exportName: string,
  path: string,
  message: string,
  value: unknown,
): never {
  throw new WasmDecodeError(`${path}: ${message}`, exportName, value);
}
