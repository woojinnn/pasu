// Pure, environment-agnostic RPC field extractor.
//
// Used by:
//   - userscript/scopeball.user.js  (Phase 1)
//   - chrome extension injected.js  (Phase 2)
//
// No window, document, or transport assumptions — caller passes in
// `origin` and `currentChainId` explicitly.

export type RpcRequest = {
  method: string;
  params?: unknown[] | Record<string, unknown>;
};

export type ExtractedRpcFields = {
  method: string;
  origin?: string;
  currentChainId?: string;
  primaryChainId?: string;
  chainIds: string[];
  addresses: string[];
  from?: string;
  to?: string;
  value?: string;
  calldata: string[];
  gasFields: Record<string, string>;
  rawParams: unknown;
  parsedTypedData?: unknown;
};

const ADDRESS_KEYS = new Set([
  "address", "account", "owner", "spender",
  "recipient", "sender", "verifyingContract", "contractAddress",
]);
const CALLDATA_KEYS = new Set(["data", "input", "calldata", "callData"]);
const GAS_KEYS = new Set([
  "gas", "gasLimit", "gasPrice", "maxFeePerGas", "maxPriorityFeePerGas",
]);

export function isAddress(v: unknown): v is string {
  return typeof v === "string" && /^0x[a-fA-F0-9]{40}$/.test(v);
}
export function isHexData(v: unknown): v is string {
  return typeof v === "string" && /^0x[a-fA-F0-9]*$/.test(v);
}
export function looksLikeCalldata(v: unknown): v is string {
  return typeof v === "string" && /^0x[a-fA-F0-9]+$/.test(v) && v.length >= 10;
}
export function normalizeChainId(v: unknown): string | null {
  if (typeof v === "string") {
    if (/^0x[0-9a-fA-F]+$/.test(v)) return v.toLowerCase();
    const n = Number(v);
    return Number.isFinite(n) ? "0x" + n.toString(16) : null;
  }
  if (typeof v === "number") return "0x" + v.toString(16);
  return null;
}

export function extractRpcFields(
  args: RpcRequest,
  opts?: { origin?: string; currentChainId?: string },
): ExtractedRpcFields {
  const result: ExtractedRpcFields = {
    method: args.method,
    origin: opts?.origin,
    currentChainId: opts?.currentChainId,
    primaryChainId: undefined,
    chainIds: [],
    addresses: [],
    from: undefined,
    to: undefined,
    value: undefined,
    calldata: [],
    gasFields: {},
    rawParams: args.params ?? [],
    parsedTypedData: undefined,
  };

  const visit = (value: unknown, key?: string): void => {
    if (value == null) return;

    // typedData JSON-string fallback: if a string looks like an object literal,
    // parse and recurse so we can pick up domain.chainId / verifyingContract.
    if (typeof value === "string") {
      const t = value.trim();
      if (t.startsWith("{") && t.endsWith("}")) {
        try {
          const parsed = JSON.parse(t);
          if (key === undefined || key === "0" || key === "1") {
            result.parsedTypedData = parsed;
          }
          visit(parsed);
          return;
        } catch {
          /* not JSON, fall through */
        }
      }
    }

    if (typeof value === "string" || typeof value === "number") {
      if (key === "chainId") {
        const c = normalizeChainId(value);
        if (c) result.chainIds.push(c);
      }
      if (key === "from" && isAddress(value)) {
        result.from = value;
        result.addresses.push(value);
      }
      if (key === "to" && isAddress(value)) {
        result.to = value;
        result.addresses.push(value);
      }
      if (key && ADDRESS_KEYS.has(key) && isAddress(value)) {
        result.addresses.push(value);
      }
      if (key && CALLDATA_KEYS.has(key) && looksLikeCalldata(value)) {
        result.calldata.push(value);
      }
      if (key === "value" && isHexData(value)) {
        result.value = value;
      }
      if (key && GAS_KEYS.has(key)) {
        result.gasFields[key] = String(value);
      }
      // key-less fallback for personal_sign / eth_sign style params
      if (typeof value === "string" && isAddress(value)) {
        result.addresses.push(value);
      }
      return;
    }

    if (Array.isArray(value)) {
      for (const item of value) visit(item);
      return;
    }
    if (typeof value === "object") {
      for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
        visit(v, k);
      }
    }
  };

  visit(args.params);

  result.chainIds = [...new Set(result.chainIds)];
  result.addresses = [...new Set(result.addresses)];
  result.calldata = [...new Set(result.calldata)];
  result.primaryChainId = result.chainIds[0] ?? result.currentChainId;
  return result;
}
