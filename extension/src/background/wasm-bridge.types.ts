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

export interface PolicyRpcRootDto {
  readonly chain_id: number;
  readonly from: string;
  readonly to: string;
  readonly value_wei: string;
  readonly block_timestamp?: number;
}

export interface PolicyRpcPlanDto {
  readonly request_id: string;
  readonly root: PolicyRpcRootDto;
  readonly envelopes: readonly unknown[];
  readonly calls: readonly PolicyRpcCallDto[];
  readonly manifest_set_hash: string;
  readonly schema_hash: string;
  readonly diagnostics: readonly string[];
}

export interface PolicyRpcResponseDto {
  readonly request_id: string;
  readonly results: readonly unknown[];
}

export interface PlanPolicyRpcInputDto {
  readonly request_id: string;
  readonly raw_request: {
    readonly method: string;
    readonly params: unknown;
    readonly chain_id: number;
    readonly block_timestamp?: number;
  };
  readonly manifests: readonly unknown[];
}

export interface EvaluatePolicyRpcInputDto {
  readonly plan: PolicyRpcPlanDto;
  readonly rpc_response: PolicyRpcResponseDto;
  readonly manifests: readonly unknown[];
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

export function parsePolicyRpcPlan(value: unknown): PolicyRpcPlanDto {
  const record = requireRecord(value, "plan_policy_rpc", "$");
  const root = requireRecord(
    requireField(record, "root", "$.root", "plan_policy_rpc"),
    "plan_policy_rpc",
    "$.root",
  );
  return {
    request_id: requireString(
      record,
      "request_id",
      "$.request_id",
      "plan_policy_rpc",
    ),
    root: {
      chain_id: requireNumber(root, "chain_id", "$.root.chain_id"),
      from: requireString(root, "from", "$.root.from", "plan_policy_rpc"),
      to: requireString(root, "to", "$.root.to", "plan_policy_rpc"),
      value_wei: requireString(
        root,
        "value_wei",
        "$.root.value_wei",
        "plan_policy_rpc",
      ),
      ...(hasOwn(root, "block_timestamp")
        ? {
            block_timestamp: requireNumber(
              root,
              "block_timestamp",
              "$.root.block_timestamp",
            ),
          }
        : {}),
    },
    envelopes: parseUnknownArray(record, "envelopes", "$.envelopes"),
    calls: parseArrayField(
      record,
      "calls",
      "$.calls",
      "plan_policy_rpc",
      parsePolicyRpcCall,
    ),
    manifest_set_hash: requireString(
      record,
      "manifest_set_hash",
      "$.manifest_set_hash",
      "plan_policy_rpc",
    ),
    schema_hash: requireString(
      record,
      "schema_hash",
      "$.schema_hash",
      "plan_policy_rpc",
    ),
    diagnostics: parseStringArray(record, "diagnostics", "$.diagnostics"),
  };
}

function parsePolicyRpcCall(
  value: unknown,
  path: string,
  exportName: string,
): PolicyRpcCallDto {
  const record = requireRecord(value, exportName, path);
  return {
    id: requireString(record, "id", `${path}.id`, exportName),
    method: requireString(record, "method", `${path}.method`, exportName),
    params: requireField(record, "params", `${path}.params`, exportName),
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

function requireNumber(
  record: JsonRecord,
  key: string,
  path: string,
): number {
  const value = requireField(record, key, path, "plan_policy_rpc");
  if (typeof value === "number" && Number.isFinite(value)) return value;
  fail("plan_policy_rpc", path, "expected finite number", value);
}

function parseUnknownArray(
  record: JsonRecord,
  key: string,
  path: string,
): readonly unknown[] {
  const value = requireField(record, key, path, "plan_policy_rpc");
  if (Array.isArray(value)) return value;
  fail("plan_policy_rpc", path, "expected array", value);
}

function parseStringArray(
  record: JsonRecord,
  key: string,
  path: string,
): readonly string[] {
  const value = parseUnknownArray(record, key, path);
  for (const [index, item] of value.entries()) {
    if (typeof item !== "string") {
      fail("plan_policy_rpc", `${path}[${index}]`, "expected string", item);
    }
  }
  return value as readonly string[];
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
