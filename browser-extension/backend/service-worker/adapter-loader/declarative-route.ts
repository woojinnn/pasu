/**
 * Phase 4B — v3 declarative routing orchestrator entry.
 *
 * Pipeline (one tx in → Action[] out, or a miss for no-match):
 *
 *   1. Extract the 4-byte selector from `(chain_id, to, calldata)`.
 *   2. JIT install via `installDeclarativeBundleV3` — registry-api-v3 fetch +
 *      WASM `declarative_install_v3_json`. A registry miss surfaces as a clean
 *      `miss` so the caller falls through to the static Tier B pipeline.
 *   3. Hand `(chain_id, to, selector, calldata, meta)` to the WASM route entry
 *      `declarative_route_request_v3_json`. The engine decodes the raw
 *      calldata using the bridge-resolved bundle's `abi_fragment.abi` and
 *      emits the PDF FSM `policy_transition::action::Action` tree.
 *   4. Return `{ actions, decoderId }`. The caller (orchestrator in
 *      `service-worker/orchestrator.ts`) plugs these into the audit trail.
 */

import {
  EngineError,
  declarativeRouteRequestV3,
  type DeclarativeRouteRequestV3Result,
} from "../wasm-bridge";
import type { V3Bundle } from "./bundle-schema";
import { extractSelector } from "./declarative-decode";
import {
  installDeclarativeBundleV3,
  InstallDeclarativeV3Error,
} from "./declarative-adapter-loader";

/**
 * Phase 4B — v3 outcome shape. The hit payload carries the new `Action[]`
 * (PDF FSM `Vec<Action>`). `decoder_id` is empty under the Phase 4B stub —
 * Phase 4D populates it from the registry-v2 manifest match.
 */
export interface DeclarativeRouteV3Hit {
  actions: Record<string, unknown>[];
  decoderId: string;
}

export type DeclarativeRouteV3Outcome =
  | { kind: "hit"; value: DeclarativeRouteV3Hit }
  | {
      kind: "miss";
      reason: "no_selector" | "bundle_not_installed";
    }
  | {
      kind: "fault";
      reason: "engine_error" | "install_failed" | "unexpected";
      cause: unknown;
    };

function isHexCalldata(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.startsWith("0x") &&
    value.length >= 10 &&
    value.length % 2 === 0
  );
}

function readAbiWord(calldataHex: string, byteOffset: number): string | null {
  const start = 2 + byteOffset * 2;
  const end = start + 64;
  if (start < 2 || end > calldataHex.length) return null;
  return calldataHex.slice(start, end);
}

function readAbiWordNumber(
  calldataHex: string,
  byteOffset: number,
): number | null {
  const word = readAbiWord(calldataHex, byteOffset);
  if (!word) return null;
  const value = BigInt(`0x${word}`);
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) return null;
  return Number(value);
}

function bytesArrayArgIndex(bundle: V3Bundle): number | null {
  const abi = bundle.abi_fragment.abi as
    | { inputs?: Array<{ type?: unknown }> }
    | null
    | undefined;
  const inputs = abi?.inputs;
  if (!Array.isArray(inputs)) return null;

  const indices = inputs.flatMap((input, index) =>
    input.type === "bytes[]" ? [index] : [],
  );
  return indices.length === 1 ? indices[0] : null;
}

function decodeBytesArrayArg(calldataHex: string, argIndex: number): string[] {
  if (!isHexCalldata(calldataHex)) return [];

  const argsStart = 4;
  const arrayRelativeOffset = readAbiWordNumber(
    calldataHex,
    argsStart + argIndex * 32,
  );
  if (arrayRelativeOffset === null) return [];

  const arrayStart = argsStart + arrayRelativeOffset;
  const childCount = readAbiWordNumber(calldataHex, arrayStart);
  if (childCount === null || childCount > 64) return [];

  const offsetsStart = arrayStart + 32;
  const children: string[] = [];
  for (let i = 0; i < childCount; i += 1) {
    const childRelativeOffset = readAbiWordNumber(
      calldataHex,
      offsetsStart + i * 32,
    );
    if (childRelativeOffset === null) return [];

    const childStart = offsetsStart + childRelativeOffset;
    const childLength = readAbiWordNumber(calldataHex, childStart);
    if (childLength === null) return [];

    const childDataStart = childStart + 32;
    const childDataEnd = childDataStart + childLength;
    if (childDataEnd * 2 + 2 > calldataHex.length) return [];
    children.push(
      `0x${calldataHex.slice(2 + childDataStart * 2, 2 + childDataEnd * 2)}`,
    );
  }

  return children;
}

function multicallChildSelectors(
  bundle: V3Bundle,
  calldataHex: string | undefined,
): string[] {
  const emit = bundle.emit as
    | { strategy?: unknown; recurse_rule_id?: unknown }
    | null
    | undefined;
  if (
    emit?.strategy !== "multicall_recurse" ||
    emit.recurse_rule_id !== "self_array_bytes_last_arg" ||
    !calldataHex?.startsWith("0x")
  ) {
    return [];
  }

  const argIndex = bytesArrayArgIndex(bundle);
  if (argIndex === null) {
    return [];
  }

  const childCalls = decodeBytesArrayArg(calldataHex, argIndex);

  const selectors = new Set<string>();
  for (const child of childCalls) {
    const selector = extractSelector(child);
    if (selector) selectors.add(selector);
  }
  return [...selectors];
}

async function preinstallMulticallChildren(args: {
  chainId: number;
  to: string;
  calldataHex: string | undefined;
  installedBundle: V3Bundle;
}): Promise<void> {
  const selectors = multicallChildSelectors(
    args.installedBundle,
    args.calldataHex,
  );
  await Promise.all(
    selectors.map((selector) =>
      installDeclarativeBundleV3({
        chainId: args.chainId,
        to: args.to,
        selector,
      }),
    ),
  );
}

/**
 * Phase M4 — v3 route entry. Pipeline:
 *   1. extract 4-byte selector,
 *   2. JIT install (registry-api-v3 fetch + WASM `declarative_install_v3_json`)
 *      via `installDeclarativeBundleV3` — same callkey ((chainId, to, selector))
 *      gets `null` on registry miss (returns `miss/bundle_not_installed`),
 *   3. forward meta fields (value / gas_limit / gas_price / submitter /
 *      submitted_at / nonce) to WASM `declarative_route_request_v3_json`,
 *   4. unwrap the WASM result into a TS-friendly outcome.
 *
 * registry-v3 anonymous fetch is enabled via Cloud Run `allUsers/run.invoker`
 * grant (Plan §M0). Bundle hydration lives in `declarative-adapter-loader.ts`
 * (Plan §M3 — JIT + 2-layer cache; v1 path is untouched).
 *
 * On `fault` the caller falls back to the legacy v1 path.
 */
export async function tryDeclarativeRouteV3(args: {
  chainId: number;
  from: string;
  to: string;
  valueWei?: string;
  gasLimit?: string;
  gasPrice?: string;
  nonce?: number;
  submittedAt?: number;
  blockTimestamp?: number;
  calldataHex: string | undefined;
}): Promise<DeclarativeRouteV3Outcome> {
  const selector = extractSelector(args.calldataHex);
  if (!selector) {
    return { kind: "miss", reason: "no_selector" };
  }
  const submittedAt = args.submittedAt ?? Math.floor(Date.now() / 1000);

  // Plan §M4 — JIT install via registry-api-v3. If the callkey has no
  // matching v3 manifest (404 / `matched: false`), `installDeclarativeBundleV3`
  // returns `null`; we surface that as a clean miss so the caller falls
  // through to v1 without surfacing it as a fault.
  try {
    const installed = await installDeclarativeBundleV3({
      chainId: args.chainId,
      to: args.to,
      selector,
    });
    if (installed === null) {
      return { kind: "miss", reason: "bundle_not_installed" };
    }
    await preinstallMulticallChildren({
      chainId: args.chainId,
      to: args.to,
      calldataHex: args.calldataHex,
      installedBundle: installed.bundle,
    });
  } catch (err) {
    if (err instanceof InstallDeclarativeV3Error) {
      return { kind: "fault", reason: "install_failed", cause: err };
    }
    return { kind: "fault", reason: "unexpected", cause: err };
  }

  let result: DeclarativeRouteRequestV3Result;
  try {
    result = await declarativeRouteRequestV3({
      chain_id: args.chainId,
      to: args.to,
      selector,
      calldata: args.calldataHex!,
      ...(args.valueWei !== undefined ? { value: args.valueWei } : {}),
      ...(args.gasLimit !== undefined ? { gas_limit: args.gasLimit } : {}),
      ...(args.gasPrice !== undefined ? { gas_price: args.gasPrice } : {}),
      submitter: args.from,
      submitted_at: submittedAt,
      ...(args.nonce !== undefined ? { nonce: args.nonce } : {}),
      ...(args.blockTimestamp !== undefined
        ? { block_timestamp: args.blockTimestamp }
        : {}),
    });
  } catch (err) {
    // The Phase 4B WASM stub only throws on malformed input — promote any
    // EngineError to `engine_error` so the caller can audit it. Other
    // throws (network glitch, etc.) bucket into `unexpected`.
    if (err instanceof EngineError) {
      return { kind: "fault", reason: "engine_error", cause: err };
    }
    return { kind: "fault", reason: "unexpected", cause: err };
  }

  return {
    kind: "hit",
    value: {
      actions: result.actions,
      decoderId: result.decoder_id,
    },
  };
}
