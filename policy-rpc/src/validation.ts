import { wrappedNativeAddressForChain } from "./chain-config.js";
import { RpcMethodError, type JsonObject, type PolicyRpcCall, type PolicyRpcRequest } from "./types.js";

export class ValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ValidationError";
  }
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parsePolicyRpcRequest(value: unknown): PolicyRpcRequest {
  const root = expectRecord(value, "request body");
  const requestId = expectNonEmptyString(root.request_id, "request_id");

  if (!Array.isArray(root.calls)) {
    throw new ValidationError("calls must be an array");
  }

  return {
    request_id: requestId,
    calls: root.calls.map(parsePolicyRpcCall),
  };
}

export function parseOracleUsdValueParams(value: unknown) {
  try {
    return parseOracleUsdValueParamsInner(value);
  } catch (error) {
    if (error instanceof RpcMethodError) {
      throw error;
    }
    if (error instanceof ValidationError) {
      throw new RpcMethodError("invalid_params", error.message);
    }
    throw error;
  }
}

/** Accepted price sources. Adding one means updating both this list,
 * `OracleUsdValueSource` in types.ts, and the method's catalog enum. */
const ORACLE_USD_VALUE_SOURCES = ["coingecko", "chainlink"] as const;

function parseOracleUsdValueParamsInner(value: unknown) {
  const params = expectRecord(value, "oracle.usd_value params");
  const chainId = expectInteger(params.chain_id, "chain_id");
  const amount = expectUnsignedIntegerString(params.amount, "amount");
  const { address, decimals } =
    params.asset === undefined
      ? {
          address: expectAddress(params.address, "address"),
          decimals: expectInteger(params.decimals, "decimals"),
        }
      : parseAssetParams(params.asset, chainId, params.decimals);

  if (decimals < 0 || decimals > 255) {
    throw new RpcMethodError("invalid_params", "decimals must be between 0 and 255");
  }

  // Optional with default — keeps pre-source-param manifests working.
  // When present, MUST match the whitelist; we reject unknown sources
  // up front so the caller gets a clear error instead of a generic
  // upstream failure deeper in the dispatcher.
  const source =
    params.source === undefined
      ? "coingecko"
      : expectEnum(
          params.source,
          "source",
          ORACLE_USD_VALUE_SOURCES as readonly string[],
        );

  return {
    chain_id: chainId,
    address,
    amount,
    decimals,
    source: source as (typeof ORACLE_USD_VALUE_SOURCES)[number],
  };
}

function expectEnum(
  value: unknown,
  field: string,
  allowed: readonly string[],
): string {
  if (typeof value !== "string") {
    throw new ValidationError(`${field} must be a string`);
  }
  if (!allowed.includes(value)) {
    throw new ValidationError(
      `${field} must be one of [${allowed.join(", ")}] (got: ${value})`,
    );
  }
  return value;
}

function parseAssetParams(
  value: unknown,
  chainId: number,
  decimalsOverride: unknown,
): { address: string; decimals: number } {
  const asset = expectRecord(value, "asset");
  const kind = expectNonEmptyString(asset.kind, "asset.kind");
  const decimals = expectInteger(decimalsOverride ?? asset.decimals, "asset.decimals");

  if (kind === "native") {
    return {
      address: wrappedNativeAddressForChain(chainId),
      decimals,
    };
  }

  if (kind !== "erc20") {
    throw new RpcMethodError("invalid_params", "asset.kind must be erc20 or native");
  }

  return {
    address: expectAddress(asset.address, "asset.address"),
    decimals,
  };
}

function parsePolicyRpcCall(value: unknown): PolicyRpcCall {
  const call = expectRecord(value, "call");

  return {
    id: expectNonEmptyString(call.id, "call.id"),
    method: expectNonEmptyString(call.method, "call.method"),
    params: expectJsonObject(call.params, "call.params"),
  };
}

function expectRecord(value: unknown, label: string): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new ValidationError(`${label} must be an object`);
  }

  return value;
}

function expectJsonObject(value: unknown, label: string): JsonObject {
  if (!isRecord(value)) {
    throw new ValidationError(`${label} must be an object`);
  }

  return value as JsonObject;
}

function expectNonEmptyString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.trim() === "") {
    throw new ValidationError(`${label} must be a non-empty string`);
  }

  return value;
}

function expectInteger(value: unknown, label: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value)) {
    throw new RpcMethodError("invalid_params", `${label} must be a safe integer`);
  }

  return value;
}

function expectAddress(value: unknown, label: string): string {
  const address = expectNonEmptyString(value, label).toLowerCase();

  if (!/^0x[0-9a-f]{40}$/.test(address)) {
    throw new RpcMethodError("invalid_params", `${label} must be an EVM address`);
  }

  return address;
}

function expectUnsignedIntegerString(value: unknown, label: string): string {
  const text = expectNonEmptyString(value, label);

  if (!/^(0|[1-9][0-9]*)$/.test(text)) {
    throw new RpcMethodError("invalid_params", `${label} must be an unsigned integer string`);
  }

  return text;
}
