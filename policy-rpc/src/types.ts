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

/**
 * Whitelist of price-source identifiers `oracle.usd_value` accepts.
 * Mirrored in the method's catalog entry (`enum_`) and in the daemon's
 * source dispatcher. Adding a new source here means:
 *   1. Add a client class beside `CoinGeckoClient`.
 *   2. Register it in `createOracleUsdValueMethod`'s `sources` map.
 *   3. Add the string to this union AND to `oracleUsdValueCatalog.params.source.enum_`.
 * The lockstep is by design — drift would let manifests reference a
 * source the daemon can't actually serve.
 */
export type OracleUsdValueSource = "coingecko";

export interface OracleUsdValueParams {
  chain_id: number;
  address: string;
  amount: string;
  decimals: number;
  /**
   * Caller-requested price source. Defaults to `"coingecko"` when the
   * manifest omits the field, so existing manifests written before the
   * source param landed keep working without edit.
   */
  source: OracleUsdValueSource;
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
