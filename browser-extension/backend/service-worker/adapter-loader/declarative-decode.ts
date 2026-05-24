/**
 * Declarative pipeline utilities — selector extraction and route-input assembly.
 *
 * Calldata decoding has moved into WASM (`declarative_route_request_json`
 * decodes internally using the bridge-resolved bundle's `abi_fragment.abi`).
 * This module now only provides:
 *   - `extractSelector` — pull the 4-byte selector from raw calldata.
 *   - `buildRouteInput` — assemble the `DeclarativeRouteRequestInput` envelope
 *     from tx-level fields and raw calldata (no prior decode step needed).
 */

import type { DeclarativeRouteRequestInput } from "../wasm-bridge";

/**
 * Extract the 4-byte selector from raw calldata as `"0x" + 8 hex`.
 * Returns `null` for empty / short calldata.
 */
export function extractSelector(calldataHex: string | undefined): string | null {
  if (!calldataHex || !calldataHex.startsWith("0x")) return null;
  // 2 ("0x") + 8 = 10 chars minimum
  if (calldataHex.length < 10) return null;
  return ("0x" + calldataHex.slice(2, 10)).toLowerCase();
}

/**
 * Build the `DeclarativeRouteRequestInput` envelope the WASM route entry
 * consumes. Caller supplies the tx tuple (`from`, `to`, `value_wei`,
 * `block_timestamp`) and the raw calldata; WASM decodes internally.
 */
export function buildRouteInput(args: {
  chainId: number;
  to: string;
  selector: string;
  from: string;
  valueWei?: string;
  blockTimestamp?: number;
  calldata: string;
}): DeclarativeRouteRequestInput {
  return {
    chain_id: args.chainId,
    to: args.to,
    selector: args.selector,
    ctx: {
      chain_id: args.chainId,
      from: args.from,
      to: args.to,
      value_wei: args.valueWei ?? "0",
      ...(args.blockTimestamp !== undefined
        ? { block_timestamp: args.blockTimestamp }
        : {}),
    },
    calldata: args.calldata,
  };
}
