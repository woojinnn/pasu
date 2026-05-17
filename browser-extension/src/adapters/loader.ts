import { AdapterBridge, type AdapterExports } from "./bridge";
import { AdapterCache, type CacheEntry } from "./cache";
import { RegistryClient } from "./registry-client";
import {
  buildHostImports,
  type LookupTarget,
  type RuntimeContext,
} from "./host-imports";
import type { AdapterResult, DecodedCall, Hex, Manifest } from "./types";

export interface LoaderConfig {
  registry: RegistryClient;
  cache: AdapterCache;
}

/**
 * Resolves (chainId, address) to a ready-to-call {@link AdapterBridge}.
 *
 * Workflow:
 *   1. cache hit → instantiate from cached bytes
 *   2. cache miss → registry resolve → fetch wasm → cache.put → instantiate
 *
 * Parallel loads for the same key are de-duplicated via an inflight map so
 * we only fetch the WASM once.
 *
 * Implements {@link LookupTarget} so it can be wired into host imports as
 * the reverse-lookup boundary for `lookup_adapter`. See `host-imports.ts`
 * for the synchronous-boundary limitation.
 */
export class Loader implements LookupTarget {
  private inflight = new Map<string, Promise<AdapterBridge | null>>();

  constructor(private cfg: LoaderConfig) {}

  async load(chainId: number, address: Hex): Promise<AdapterBridge | null> {
    const key = `${chainId}:${address.toLowerCase()}`;
    const existing = this.inflight.get(key);
    if (existing) return existing;
    const p = this.loadInner(chainId, address);
    this.inflight.set(key, p);
    try {
      return await p;
    } finally {
      this.inflight.delete(key);
    }
  }

  private async loadInner(
    chainId: number,
    address: Hex
  ): Promise<AdapterBridge | null> {
    const cached = await this.cfg.cache.get(chainId, address);
    if (cached) return this.instantiate(cached.wasm, cached.manifest);
    const res = await this.cfg.registry.resolve(chainId, address);
    if (!res) return null;
    const wasm = await this.cfg.registry.fetchWasm(res.wasm_url);
    const entry: CacheEntry = {
      version: res.version,
      manifest: res.manifest,
      wasm,
      fetchedAt: Date.now(),
    };
    await this.cfg.cache.put(chainId, address, entry);
    return this.instantiate(wasm, res.manifest);
  }

  private async instantiate(
    wasm: Uint8Array,
    manifest: Manifest
  ): Promise<AdapterBridge> {
    const context: RuntimeContext = { stack: [], depthLimit: 8 };
    const binding = buildHostImports(this, context);
    const module = await WebAssembly.compile(wasm);
    const instance = await WebAssembly.instantiate(module, binding.imports);
    const exp = instance.exports as unknown as AdapterExports;
    binding.attach(exp.memory, exp.alloc);
    return new AdapterBridge(instance, manifest);
  }

  /**
   * Reverse-lookup entrypoint, invoked from the `lookup_adapter` host import
   * (once JSPI/async-boundary support lands). Pushes onto the cycle-detection
   * stack, resolves the sub-adapter, runs decode_call, and pops on return.
   */
  async decodeViaLookup(
    chainId: number,
    address: Hex,
    calldata: Uint8Array,
    context: RuntimeContext
  ): Promise<AdapterResult<DecodedCall>> {
    const key: `${number}:${string}` = `${chainId}:${address.toLowerCase()}`;
    context.stack.push(key);
    try {
      const bridge = await this.load(chainId, address);
      if (!bridge) {
        return { Err: { kind: "decode_failed", message: "no adapter" } };
      }
      const selectorBytes = calldata.slice(0, 4);
      const selector = ("0x" +
        Array.from(selectorBytes)
          .map((b) => b.toString(16).padStart(2, "0"))
          .join("")) as Hex;
      return bridge.decodeCall(
        { chain_id: chainId, target: address, selector },
        calldata
      );
    } finally {
      context.stack.pop();
    }
  }
}
