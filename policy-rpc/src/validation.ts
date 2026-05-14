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
  const params = expectRecord(value, "oracle.usd_value params");
  const chainId = expectInteger(params.chain_id, "chain_id");
  const address = expectAddress(params.address, "address");
  const amount = expectUnsignedIntegerString(params.amount, "amount");
  const decimals = expectInteger(params.decimals, "decimals");

  if (decimals < 0 || decimals > 255) {
    throw new RpcMethodError("invalid_params", "decimals must be between 0 and 255");
  }

  return {
    chain_id: chainId,
    address,
    amount,
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
