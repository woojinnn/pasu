/**
 * Declarative pipeline utilities — selector extraction.
 *
 * Calldata decoding has moved into WASM (`declarative_route_request_v3_json`
 * decodes internally using the bridge-resolved bundle's `abi_fragment.abi`).
 * Post-B4 cleanup (commits 6aa3cc0 / b6f3ac9): v1 `buildRouteInput` +
 * `DeclarativeRouteRequestInput` removed — v3 route entry consumes raw tx
 * fields directly, no separate input envelope needed.
 */

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
