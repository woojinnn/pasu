/**
 * Protocol-aware source resolver framework.
 *
 * Each protocol that exposes receipt tokens / pool tokens via deterministic
 * on-chain enumeration (Aave V3 reserve list, Compound V3 markets, Pendle
 * active PT, etc.) ships one file under `scripts/resolvers/<protocol>.ts`
 * that registers one or more `ProtocolResolver`s under unique source kinds
 * matching `<protocol>:<scope>`.
 *
 * Resolution is delegated from `build-index.ts::resolveBundle` at build
 * time. Results are disk-cached under `registryV2/cache/protocol-sources/`
 * with a 30-day default TTL; `--force-refresh` invalidates and re-fetches.
 *
 * Wire convention: all returned addresses are lowercased 0x-prefixed Hex.
 */

import type { Address } from "viem";

/** ERC contract kind (re-exported here to avoid an import cycle with build-index). */
export type Hex = string;

/**
 * Protocol source kind. Concrete support is the resolver registry in
 * `index.ts`, not a duplicated hardcoded union here and in build-index.
 */
export type ProtocolSourceKind = `${string}:${string}`;

/** A single resolver entry — one source kind → one async address fetcher. */
export interface ProtocolResolver {
  /** Source kind identifier, e.g. "aave_v3:atokens". */
  readonly source: ProtocolSourceKind;

  /**
   * Enumerate addresses for `chainId`. Implementations:
   *  - Read from cache when present and fresh (TTL not exceeded) and
   *    `opts.forceRefresh` is false.
   *  - Otherwise fetch on-chain (single Multicall3 batch where possible),
   *    write back to cache, and return the sorted lowercase address list.
   *
   * Returning an empty array on a chain where the protocol is not deployed
   * is acceptable — `resolveBundle` will skip that chain. Throwing is
   * reserved for unrecoverable RPC errors with no fallback cache.
   */
  resolve(chainId: number, opts: ResolverOpts): Promise<Hex[]>;

  /**
   * Optional richer resolver for protocols whose manifests need per-address
   * metadata baked into the emitted bundle at build time. Plain address
   * expansion is not enough for pool-heavy protocols like Curve where coin
   * maps differ by pool.
   */
  resolveWithContext?(chainId: number, opts: ResolverOpts): Promise<ProtocolResolvedAddress[]>;
}

export interface ProtocolResolvedAddress {
  /** Lowercased 0x-prefixed address to write into `match.chain_to_addresses`. */
  address: Hex;
  /** Unique, filesystem/id-safe suffix appended to the manifest id before @version. */
  id_suffix?: string;
  /** Build-time substitution context consumed by `$source.*` placeholders. */
  context?: Record<string, unknown>;
}

export interface ResolverOpts {
  /** Bypass cache lookup and force a live RPC fetch. CLI flag `--force-refresh`. */
  forceRefresh: boolean;

  /** Shared RPC client (multicall3-batched, per-chain). */
  rpc: RpcClient;
}

// ---------------------------------------------------------------------------
// RPC client surface (concrete impl in `rpc.ts`)
// ---------------------------------------------------------------------------

/**
 * Per-chain RPC client. Implementations wrap viem's `publicClient` with
 * Multicall3 batching and a typed `batchCall` interface for the resolver
 * use case (uniform ABI struct → uniform field extract).
 */
export interface RpcClient {
  /** Single eth_call. ABI-encoded result is decoded by the caller. */
  call(chainId: number, params: {
    to: Address;
    data: Hex;
  }): Promise<Hex>;

  /** Current block number for the chain (used for cache provenance). */
  blockNumber(chainId: number): Promise<bigint>;
}

// ---------------------------------------------------------------------------
// Cache schema (disk-persisted under `registryV2/cache/protocol-sources/`)
// ---------------------------------------------------------------------------

export interface CacheEntry {
  /** Source kind that produced this snapshot. */
  scope: ProtocolSourceKind;
  /** Chain id. */
  chainId: number;
  /** Lowercased 0x-prefixed addresses, sorted. */
  addresses: Hex[];
  /** Unix epoch seconds when the snapshot was last fetched. */
  synced_at: number;
  /** Block height at fetch time — for audit / reproducibility. */
  source_block: number;
  /** Pool / registry address consulted at fetch time (resolver-specific). */
  pool_address?: Hex;
  /** Reserve / market / pool count (resolver-specific). */
  entry_count?: number;
}
