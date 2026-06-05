/**
 * v3 declarative routing orchestrator entry.
 *
 * Pipeline (one tx in → Action[] out, or a miss for no-match):
 *
 *   1. Extract the 4-byte selector from `(chain_id, to, calldata)`.
 *   2. JIT install via `installDeclarativeBundleV3` — registry-api-v3 fetch +
 *      WASM `declarative_install_v3_json`. A registry miss surfaces as a clean
 *      `miss`; the orchestrator treats misses/faults as fail-closed warnings.
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
import { type Abi, type Hex, decodeFunctionData } from "viem";
import {
  installDeclarativeBundleV3,
  InstallDeclarativeV3Error,
  type InstallDeclarativeV3Result,
} from "./declarative-adapter-loader";

/**
 * v3 outcome shape. The hit payload carries decoded `Action[]` values
 * (PDF FSM `Vec<Action>`) and the registry-v2 manifest decoder id.
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

/** Index of the single `tuple[]` argument — the `Call[]` bundle of a
 * `multicall_call_array` (Bundler3) manifest. */
function callArrayArgIndex(bundle: V3Bundle): number | null {
  const abi = bundle.abi_fragment.abi as
    | { inputs?: Array<{ type?: unknown }> }
    | null
    | undefined;
  const inputs = abi?.inputs;
  if (!Array.isArray(inputs)) return null;
  const indices = inputs.flatMap((input, index) =>
    input.type === "tuple[]" ? [index] : [],
  );
  return indices.length === 1 ? indices[0] : null;
}

/** One decoded `Call` tuple leg: its target, 4-byte selector, and full data
 * (`0x`-hex) — the data is kept so a Morpho `reenter(Call[])` callback nested in
 * it can be recursed (D-C). */
type DecodedCallLeg = { to: string; selector: string; dataHex: string };

/** Decode the `Call[]` tuples beginning at byte `arrayStart` (the `count` word)
 * of `hex`. `Call = (address to, bytes data, uint256 value, bool skipRevert,
 * bytes32 callbackHash)`; each element is dynamic (carries `bytes data`). Shared
 * by the top-level `multicall(Call[])` and the nested `reenter(Call[])` decode. */
function decodeCallTuplesAt(hex: string, arrayStart: number): DecodedCallLeg[] {
  const count = readAbiWordNumber(hex, arrayStart);
  if (count === null || count > 64) return [];

  const offsetsStart = arrayStart + 32;
  const legs: DecodedCallLeg[] = [];
  for (let i = 0; i < count; i += 1) {
    const elemRelativeOffset = readAbiWordNumber(hex, offsetsStart + i * 32);
    if (elemRelativeOffset === null) return [];
    const elemStart = offsetsStart + elemRelativeOffset;

    // tuple words: 0=to, 1=data offset (rel to elemStart), 2=value, 3=skipRevert, 4=callbackHash
    const toWord = readAbiWord(hex, elemStart);
    if (!toWord) return [];
    const to = `0x${toWord.slice(24)}`;

    const dataRelativeOffset = readAbiWordNumber(hex, elemStart + 32);
    if (dataRelativeOffset === null) return [];
    const dataStart = elemStart + dataRelativeOffset;
    const dataLength = readAbiWordNumber(hex, dataStart);
    if (dataLength === null) return [];
    if (dataLength < 4) continue; // bare value-transfer leg — no selector to route

    const contentStart = 2 + (dataStart + 32) * 2;
    const dataBody = hex.slice(contentStart, contentStart + dataLength * 2);
    if (dataBody.length !== dataLength * 2) return [];
    legs.push({
      to,
      selector: `0x${dataBody.slice(0, 8)}`,
      dataHex: `0x${dataBody}`,
    });
  }
  return legs;
}

/** Decode the top-level `Call[]` arg (`argIndex`) of a `multicall(Call[])`. */
function decodeCallArrayArg(
  calldataHex: string,
  argIndex: number,
): DecodedCallLeg[] {
  if (!isHexCalldata(calldataHex)) return [];
  const argsStart = 4;
  const arrayRelativeOffset = readAbiWordNumber(
    calldataHex,
    argsStart + argIndex * 32,
  );
  if (arrayRelativeOffset === null) return [];
  return decodeCallTuplesAt(calldataHex, argsStart + arrayRelativeOffset);
}

/** When `leg`'s installed `bundle` declares `emit.reenter_callback_arg`, extract
 * the `reenter(Call[])` callback legs nested in that `bytes` arg. Manifest-driven
 * + ABI-driven — NO per-protocol selector or arg-position list: the bundle names
 * the arg, and its `abi_fragment.abi` locates it (viem decode handles the static
 * `marketParams` tuple etc.). The arg value is the raw `abi.encode(Call[])`. */
function reenterCallbackLegs(
  bundle: V3Bundle,
  legDataHex: string,
): DecodedCallLeg[] {
  const emit = bundle.emit as { reenter_callback_arg?: unknown } | null | undefined;
  const arg = emit?.reenter_callback_arg;
  if (typeof arg !== "string") return [];
  const abi = bundle.abi_fragment?.abi as
    | { inputs?: Array<{ name?: unknown }> }
    | undefined;
  const inputs = abi?.inputs;
  if (!Array.isArray(inputs)) return [];
  const argIndex = inputs.findIndex((input) => input?.name === arg);
  if (argIndex < 0) return [];

  let args: readonly unknown[];
  try {
    ({ args = [] } = decodeFunctionData({
      abi: [abi] as unknown as Abi,
      data: legDataHex as Hex,
    }));
  } catch {
    return [];
  }
  const callbackHex = args[argIndex];
  if (typeof callbackHex !== "string" || !isHexCalldata(callbackHex)) return [];
  // callbackHex == abi.encode(Call[]): the array `count` sits at the offset in
  // word 0 (a single dynamic param is [offset][array data]).
  const arrayStart = readAbiWordNumber(callbackHex, 0);
  if (arrayStart === null) return [];
  return decodeCallTuplesAt(callbackHex, arrayStart);
}

/** Install the WHOLE `multicall(Call[])` call tree at each leg's OWN `to`: the
 * top-level legs PLUS the legs nested in any `reenter(Call[])` callback (D-C —
 * leverage/flash-loan callbacks target the adapter too and must be pre-installed
 * or the engine's recursive re-route would drop them). INSTALL-DRIVEN: each leg's
 * installed manifest tells us (via `reenter_callback_arg`) whether it carries a
 * callback, so there is NO per-protocol selector list. Bundles are cached per
 * callkey; recursion bounded by `MAX_REENTER_DEPTH`. */
async function installCallTree(
  chainId: number,
  legs: DecodedCallLeg[],
  depth: number,
  cache: Map<string, InstallDeclarativeV3Result | null>,
): Promise<void> {
  const MAX_REENTER_DEPTH = 4;
  await Promise.all(
    legs.map(async (leg) => {
      const key = `${leg.to}:${leg.selector}`.toLowerCase();
      let installed = cache.get(key);
      if (installed === undefined) {
        installed = await installDeclarativeBundleV3({
          chainId,
          to: leg.to,
          selector: leg.selector,
        });
        cache.set(key, installed);
      }
      if (depth >= MAX_REENTER_DEPTH || !installed) return;
      const callbackLegs = reenterCallbackLegs(installed.bundle, leg.dataHex);
      if (callbackLegs.length > 0) {
        await installCallTree(chainId, callbackLegs, depth + 1, cache);
      }
    }),
  );
}

async function preinstallMulticallChildren(args: {
  chainId: number;
  to: string;
  calldataHex: string | undefined;
  installedBundle: V3Bundle;
}): Promise<void> {
  const emit = args.installedBundle.emit as
    | { strategy?: unknown }
    | null
    | undefined;

  // PER-LEG-TO (Bundler3 `multicall(Call[])`): each leg targets its OWN `to`
  // (e.g. GeneralAdapter1, Permit2). Pre-install at each leg's address so the
  // engine's `multicall_call_array` re-route finds the child mapper instead of
  // skipping it (no_declarative_v3_mapper) and dropping the leg.
  if (emit?.strategy === "multicall_call_array") {
    const argIndex = callArrayArgIndex(args.installedBundle);
    if (argIndex === null || !args.calldataHex) return;
    // Install the WHOLE call tree — top-level legs PLUS the legs nested in any
    // `reenter(Call[])` callback (D-C) — so the engine's recursive re-route finds
    // every child mapper instead of dropping callback legs. Manifest-driven.
    await installCallTree(
      args.chainId,
      decodeCallArrayArg(args.calldataHex, argIndex),
      0,
      new Map(),
    );
    return;
  }

  // SAME-TO (`multicall_recurse`, `multicall(bytes[])`): children share the outer `to`.
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
 * Reserved sentinel for selector-less BARE native-ETH transfers (B.3). A tx with
 * EMPTY calldata has no 4-byte selector, so it cannot be keyed by a real function.
 */
const NATIVE_TRANSFER_SELECTOR = "0x00000000";

/**
 * Route a value-bearing EMPTY-calldata tx under the native-transfer sentinel so a
 * `match.selector="0x00000000"` manifest (Lido bare-ETH stake into stETH's
 * fallback / wstETH's receive; HyperLiquid HYPE deposit) can decode it. The WASM
 * route substitutes the same sentinel ONLY for empty calldata, so it never
 * collides with a real dispatch. An address with no sentinel manifest simply
 * misses on install → the same fail-closed warn as before (one extra registry
 * lookup). Gated on value > 0: an empty-calldata 0-value call stakes nothing, so
 * it stays a plain `no_selector` miss (no pointless lookup per zero-value poke).
 */
function nativeTransferSelector(
  calldataHex: string | undefined,
  valueWei: string | undefined,
): string | null {
  const isEmptyCalldata =
    calldataHex === undefined || calldataHex === "" || calldataHex === "0x";
  const isValueBearing =
    valueWei !== undefined && valueWei !== "" && valueWei !== "0";
  return isEmptyCalldata && isValueBearing ? NATIVE_TRANSFER_SELECTOR : null;
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
 * Bundle hydration lives in `declarative-adapter-loader.ts`; the active
 * verdict path no longer has a legacy v1/static fallback.
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
  const selector =
    extractSelector(args.calldataHex) ??
    nativeTransferSelector(args.calldataHex, args.valueWei);
  if (!selector) {
    return { kind: "miss", reason: "no_selector" };
  }
  const submittedAt = args.submittedAt ?? Math.floor(Date.now() / 1000);

  // JIT install via registry-api-v3. If the callkey has no
  // matching v3 manifest (404 / `matched: false`), `installDeclarativeBundleV3`
  // returns `null`; we surface that as a clean miss so the orchestrator can
  // produce the fail-closed verdict/audit row.
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
      calldata: args.calldataHex ?? "0x",
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
    // Promote WASM EngineError to `engine_error` so the caller can audit it.
    // Other throws bucket into `unexpected`.
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
