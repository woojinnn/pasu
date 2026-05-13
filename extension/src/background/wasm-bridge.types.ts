type JsonRecord = Record<string, unknown>;

const ACTION_VARIANTS = [
  "dex",
  "other",
  "permit2",
  "eip2612",
  "eip712Other",
] as const;

const ORACLE_REQUIREMENT_KINDS = ["input", "minOutput"] as const;
const PERMIT2_KINDS = [
  "PermitSingle",
  "PermitBatch",
  "PermitTransferFrom",
  "PermitBatchTransferFrom",
  "PermitWitnessTransferFrom",
  "PermitBatchWitnessTransferFrom",
] as const;
const VERDICT_KINDS = ["pass", "fail", "warn"] as const;
const POLICY_SEVERITIES = ["deny", "warn"] as const;
const POLICY_REQUEST_ORIGINS = ["action", "tx", "engine_error"] as const;

export type ParsedAction =
  | ParsedDexAction
  | ParsedOtherAction
  | ParsedPermit2Action
  | ParsedEip2612Action
  | ParsedEip712OtherAction;

export interface ParsedDexAction extends Readonly<Record<string, unknown>> {
  readonly dex: DexAction;
}

export interface ParsedOtherAction extends Readonly<Record<string, unknown>> {
  readonly other: OtherAction;
}

export interface ParsedPermit2Action extends Readonly<Record<string, unknown>> {
  readonly permit2: Permit2Action;
}

export interface ParsedEip2612Action extends Readonly<Record<string, unknown>> {
  readonly eip2612: Eip2612Action;
}

export interface ParsedEip712OtherAction
  extends Readonly<Record<string, unknown>> {
  readonly eip712Other: Eip712OtherAction;
}

export interface Token {
  readonly chain_id: number;
  readonly address: string;
  readonly symbol: string;
  readonly decimals: number;
  readonly is_native: boolean;
}

export interface UsdValuation {
  readonly value: string;
  readonly as_of_ts: number;
  readonly sources: readonly string[];
  readonly stale_sec: number;
}

export interface OracleRequirement {
  readonly kind: (typeof ORACLE_REQUIREMENT_KINDS)[number];
  readonly token: Token;
  readonly raw_amount: string;
}

export interface WindowStatsContext {
  readonly swap_volume_usd_24h: string | null;
  readonly swap_count_24h: number | null;
}

export interface DexFacts {
  readonly protocol_ids: readonly string[];
  readonly input_tokens: readonly Token[];
  readonly output_tokens: readonly Token[];
  readonly total_input_usd: UsdValuation | null;
  readonly total_min_output_usd: UsdValuation | null;
  readonly max_fee_bps: number | null;
  readonly has_zero_min_output: boolean;
  readonly has_external_recipient: boolean;
  readonly total_input_fraction_of_portfolio_bps: number | null;
  readonly allowances_cover_inputs: boolean | null;
  readonly window_stats: WindowStatsContext | null;
}

export interface DexTrace {
  readonly steps: readonly string[];
}

export interface DexAction {
  readonly actor: string;
  readonly target: string;
  readonly value_wei: string;
  readonly facts: DexFacts;
  readonly oracle_requirements: readonly OracleRequirement[];
  readonly trace: DexTrace;
}

export interface OtherAction {
  readonly actor: string;
  readonly target: string;
  readonly selector: string;
  readonly value_wei: string;
  readonly raw_calldata: string;
}

export type Permit2PermitKind = (typeof PERMIT2_KINDS)[number];

export interface Permit2Approval {
  readonly token: Token;
  readonly amount: string;
  readonly expiration: number;
  readonly nonce: string;
}

export interface Permit2Action {
  readonly signer: string;
  readonly chain_id: number;
  readonly domain_chain_id: number;
  readonly verifying_contract: string;
  readonly primary_type: string;
  readonly permit_kind: Permit2PermitKind;
  readonly spender: string;
  readonly token: Token;
  readonly amount: string;
  readonly expiration: number;
  readonly sig_deadline: number;
  readonly nonce: string;
  readonly approvals: readonly Permit2Approval[];
  readonly is_unlimited: boolean;
  readonly nonce_valid: boolean;
  readonly witness_present: boolean;
  readonly total_approved_usd: UsdValuation | null;
}

export interface Eip2612Action {
  readonly signer: string;
  readonly owner: string;
  readonly chain_id: number;
  readonly domain_chain_id: number;
  readonly verifying_contract: string;
  readonly primary_type: string;
  readonly spender: string;
  readonly token: Token;
  readonly is_unlimited: boolean;
  readonly nonce_valid: boolean;
  readonly value: string;
  readonly deadline: number;
  readonly nonce: string;
  readonly total_approved_usd: UsdValuation | null;
}

export interface Eip712OtherAction {
  readonly signer: string;
  readonly chain_id: number;
  readonly domain_chain_id: number;
  readonly verifying_contract: string;
  readonly primary_type: string;
  readonly domain_name: string | null;
  readonly domain_version: string | null;
  readonly domain_salt: string | null;
  readonly types_json: string;
  readonly message_json: string;
}

export interface Tier1Plan {
  readonly tokens_for_oracle: readonly Token[];
  readonly balances: readonly BalanceRequirement[];
  readonly allowances: readonly AllowanceRequirement[];
  readonly clock_required: boolean;
  readonly sig_oracle_requirements: readonly OracleRequirement[];
}

export interface BalanceRequirement {
  readonly owner: string;
  readonly token: Token;
}

export interface AllowanceRequirement {
  readonly owner: string;
  readonly token: Token;
  readonly spender: string;
}

export interface WindowKeys {
  readonly keys: readonly WindowKey[];
}

export interface WindowKey {
  readonly actor: string;
  readonly name: string;
}

export type VerdictDto = PassVerdictDto | WarnVerdictDto | FailVerdictDto;

export interface PassVerdictDto {
  readonly kind: "pass";
  readonly matched?: undefined;
}

export interface WarnVerdictDto {
  readonly kind: "warn";
  readonly matched: readonly MatchedPolicy[];
}

export interface FailVerdictDto {
  readonly kind: "fail";
  readonly matched: readonly MatchedPolicy[];
}

export interface MatchedPolicy {
  readonly policy_id: string;
  readonly reason: string | null;
  readonly severity: (typeof POLICY_SEVERITIES)[number];
  readonly origin: (typeof POLICY_REQUEST_ORIGINS)[number];
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

export function parseAction(value: unknown): ParsedAction {
  const record = requireRecord(value, "buildActionForRequest", "$");
  const keys = Object.keys(record);
  const variantKeys = ACTION_VARIANTS.filter((key) => hasOwn(record, key));
  if (keys.length !== 1 || variantKeys.length !== 1) {
    fail(
      "buildActionForRequest",
      "$",
      `expected exactly one action variant (${ACTION_VARIANTS.join(", ")})`,
      value,
    );
  }

  const variant = variantKeys[0];
  switch (variant) {
    case "dex":
      return {
        dex: parseDexAction(record.dex, "$.dex", "buildActionForRequest"),
      };
    case "other":
      return {
        other: parseOtherAction(
          record.other,
          "$.other",
          "buildActionForRequest",
        ),
      };
    case "permit2":
      return {
        permit2: parsePermit2Action(
          record.permit2,
          "$.permit2",
          "buildActionForRequest",
        ),
      };
    case "eip2612":
      return {
        eip2612: parseEip2612Action(
          record.eip2612,
          "$.eip2612",
          "buildActionForRequest",
        ),
      };
    case "eip712Other":
      return {
        eip712Other: parseEip712OtherAction(
          record.eip712Other,
          "$.eip712Other",
          "buildActionForRequest",
        ),
      };
  }
}

export function parseTier1Plan(value: unknown): Tier1Plan {
  const record = requireRecord(value, "tier1FactPlan", "$");
  return {
    tokens_for_oracle: parseArrayField(
      record,
      "tokens_for_oracle",
      "$.tokens_for_oracle",
      "tier1FactPlan",
      parseToken,
    ),
    balances: parseArrayField(
      record,
      "balances",
      "$.balances",
      "tier1FactPlan",
      parseBalanceRequirement,
    ),
    allowances: parseArrayField(
      record,
      "allowances",
      "$.allowances",
      "tier1FactPlan",
      parseAllowanceRequirement,
    ),
    clock_required: requireBoolean(
      record,
      "clock_required",
      "$.clock_required",
      "tier1FactPlan",
    ),
    sig_oracle_requirements: parseArrayField(
      record,
      "sig_oracle_requirements",
      "$.sig_oracle_requirements",
      "tier1FactPlan",
      parseOracleRequirement,
    ),
  };
}

export function parseWindowKeys(value: unknown): WindowKeys {
  const record = requireRecord(value, "tier2WindowKeys", "$");
  return {
    keys: parseArrayField(
      record,
      "keys",
      "$.keys",
      "tier2WindowKeys",
      parseWindowKey,
    ),
  };
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

function parseDexAction(
  value: unknown,
  path: string,
  exportName: string,
): DexAction {
  const record = requireRecord(value, exportName, path);
  return {
    actor: requireString(record, "actor", `${path}.actor`, exportName),
    target: requireString(record, "target", `${path}.target`, exportName),
    value_wei: requireString(
      record,
      "value_wei",
      `${path}.value_wei`,
      exportName,
    ),
    facts: parseDexFacts(
      requireField(record, "facts", `${path}.facts`, exportName),
      `${path}.facts`,
      exportName,
    ),
    oracle_requirements: parseArrayField(
      record,
      "oracle_requirements",
      `${path}.oracle_requirements`,
      exportName,
      parseOracleRequirement,
    ),
    trace: parseDexTrace(
      requireField(record, "trace", `${path}.trace`, exportName),
      `${path}.trace`,
      exportName,
    ),
  };
}

function parseOtherAction(
  value: unknown,
  path: string,
  exportName: string,
): OtherAction {
  const record = requireRecord(value, exportName, path);
  return {
    actor: requireString(record, "actor", `${path}.actor`, exportName),
    target: requireString(record, "target", `${path}.target`, exportName),
    selector: requireString(record, "selector", `${path}.selector`, exportName),
    value_wei: requireString(
      record,
      "value_wei",
      `${path}.value_wei`,
      exportName,
    ),
    raw_calldata: requireString(
      record,
      "raw_calldata",
      `${path}.raw_calldata`,
      exportName,
    ),
  };
}

function parsePermit2Action(
  value: unknown,
  path: string,
  exportName: string,
): Permit2Action {
  const record = requireRecord(value, exportName, path);
  return {
    signer: requireString(record, "signer", `${path}.signer`, exportName),
    chain_id: requireUnsignedInteger(
      record,
      "chain_id",
      `${path}.chain_id`,
      exportName,
    ),
    domain_chain_id: requireUnsignedInteger(
      record,
      "domain_chain_id",
      `${path}.domain_chain_id`,
      exportName,
    ),
    verifying_contract: requireString(
      record,
      "verifying_contract",
      `${path}.verifying_contract`,
      exportName,
    ),
    primary_type: requireString(
      record,
      "primary_type",
      `${path}.primary_type`,
      exportName,
    ),
    permit_kind: requireOneOf(
      record,
      "permit_kind",
      `${path}.permit_kind`,
      exportName,
      PERMIT2_KINDS,
    ),
    spender: requireString(record, "spender", `${path}.spender`, exportName),
    token: parseToken(
      requireField(record, "token", `${path}.token`, exportName),
      `${path}.token`,
      exportName,
    ),
    amount: requireString(record, "amount", `${path}.amount`, exportName),
    expiration: requireUnsignedInteger(
      record,
      "expiration",
      `${path}.expiration`,
      exportName,
    ),
    sig_deadline: requireUnsignedInteger(
      record,
      "sig_deadline",
      `${path}.sig_deadline`,
      exportName,
    ),
    nonce: requireString(record, "nonce", `${path}.nonce`, exportName),
    approvals: parseArrayField(
      record,
      "approvals",
      `${path}.approvals`,
      exportName,
      parsePermit2Approval,
    ),
    is_unlimited: requireBoolean(
      record,
      "is_unlimited",
      `${path}.is_unlimited`,
      exportName,
    ),
    nonce_valid: requireBoolean(
      record,
      "nonce_valid",
      `${path}.nonce_valid`,
      exportName,
    ),
    witness_present: requireBoolean(
      record,
      "witness_present",
      `${path}.witness_present`,
      exportName,
    ),
    total_approved_usd: parseNullableUsdValuation(
      requireField(
        record,
        "total_approved_usd",
        `${path}.total_approved_usd`,
        exportName,
      ),
      `${path}.total_approved_usd`,
      exportName,
    ),
  };
}

function parseEip2612Action(
  value: unknown,
  path: string,
  exportName: string,
): Eip2612Action {
  const record = requireRecord(value, exportName, path);
  return {
    signer: requireString(record, "signer", `${path}.signer`, exportName),
    owner: requireString(record, "owner", `${path}.owner`, exportName),
    chain_id: requireUnsignedInteger(
      record,
      "chain_id",
      `${path}.chain_id`,
      exportName,
    ),
    domain_chain_id: requireUnsignedInteger(
      record,
      "domain_chain_id",
      `${path}.domain_chain_id`,
      exportName,
    ),
    verifying_contract: requireString(
      record,
      "verifying_contract",
      `${path}.verifying_contract`,
      exportName,
    ),
    primary_type: requireString(
      record,
      "primary_type",
      `${path}.primary_type`,
      exportName,
    ),
    spender: requireString(record, "spender", `${path}.spender`, exportName),
    token: parseToken(
      requireField(record, "token", `${path}.token`, exportName),
      `${path}.token`,
      exportName,
    ),
    is_unlimited: requireBoolean(
      record,
      "is_unlimited",
      `${path}.is_unlimited`,
      exportName,
    ),
    nonce_valid: requireBoolean(
      record,
      "nonce_valid",
      `${path}.nonce_valid`,
      exportName,
    ),
    value: requireString(record, "value", `${path}.value`, exportName),
    deadline: requireUnsignedInteger(
      record,
      "deadline",
      `${path}.deadline`,
      exportName,
    ),
    nonce: requireString(record, "nonce", `${path}.nonce`, exportName),
    total_approved_usd: parseNullableUsdValuation(
      requireField(
        record,
        "total_approved_usd",
        `${path}.total_approved_usd`,
        exportName,
      ),
      `${path}.total_approved_usd`,
      exportName,
    ),
  };
}

function parseEip712OtherAction(
  value: unknown,
  path: string,
  exportName: string,
): Eip712OtherAction {
  const record = requireRecord(value, exportName, path);
  return {
    signer: requireString(record, "signer", `${path}.signer`, exportName),
    chain_id: requireUnsignedInteger(
      record,
      "chain_id",
      `${path}.chain_id`,
      exportName,
    ),
    domain_chain_id: requireUnsignedInteger(
      record,
      "domain_chain_id",
      `${path}.domain_chain_id`,
      exportName,
    ),
    verifying_contract: requireString(
      record,
      "verifying_contract",
      `${path}.verifying_contract`,
      exportName,
    ),
    primary_type: requireString(
      record,
      "primary_type",
      `${path}.primary_type`,
      exportName,
    ),
    domain_name: requireNullableString(
      record,
      "domain_name",
      `${path}.domain_name`,
      exportName,
    ),
    domain_version: requireNullableString(
      record,
      "domain_version",
      `${path}.domain_version`,
      exportName,
    ),
    domain_salt: requireNullableString(
      record,
      "domain_salt",
      `${path}.domain_salt`,
      exportName,
    ),
    types_json: requireString(
      record,
      "types_json",
      `${path}.types_json`,
      exportName,
    ),
    message_json: requireString(
      record,
      "message_json",
      `${path}.message_json`,
      exportName,
    ),
  };
}

function parseDexFacts(
  value: unknown,
  path: string,
  exportName: string,
): DexFacts {
  const record = requireRecord(value, exportName, path);
  return {
    protocol_ids: parseStringArrayField(
      record,
      "protocol_ids",
      `${path}.protocol_ids`,
      exportName,
    ),
    input_tokens: parseArrayField(
      record,
      "input_tokens",
      `${path}.input_tokens`,
      exportName,
      parseToken,
    ),
    output_tokens: parseArrayField(
      record,
      "output_tokens",
      `${path}.output_tokens`,
      exportName,
      parseToken,
    ),
    total_input_usd: parseNullableUsdValuation(
      requireField(
        record,
        "total_input_usd",
        `${path}.total_input_usd`,
        exportName,
      ),
      `${path}.total_input_usd`,
      exportName,
    ),
    total_min_output_usd: parseNullableUsdValuation(
      requireField(
        record,
        "total_min_output_usd",
        `${path}.total_min_output_usd`,
        exportName,
      ),
      `${path}.total_min_output_usd`,
      exportName,
    ),
    max_fee_bps: requireNullableUnsignedInteger(
      record,
      "max_fee_bps",
      `${path}.max_fee_bps`,
      exportName,
    ),
    has_zero_min_output: requireBoolean(
      record,
      "has_zero_min_output",
      `${path}.has_zero_min_output`,
      exportName,
    ),
    has_external_recipient: requireBoolean(
      record,
      "has_external_recipient",
      `${path}.has_external_recipient`,
      exportName,
    ),
    total_input_fraction_of_portfolio_bps: requireNullableUnsignedInteger(
      record,
      "total_input_fraction_of_portfolio_bps",
      `${path}.total_input_fraction_of_portfolio_bps`,
      exportName,
    ),
    allowances_cover_inputs: requireNullableBoolean(
      record,
      "allowances_cover_inputs",
      `${path}.allowances_cover_inputs`,
      exportName,
    ),
    window_stats: parseNullableWindowStats(
      requireField(record, "window_stats", `${path}.window_stats`, exportName),
      `${path}.window_stats`,
      exportName,
    ),
  };
}

function parseDexTrace(
  value: unknown,
  path: string,
  exportName: string,
): DexTrace {
  const record = requireRecord(value, exportName, path);
  return {
    steps: parseStringArrayField(record, "steps", `${path}.steps`, exportName),
  };
}

function parseToken(value: unknown, path: string, exportName: string): Token {
  const record = requireRecord(value, exportName, path);
  return {
    chain_id: requireUnsignedInteger(
      record,
      "chain_id",
      `${path}.chain_id`,
      exportName,
    ),
    address: requireString(record, "address", `${path}.address`, exportName),
    symbol: requireString(record, "symbol", `${path}.symbol`, exportName),
    decimals: requireUnsignedInteger(
      record,
      "decimals",
      `${path}.decimals`,
      exportName,
    ),
    is_native: requireBoolean(
      record,
      "is_native",
      `${path}.is_native`,
      exportName,
    ),
  };
}

function parseUsdValuation(
  value: unknown,
  path: string,
  exportName: string,
): UsdValuation {
  const record = requireRecord(value, exportName, path);
  return {
    value: requireString(record, "value", `${path}.value`, exportName),
    as_of_ts: requireUnsignedInteger(
      record,
      "as_of_ts",
      `${path}.as_of_ts`,
      exportName,
    ),
    sources: parseStringArrayField(
      record,
      "sources",
      `${path}.sources`,
      exportName,
    ),
    stale_sec: requireUnsignedInteger(
      record,
      "stale_sec",
      `${path}.stale_sec`,
      exportName,
    ),
  };
}

function parseNullableUsdValuation(
  value: unknown,
  path: string,
  exportName: string,
): UsdValuation | null {
  if (value === null) return null;
  return parseUsdValuation(value, path, exportName);
}

function parseWindowStats(
  value: unknown,
  path: string,
  exportName: string,
): WindowStatsContext {
  const record = requireRecord(value, exportName, path);
  return {
    swap_volume_usd_24h: requireNullableString(
      record,
      "swap_volume_usd_24h",
      `${path}.swap_volume_usd_24h`,
      exportName,
    ),
    swap_count_24h: requireNullableUnsignedInteger(
      record,
      "swap_count_24h",
      `${path}.swap_count_24h`,
      exportName,
    ),
  };
}

function parseNullableWindowStats(
  value: unknown,
  path: string,
  exportName: string,
): WindowStatsContext | null {
  if (value === null) return null;
  return parseWindowStats(value, path, exportName);
}

function parsePermit2Approval(
  value: unknown,
  path: string,
  exportName: string,
): Permit2Approval {
  const record = requireRecord(value, exportName, path);
  return {
    token: parseToken(
      requireField(record, "token", `${path}.token`, exportName),
      `${path}.token`,
      exportName,
    ),
    amount: requireString(record, "amount", `${path}.amount`, exportName),
    expiration: requireUnsignedInteger(
      record,
      "expiration",
      `${path}.expiration`,
      exportName,
    ),
    nonce: requireString(record, "nonce", `${path}.nonce`, exportName),
  };
}

function parseBalanceRequirement(
  value: unknown,
  path: string,
  exportName: string,
): BalanceRequirement {
  const record = requireRecord(value, exportName, path);
  return {
    owner: requireString(record, "owner", `${path}.owner`, exportName),
    token: parseToken(
      requireField(record, "token", `${path}.token`, exportName),
      `${path}.token`,
      exportName,
    ),
  };
}

function parseAllowanceRequirement(
  value: unknown,
  path: string,
  exportName: string,
): AllowanceRequirement {
  const record = requireRecord(value, exportName, path);
  return {
    owner: requireString(record, "owner", `${path}.owner`, exportName),
    token: parseToken(
      requireField(record, "token", `${path}.token`, exportName),
      `${path}.token`,
      exportName,
    ),
    spender: requireString(record, "spender", `${path}.spender`, exportName),
  };
}

function parseOracleRequirement(
  value: unknown,
  path: string,
  exportName: string,
): OracleRequirement {
  const record = requireRecord(value, exportName, path);
  return {
    kind: requireOneOf(
      record,
      "kind",
      `${path}.kind`,
      exportName,
      ORACLE_REQUIREMENT_KINDS,
    ),
    token: parseToken(
      requireField(record, "token", `${path}.token`, exportName),
      `${path}.token`,
      exportName,
    ),
    raw_amount: requireString(
      record,
      "raw_amount",
      `${path}.raw_amount`,
      exportName,
    ),
  };
}

function parseWindowKey(
  value: unknown,
  path: string,
  exportName: string,
): WindowKey {
  const record = requireRecord(value, exportName, path);
  return {
    actor: requireString(record, "actor", `${path}.actor`, exportName),
    name: requireString(record, "name", `${path}.name`, exportName),
  };
}

function parseMatchedPolicy(
  value: unknown,
  path: string,
  exportName: string,
): MatchedPolicy {
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

function parseStringArrayField(
  record: JsonRecord,
  key: string,
  path: string,
  exportName: string,
): readonly string[] {
  return parseArrayField(record, key, path, exportName, (item, itemPath) => {
    if (typeof item !== "string") {
      fail(exportName, itemPath, "expected string", item);
    }
    return item;
  });
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

function requireBoolean(
  record: JsonRecord,
  key: string,
  path: string,
  exportName: string,
): boolean {
  const value = requireField(record, key, path, exportName);
  if (typeof value === "boolean") return value;
  fail(exportName, path, "expected boolean", value);
}

function requireNullableBoolean(
  record: JsonRecord,
  key: string,
  path: string,
  exportName: string,
): boolean | null {
  const value = requireField(record, key, path, exportName);
  if (value === null || typeof value === "boolean") return value;
  fail(exportName, path, "expected boolean or null", value);
}

function requireUnsignedInteger(
  record: JsonRecord,
  key: string,
  path: string,
  exportName: string,
): number {
  const value = requireField(record, key, path, exportName);
  if (isUnsignedInteger(value)) return value;
  fail(exportName, path, "expected unsigned integer", value);
}

function requireNullableUnsignedInteger(
  record: JsonRecord,
  key: string,
  path: string,
  exportName: string,
): number | null {
  const value = requireField(record, key, path, exportName);
  if (value === null || isUnsignedInteger(value)) return value;
  fail(exportName, path, "expected unsigned integer or null", value);
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

function isUnsignedInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0;
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
