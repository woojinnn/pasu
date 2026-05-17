import type { Hex } from "./types";

export interface RuntimeContext {
  stack: Array<`${number}:${string}`>;
  depthLimit: number;
}

/**
 * Reverse-lookup interface — used to break the host-imports → loader cycle
 * without creating a hard import dependency.
 */
export interface LookupTarget {
  decodeViaLookup(
    chain: number,
    address: Hex,
    calldata: Uint8Array,
    context: RuntimeContext
  ): Promise<unknown>;
}

/**
 * Build the `env` import object for instantiating an adapter.
 *
 * Each adapter instance gets its own import object closing over its OWN
 * `alloc` function — `lookup_adapter` must materialise the result JSON into
 * the *caller's* memory, not the loader's.
 *
 * The host imports do not yet have a memory reference at construction time;
 * we patch in `alloc`/`memory` once the instance is built.
 *
 * KNOWN LIMITATION — synchronous `lookup_adapter`:
 *   WebAssembly host imports are synchronous, but the loader is async. The
 *   v0 strategy is to PRE-WARM the loader cache by walking the call tree
 *   top-down before invoking `decode_call`, so that `lookup_adapter` only
 *   needs a synchronous cache hit + synchronous adapter invocation. When a
 *   sub-adapter is not pre-warmed we return a packed `Err{kind:"host"}` so
 *   the caller can surface a meaningful error.
 *   TODO(plan-2-followup): integrate WebAssembly.Suspending/Promising or a
 *   JSPI shim once it ships across both Chrome/Firefox.
 */
export interface HostBinding {
  imports: WebAssembly.Imports;
  attach(memory: WebAssembly.Memory, alloc: (n: number) => number): void;
}

export function buildHostImports(
  loader: LookupTarget,
  context: RuntimeContext
): HostBinding {
  // Reference loader so the host import boundary can be wired up to the
  // reverse-lookup target later (JSPI follow-up). Today the synchronous
  // boundary returns an error immediately, so we never call into it.
  void loader;

  let mem: WebAssembly.Memory | null = null;
  let alloc: ((n: number) => number) | null = null;

  const log = (level: number, ptr: number, len: number) => {
    if (!mem) return;
    const buf = new Uint8Array(mem.buffer, ptr, len);
    const msg = new TextDecoder().decode(buf);
    console.debug(`[adapter:${level}] ${msg}`);
  };

  const lookup_adapter = (
    chain: bigint,
    addrPtr: number,
    dataPtr: number,
    dataLen: number
  ): bigint => {
    if (!mem || !alloc) return 0n;
    const chainNum = Number(chain);
    const addrBytes = new Uint8Array(mem.buffer, addrPtr, 20);
    const addrHex = ("0x" +
      Array.from(addrBytes)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("")) as Hex;
    // Defensive copy: subsequent alloc() calls may grow memory and detach
    // the original view.
    const _calldata = new Uint8Array(mem.buffer, dataPtr, dataLen).slice();
    void _calldata;

    const key: `${number}:${string}` = `${chainNum}:${addrHex.toLowerCase()}`;
    if (context.stack.length >= context.depthLimit) {
      return packIntoCaller(
        JSON.stringify({ Err: { kind: "depth_exceeded" } }),
        mem,
        alloc
      );
    }
    if (context.stack.includes(key)) {
      return packIntoCaller(
        JSON.stringify({
          Err: { kind: "cycle", chain: chainNum, address: addrHex },
        }),
        mem,
        alloc
      );
    }

    // The host import boundary is synchronous — we can't await
    // loader.decodeViaLookup here. v0 strategy: clients PRE-WARM the loader
    // cache for sub-adapters before invoking the parent decode_call. For
    // the simplest path (leaf adapter, no recursion), this code path never
    // fires. See file header KNOWN LIMITATION block for details.
    return packIntoCaller(
      JSON.stringify({
        Err: {
          kind: "host",
          message:
            "lookup_adapter synchronous fast-path not implemented; pre-warm cache or use top-level orchestration",
        },
      }),
      mem,
      alloc
    );
  };

  return {
    imports: {
      env: {
        log,
        lookup_adapter,
      },
    },
    attach(memory, allocFn) {
      mem = memory;
      alloc = allocFn;
    },
  };
}

function packIntoCaller(
  json: string,
  mem: WebAssembly.Memory,
  alloc: (n: number) => number
): bigint {
  const bytes = new TextEncoder().encode(json);
  const ptr = alloc(bytes.length);
  new Uint8Array(mem.buffer, ptr, bytes.length).set(bytes);
  return (BigInt(ptr) << 32n) | BigInt(bytes.length);
}
