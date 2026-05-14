import { createOracleUsdValueMethod, type OracleUsdValueMethodOptions } from "./oracle-usd-value.js";
import {
  RpcMethodError,
  type JsonObject,
  type PolicyRpcCall,
  type RpcResult,
} from "../types.js";

export type RpcMethod = (params: unknown) => Promise<JsonObject>;

export interface MethodRegistry {
  listMethods(): string[];
  execute(call: PolicyRpcCall): Promise<RpcResult>;
}

export interface MethodRegistryOptions extends OracleUsdValueMethodOptions {}

export function createMethodRegistry(options: MethodRegistryOptions = {}): MethodRegistry {
  const methods = new Map<string, RpcMethod>([
    ["oracle.usd_value", createOracleUsdValueMethod(options) as RpcMethod],
  ]);

  return {
    listMethods: () => [...methods.keys()].sort(),

    async execute(call: PolicyRpcCall): Promise<RpcResult> {
      const method = methods.get(call.method);

      if (!method) {
        return {
          id: call.id,
          ok: false,
          error: {
            code: "method_not_found",
            message: `Unknown method ${call.method}`,
          },
        };
      }

      try {
        const result = await method(call.params);

        return {
          id: call.id,
          ok: true,
          result,
        };
      } catch (error) {
        const methodError = normalizeMethodError(error);

        return {
          id: call.id,
          ok: false,
          error: {
            code: methodError.code,
            message: methodError.message,
          },
        };
      }
    },
  };
}

function normalizeMethodError(error: unknown): RpcMethodError {
  if (error instanceof RpcMethodError) {
    return error;
  }

  if (error instanceof Error) {
    return new RpcMethodError("internal_error", error.message);
  }

  return new RpcMethodError("internal_error", "Unknown method error");
}
