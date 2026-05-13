type JsonRecord = Record<string, unknown>;

const VERDICT_KINDS = ["pass", "fail", "warn"] as const;
const POLICY_SEVERITIES = ["deny", "warn"] as const;
const POLICY_REQUEST_ORIGINS = ["action", "tx", "engine_error"] as const;

export type Severity = (typeof POLICY_SEVERITIES)[number];
export type Origin = (typeof POLICY_REQUEST_ORIGINS)[number];

export type VerdictDto = PassVerdictDto | WarnVerdictDto | FailVerdictDto;

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
