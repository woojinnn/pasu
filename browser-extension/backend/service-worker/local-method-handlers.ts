/**
 * In-process handlers for policy-rpc methods that are pure functions of the
 * ActionBody itself — no oracles, no chain RPC, no portfolio lookup.
 *
 * The remote policy-rpc server (localhost:8787) exists for enrichment that
 * needs external data (`oracle.usd_value`, `chain.is_contract`, etc.). For
 * methods whose inputs are already present in calldata, routing through
 * HTTP is pure overhead and an unnecessary runtime dependency: a user's
 * policy stops working if the policy-rpc daemon happens to be down, even
 * though no external information was actually needed.
 *
 * This module shortcuts that path. `tryHandleLocally` is consulted before
 * `postPolicyRpc`; any method it recognises is computed here and the
 * caller never forwards that call across the network. The result shape
 * mirrors what the remote server would have returned so the WASM
 * materializer is oblivious to the split.
 */
import type { PolicyRpcCallDto } from "./wasm-bridge.types";

/**
 * Shape of a single entry in `PolicyRpcResponseDto.results`, matching the
 * `/v1/rpc` batch response shape so the WASM bridge can consume locally
 * produced results interchangeably with remote ones.
 */
export type LocalRpcResult =
  | { readonly id: string; readonly ok: true; readonly result: Record<string, unknown> }
  | {
      readonly id: string;
      readonly ok: false;
      readonly error: { readonly code: string; readonly message: string };
    };

type LocalHandler = (params: unknown) => Record<string, unknown>;

/**
 * The implicit Long-side exponent every token-native amount field shares.
 * The manifest rescales raw on-chain `amount.value` by `10^(9 - decimals)`
 * so the resulting Long sits at the same Gwei-style unit regardless of the
 * token's own decimals. Policy literals on the builder side use the same
 * shift so `> 0.00003` lines up with `> 30000` here.
 */
const NANO_SCALE = 9n;

/**
 * Largest BigInt JS can convert to a Number without precision loss.
 * `Number.MAX_SAFE_INTEGER` is 2^53 − 1 ≈ 9.007 × 10¹⁵. The WASM side
 * deserializes the value as i64, which has wider range, but the JSON
 * boundary forces us through `number` — so we clamp to MAX_SAFE_INTEGER
 * and report overflow explicitly rather than silently losing low-order
 * digits during the conversion.
 */
const MAX_SAFE = BigInt(Number.MAX_SAFE_INTEGER);

const LOCAL_HANDLERS: Record<string, LocalHandler> = {
  "token.normalize_to_nano": (rawParams: unknown) => {
    if (typeof rawParams !== "object" || rawParams === null) {
      throw new LocalMethodError(
        "invalid_params",
        "params must be an object { amount, decimals }",
      );
    }
    const { amount, decimals } = rawParams as {
      amount?: unknown;
      decimals?: unknown;
    };
    if (typeof amount !== "string") {
      throw new LocalMethodError(
        "invalid_params",
        "amount must be a string (uint256 in token-native smallest unit)",
      );
    }
    if (typeof decimals !== "number" || !Number.isInteger(decimals)) {
      throw new LocalMethodError("invalid_params", "decimals must be an integer");
    }
    if (decimals < 0 || decimals > 30) {
      throw new LocalMethodError("invalid_params", "decimals out of range (0–30)");
    }

    let wei: bigint;
    try {
      wei = BigInt(amount);
    } catch {
      throw new LocalMethodError("invalid_params", `amount is not a valid integer string: ${amount}`);
    }

    // Negative amounts aren't meaningful for token quantities; reject so a
    // typo doesn't compare against a deceptively negative Long literal.
    if (wei < 0n) {
      throw new LocalMethodError("invalid_params", "amount must be non-negative");
    }

    const decimalsBn = BigInt(decimals);
    const nano =
      decimalsBn >= NANO_SCALE
        ? wei / 10n ** (decimalsBn - NANO_SCALE)
        : wei * 10n ** (NANO_SCALE - decimalsBn);

    if (nano > MAX_SAFE) {
      throw new LocalMethodError(
        "overflow",
        `rescaled value ${nano} exceeds JS Number safe range — refine the threshold`,
      );
    }

    return { nano: Number(nano) };
  },
};

class LocalMethodError extends Error {
  readonly code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
    this.name = "LocalMethodError";
  }
}

/**
 * Attempt to execute `call` in-process. Returns `null` when no local
 * handler is registered for the method — the caller is expected to fall
 * back to the remote policy-rpc server in that case.
 *
 * On success, returns a `LocalRpcResult` whose shape matches what the
 * remote server would have emitted, so `materializePolicyRpc` consumes
 * local and remote results from the same array without branching.
 *
 * On handler failure (validation error, overflow, …), returns a failure
 * variant rather than throwing — keeping the call-batch resilient the way
 * the remote path is.
 */
export function tryHandleLocally(call: PolicyRpcCallDto): LocalRpcResult | null {
  const handler = LOCAL_HANDLERS[call.method];
  if (!handler) return null;
  try {
    const result = handler(call.params);
    return { id: call.id, ok: true, result };
  } catch (err) {
    if (err instanceof LocalMethodError) {
      return {
        id: call.id,
        ok: false,
        error: { code: err.code, message: err.message },
      };
    }
    return {
      id: call.id,
      ok: false,
      error: {
        code: "local_error",
        message: err instanceof Error ? err.message : String(err),
      },
    };
  }
}

/**
 * List of method names this module handles, for diagnostic logging and
 * tests asserting which calls bypass the remote server.
 */
export function locallyHandledMethods(): string[] {
  return Object.keys(LOCAL_HANDLERS);
}
