/**
 * Phase 2B — Just-In-Time adapter resolver.
 *
 * Spec: `ADAPTER_LOADER_ARCHITECTURE.md` §7 (3-layer loading) and
 * §7.3 (`resolveAdapter` / `doJitFetch` pseudocode).
 *
 * Lookup order:
 *   1. Layer 1 — `lookupMountedBundle` (seed bundles + JIT-installed
 *      bundles previously mounted in the same SW lifetime).
 *   2. Negative cache — return the cached verdict without re-hitting the
 *      registry. Reasons: `no_publisher` (5 min), `integrity_failed`
 *      (5 min), `timeout` (30 s).
 *   3. Layer 3 — JIT fetch from the registry, dedup'd via an `inflight`
 *      `Map<key, Promise>` so N concurrent callers share one network
 *      round-trip + WASM mount.
 *
 * Not implemented (deferred per task spec):
 *   - Layer 2 prefetch (LRU IndexedDB) — Phase 4+
 *   - HMAC-SHA256 key obfuscation (§7.5)
 *   - cross-worker BroadcastChannel coordination (§7.4 future row)
 */

import {
  byCallKey,
  RegistryError,
  type ByCallKeyOptions,
  type CallMatchKey,
} from "../registry/client";
import { lookupMountedBundle, type MountResult } from "./declarative-adapter-loader";
import { InstallError, installBundle } from "./installBundle";
import {
  negativeCache,
  serializeKey,
  type NegativeReason,
} from "./negative-cache";
import { adapterCache } from "./adapter-cache";

/**
 * Output of `resolveAdapter`. Either a ready-to-use adapter (Layer 1 hit
 * or JIT install success) or a verdict the orchestrator can short-circuit
 * Cedar evaluation on.
 */
export type AdapterOrVerdict =
  | { kind: "adapter"; adapter: MountResult; source: "layer1" | "layer2" | "jit" }
  | { kind: "verdict"; verdict: "no_adapter"; reason: NegativeReason };

export interface ResolveAdapterOptions {
  /** Forwarded to the registry client — baseUrl, timeoutMs, fetchImpl. */
  registry?: ByCallKeyOptions;
}

/**
 * In-flight dedupe (§7.3:811-813). Same SW process — a `Map` is enough.
 * Cross-worker dedupe via BroadcastChannel is a future enhancement.
 *
 * Round 2 audit (P1) — bound the size so a hostile dapp that issues N
 * unique `(chain_id, to, selector)` triplets cannot accumulate unbounded
 * Promises (each holding a fetch / WASM call in flight). The cap is 256:
 * legitimate Uniswap traffic rarely sees more than a couple of concurrent
 * mounts, so this gives plenty of headroom while still bounding worst-case
 * memory.
 */
const MAX_INFLIGHT_ADAPTER_FETCHES = 256;
const inflight = new Map<string, Promise<AdapterOrVerdict>>();

/**
 * Resolve the adapter that should handle the given call. Walks the
 * Layer-1 → negative-cache → JIT chain.
 */
export async function resolveAdapter(
  key: CallMatchKey,
  options: ResolveAdapterOptions = {},
): Promise<AdapterOrVerdict> {
  // Layer 1 — already mounted (seed bundle or earlier JIT install).
  const mounted = lookupMountedBundle(key.chain_id, key.to, key.selector);
  if (mounted) {
    return { kind: "adapter", adapter: mounted, source: "layer1" };
  }

  // Negative cache — short-circuit known misses without going to the wire.
  const neg = negativeCache.get(key);
  if (neg) {
    return { kind: "verdict", verdict: "no_adapter", reason: neg.reason };
  }

  // Layer 2 — persistent chrome.storage cache (survives SW restart).
  const cached = await adapterCache.get(key);
  if (cached) {
    try {
      const adapter = await installBundle(cached.bundle, cached.bundle_sha256);
      return { kind: "adapter", adapter, source: "layer2" };
    } catch {
      // 손상된 캐시 entry (sha mismatch 등) — drop 하고 Layer 3 로 fall-through.
      await adapterCache.delete(key);
    }
  }

  // Layer 3 — JIT. Dedupe so concurrent callers share one fetch.
  const k = serializeKey(key);
  const existing = inflight.get(k);
  if (existing) return existing;

  // Round 2 audit (P1) — reject new fetches when the inflight Map is full.
  // The cap is well above the legitimate concurrency ceiling, so a trip
  // here means the SW is under abuse: a hostile dapp pumping unique
  // callkeys. Returning `timeout` reuses the existing negative-cache
  // verdict path and lets the caller short-circuit to the static path.
  if (inflight.size >= MAX_INFLIGHT_ADAPTER_FETCHES) {
    return { kind: "verdict", verdict: "no_adapter", reason: "timeout" };
  }

  const p = doJitFetch(key, options).finally(() => {
    // Always clean up so a settled Promise can't keep blocking the slot.
    // The cleanup runs regardless of resolve/reject.
    inflight.delete(k);
  });
  inflight.set(k, p);
  return p;
}

/**
 * Best-effort child-bundle prefetch for `multicall_recurse`.
 *
 * Resolves (fetch + install + mount) every child callkey so a subsequent
 * WASM `declarative_route_request_json` finds each child in the engine
 * bridge. Unlike `resolveAdapter`, this never throws and never surfaces a
 * verdict: a child that 404s, times out, or fails integrity is simply left
 * un-mounted — the WASM `WasmChildResolver` then produces its own
 * `map_failed` for that child, which the orchestrator already degrades to
 * the static path.
 *
 * Children resolve in parallel via `Promise.allSettled`. Identical callkeys
 * are de-duped up front, and `resolveAdapter`'s `inflight` map collapses any
 * remaining concurrent same-callkey fetches, so a multicall with duplicate
 * child selectors costs one network round-trip per distinct callkey.
 */
export async function prefetchChildAdapters(
  childKeys: readonly CallMatchKey[],
  options: ResolveAdapterOptions = {},
): Promise<void> {
  if (childKeys.length === 0) return;
  const seen = new Set<string>();
  const unique: CallMatchKey[] = [];
  for (const key of childKeys) {
    const id = serializeKey(key);
    if (seen.has(id)) continue;
    seen.add(id);
    unique.push(key);
  }
  await Promise.allSettled(unique.map((key) => resolveAdapter(key, options)));
}

/**
 * Inner JIT fetch + install + cache-on-failure pipeline. Wraps
 * `installBundle` so any failure produces a uniform negative-cache entry
 * matching the spec's TTL map.
 */
async function doJitFetch(
  key: CallMatchKey,
  options: ResolveAdapterOptions,
): Promise<AdapterOrVerdict> {
  try {
    const result = await byCallKey(key, options.registry);
    // 200 OK with matched: true — install + mount.
    const adapter = await installBundle(result.bundle, result.bundle_sha256);
    await adapterCache.put(key, result.bundle, result.bundle_sha256);
    return { kind: "adapter", adapter, source: "jit" };
  } catch (e) {
    return classifyAndCache(key, e);
  }
}

/**
 * Map a JIT error to the spec's three negative-cache reasons and TTLs:
 *
 *   - bundle_hash_mismatch → integrity_failed (300s) — suspicious tamper
 *   - registry 404         → no_publisher    (300s) — permanent absence
 *   - any other failure    → timeout         (30s)  — self-healing
 *
 * Other `InstallError` variants (`schema_invalid`, `wasm_install_failed`)
 * fall into `timeout`. Rationale: they're not a publisher absence and
 * not a hash mismatch; a 30 s cool-down lets the user retry without
 * spamming the registry, and downstream telemetry can pick them up.
 */
function classifyAndCache(
  key: CallMatchKey,
  e: unknown,
): AdapterOrVerdict {
  if (e instanceof InstallError && e.code === "bundle_hash_mismatch") {
    negativeCache.add(key, 300, "integrity_failed");
    return { kind: "verdict", verdict: "no_adapter", reason: "integrity_failed" };
  }
  if (e instanceof RegistryError && e.code === "not_found") {
    negativeCache.add(key, 300, "no_publisher");
    return { kind: "verdict", verdict: "no_adapter", reason: "no_publisher" };
  }
  // timeout, network, malformed_response, schema_invalid, wasm_install_failed
  negativeCache.add(key, 30, "timeout");
  return { kind: "verdict", verdict: "no_adapter", reason: "timeout" };
}

/**
 * Test helper — drop the inflight Map between cases so a dangling
 * Promise from a previous case can't merge into the new one.
 */
export function __resetJitFetcherForTest(): void {
  inflight.clear();
}
