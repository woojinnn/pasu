export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | JsonObject;

export interface JsonObject {
  [key: string]: JsonValue;
}

export type FetchLike = (input: string | URL, init?: RequestInit) => Promise<Response>;
export type NowMs = () => number;

export interface PolicyRpcCall {
  id: string;
  method: string;
  params: JsonObject;
}

export interface PolicyRpcRequest {
  request_id: string;
  calls: PolicyRpcCall[];
}

export interface RpcErrorBody {
  code: string;
  message: string;
}

export interface RpcSuccessResult {
  id: string;
  ok: true;
  result: JsonObject;
}

export interface RpcFailureResult {
  id: string;
  ok: false;
  error: RpcErrorBody;
}

export type RpcResult = RpcSuccessResult | RpcFailureResult;

export interface PolicyRpcResponse {
  request_id: string;
  results: RpcResult[];
}

export interface OracleUsdValueParams {
  chain_id: number;
  address: string;
  amount: string;
  decimals: number;
}

export interface UsdValuation extends JsonObject {
  value: string;
  asOfTs: number;
  staleSec: number;
  sources: string[];
}

export class RpcMethodError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "RpcMethodError";
    this.code = code;
  }
}
