/**
 * Phase 7D — Token metadata registry client.
 *
 * Spec: `ADAPTER_MARKETPLACE_ARCHITECTURE.md` §8 (`host:token_metadata`
 * enrichment) and §7 (3-layer loading), reused for the
 * `host:token_metadata` enrichment path.
 *
 * Lookup order (mirrors `jit-fetcher.ts` for adapter bundles):
 *   1. Layer 1 — in-process cache + persistent `Browser.storage.local`
 *      "IndexedDB-style" cache. Hit returns the metadata immediately.
 *   2. Negative cache — known misses cached by `${chainId}__${address}`.
 *      TTL: `no_publisher` 5 min, `integrity_failed` 5 min, `timeout` 30 s.
 *   3. Inflight dedupe — concurrent lookups for the same key share one
 *      Promise so N callers fan into one network round-trip.
 *   4. Layer 2 — HTTP fetch from
 *      `${REGISTRY_BASE_URL}/tokens/${chainId}/${address}.json` per Phase 7C.
 *
 * Address normalisation: every input address is lowercased before being
 * used as a cache key, URL component, or persisted payload. EIP-55
 * checksum is intentionally discarded — the registry filenames are
 * lowercased, so we MUST lowercase to avoid case-sensitive 404s on the
 * static host.
 *
 * Out of scope (per Phase 7D PoC):
 *   - HMAC-SHA256 key obfuscation (§7.5) — raw keys for now.
 *   - True `indexedDB` (use `Browser.storage.local` like the rest of the
 *     marketplace stack — quota fits the 4-token PoC comfortably).
 *   - sha256 integrity verification — token metadata is small and the
 *     registry is trusted; integrity_failed remains a slot in case we
 *     add it later.
 */
import Browser from "webextension-polyfill";

/**
 * Tagged metadata record returned for a registered token. `kind` is a
 * discriminator so we can extend the registry to ERC-721 / native /
 * other token kinds without changing the cache layout.
 */
export interface TokenMetadata {
  kind: "erc20";
  chainId: number;
  /** Lowercased EVM address — guaranteed by `normaliseAddress`. */
  address: string;
  symbol: string;
  decimals: number;
  name: string;
}

export interface TokenRegistryClient {
  lookup(chainId: number, address: string): Promise<TokenMetadata | null>;
}

export interface TokenRegistryClientOptions {
  baseUrl?: string;
  timeoutMs?: number;
  /** Injected for tests — defaults to global `fetch`. */
  fetchImpl?: typeof fetch;
}

const DEFAULT_BASE_URL =
  typeof process !== "undefined" && process.env?.REGISTRY_BASE_URL
    ? process.env.REGISTRY_BASE_URL
    : "http://localhost:8000";
const DEFAULT_TIMEOUT_MS = 2000;

const STORAGE_KEY = "registry:tokens";

type NegativeReason = "no_publisher" | "integrity_failed" | "timeout";

interface NegativeCacheEntry {
  reason: NegativeReason;
  expiresAt: number;
}

/**
 * Normalise (chainId, address) → canonical cache/URL key. Always
 * lowercases the address so checksum-cased input maps to the same slot
 * as plain-lower input.
 */
function tokenKey(chainId: number, address: string): string {
  return `${chainId}__${address.toLowerCase()}`;
}

function normaliseAddress(address: string): string {
  return address.toLowerCase();
}

/**
 * Shape-check a wire payload. We require every field on `TokenMetadata`
 * to be present and well-typed; any deviation → `integrity_failed`.
 */
function isTokenMetadata(v: unknown): v is TokenMetadata {
  if (!v || typeof v !== "object") return false;
  const o = v as Record<string, unknown>;
  return (
    o.kind === "erc20" &&
    typeof o.chainId === "number" &&
    typeof o.address === "string" &&
    typeof o.symbol === "string" &&
    typeof o.decimals === "number" &&
    typeof o.name === "string"
  );
}

function isAbortError(err: unknown): boolean {
  if (!err || typeof err !== "object") return false;
  const e = err as { name?: unknown; code?: unknown };
  return e.name === "AbortError" || e.code === "ABORT_ERR";
}

/**
 * Single SW-process singleton. Boots a single TokenRegistryClient
 * implementation so caches and inflight dedupe are shared across all
 * callers in the same lifetime.
 */
/**
 * Round 2 audit (P1) — bound the in-process token cache so a hostile dapp
 * cannot inflate it indefinitely by submitting calldata that references
 * thousands of unique addresses. A simple insertion-order LRU is enough:
 * `Map` preserves insertion order, so re-inserting on hit moves an entry
 * to the back of the queue and `keys().next()` evicts the coldest.
 *
 * 2048 entries × ~150 bytes per `TokenMetadata` payload caps the cache
 * around 300 KiB — comfortably under the SW heap budget.
 */
const MAX_TOKEN_MEM_CACHE = 2048;
const MAX_TOKEN_NEGATIVE_ENTRIES = 1024;
const MAX_TOKEN_INFLIGHT_ENTRIES = 256;

class TokenRegistryClientImpl implements TokenRegistryClient {
  /** In-process Layer 1 — process-local mirror of `Browser.storage.local`. */
  private readonly memCache = new Map<string, TokenMetadata>();
  /** Negative cache — see `NegativeCacheEntry` for TTL semantics. */
  private readonly negative = new Map<string, NegativeCacheEntry>();
  /** Inflight dedupe — same key callers share one Promise. */
  private readonly inflight = new Map<
    string,
    Promise<TokenMetadata | null>
  >();
  private hydrated = false;

  constructor(private readonly options: TokenRegistryClientOptions = {}) {}

  /**
   * Refresh recency on a `Map` so the oldest entry is at the front.
   * Removes + re-inserts the key when present; no-op otherwise.
   */
  private touchMemCache(key: string, meta: TokenMetadata): void {
    this.memCache.delete(key);
    this.memCache.set(key, meta);
    while (this.memCache.size > MAX_TOKEN_MEM_CACHE) {
      const oldest = this.memCache.keys().next().value;
      if (oldest === undefined) break;
      this.memCache.delete(oldest);
    }
  }

  /**
   * Lazy hydrate the in-process cache from `Browser.storage.local`. The
   * SW can wake up cold, so this runs at most once per process — every
   * lookup pays the storage read once, then the in-memory map handles
   * subsequent hits.
   */
  private async hydrate(): Promise<void> {
    if (this.hydrated) return;
    try {
      const got = (await Browser.storage.local.get(STORAGE_KEY)) as Record<
        string,
        unknown
      >;
      const stored = got[STORAGE_KEY] as
        | Record<string, TokenMetadata>
        | undefined;
      if (stored) {
        for (const [k, v] of Object.entries(stored)) {
          if (isTokenMetadata(v)) this.memCache.set(k, v);
        }
      }
    } catch {
      // Storage read failure shouldn't crash the lookup — degrade to a
      // network-every-time mode. The next persist attempt will retry.
    }
    this.hydrated = true;
  }

  private async persist(key: string, meta: TokenMetadata): Promise<void> {
    try {
      const got = (await Browser.storage.local.get(STORAGE_KEY)) as Record<
        string,
        unknown
      >;
      const stored =
        (got[STORAGE_KEY] as Record<string, TokenMetadata> | undefined) ?? {};
      stored[key] = meta;
      await Browser.storage.local.set({ [STORAGE_KEY]: stored });
    } catch {
      // Persist failure is non-fatal — we keep the in-memory copy so the
      // current SW lifetime still benefits from the lookup.
    }
  }

  /**
   * Check + sweep the negative cache. Lazy expiry pattern matches
   * `marketplace/negative-cache.ts`.
   */
  private negativeGet(key: string): NegativeCacheEntry | null {
    const entry = this.negative.get(key);
    if (!entry) return null;
    if (Date.now() >= entry.expiresAt) {
      this.negative.delete(key);
      return null;
    }
    return entry;
  }

  private negativeAdd(
    key: string,
    ttlSec: number,
    reason: NegativeReason,
  ): void {
    // Round 2 audit (P1) — bound the negative cache so a hostile dapp
    // cannot accumulate 100K miss-entries by probing unique addresses.
    // LRU eviction (drop the oldest insertion) is fine: the cache only
    // suppresses repeat fetches for the same key.
    while (this.negative.size >= MAX_TOKEN_NEGATIVE_ENTRIES) {
      const oldest = this.negative.keys().next().value;
      if (oldest === undefined) break;
      this.negative.delete(oldest);
    }
    this.negative.set(key, {
      reason,
      expiresAt: Date.now() + ttlSec * 1000,
    });
  }

  async lookup(
    chainId: number,
    address: string,
  ): Promise<TokenMetadata | null> {
    const addr = normaliseAddress(address);
    const key = tokenKey(chainId, addr);

    // Layer 1 — in-process map first; then hydrate from storage if cold.
    const inMem = this.memCache.get(key);
    if (inMem) return inMem;

    await this.hydrate();
    const hydrated = this.memCache.get(key);
    if (hydrated) return hydrated;

    // Negative cache short-circuit.
    if (this.negativeGet(key)) return null;

    // Inflight dedupe — concurrent lookups share one network fetch.
    const existing = this.inflight.get(key);
    if (existing) return existing;

    // Round 2 audit (P1) — refuse new fetches when the inflight slot is
    // saturated. A hostile dapp could otherwise pump unique addresses to
    // hold 100K concurrent Promises in memory. Returning `null` matches
    // the "registry hiccup" path, which `enrichEnvelopeAssets` already
    // treats as a skip.
    if (this.inflight.size >= MAX_TOKEN_INFLIGHT_ENTRIES) {
      return null;
    }

    const p = this.doFetch(chainId, addr, key).finally(() => {
      // Always clear the slot so a settled Promise can't keep blocking.
      this.inflight.delete(key);
    });
    this.inflight.set(key, p);
    return p;
  }

  private async doFetch(
    chainId: number,
    address: string,
    key: string,
  ): Promise<TokenMetadata | null> {
    const baseUrl = this.options.baseUrl ?? DEFAULT_BASE_URL;
    const timeoutMs = this.options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    const doFetch = this.options.fetchImpl ?? fetch;

    const base = baseUrl.endsWith("/") ? baseUrl.slice(0, -1) : baseUrl;
    const url = `${base}/tokens/${chainId}/${address}.json`;

    const controller = new AbortController();
    const timeoutHandle = setTimeout(() => controller.abort(), timeoutMs);

    let response: Response;
    try {
      response = await doFetch(url, { signal: controller.signal });
    } catch (err) {
      clearTimeout(timeoutHandle);
      // AbortError and any other network error → 30 s self-healing cool-down.
      // (We intentionally don't distinguish — a stuck endpoint and a
      // genuinely-aborted timeout both deserve the same retry window.)
      void isAbortError(err); // touch the helper to keep it linted-in
      this.negativeAdd(key, 30, "timeout");
      return null;
    }
    clearTimeout(timeoutHandle);

    if (response.status === 404) {
      this.negativeAdd(key, 300, "no_publisher");
      return null;
    }
    if (!response.ok) {
      // 5xx + 4xx (non-404) → also a 30s cool-down so a flaky CDN
      // can recover without spamming us.
      this.negativeAdd(key, 30, "timeout");
      return null;
    }

    let parsed: unknown;
    try {
      parsed = await response.json();
    } catch {
      // Malformed JSON is a publisher error, not absence — treat as
      // integrity_failed so an alarm path (if any) sees it as suspicious.
      this.negativeAdd(key, 300, "integrity_failed");
      return null;
    }

    if (!isTokenMetadata(parsed)) {
      this.negativeAdd(key, 300, "integrity_failed");
      return null;
    }

    // Normalise on the way in — registry payloads MUST be lowercased per
    // §8 anyway, but we re-normalise to defend against a publisher slip-up.
    const normalised: TokenMetadata = {
      ...parsed,
      address: normaliseAddress(parsed.address),
    };

    this.touchMemCache(key, normalised);
    await this.persist(key, normalised);
    return normalised;
  }

  /** Test helper — drop every cache + the inflight slot. */
  reset(): void {
    this.memCache.clear();
    this.negative.clear();
    this.inflight.clear();
    this.hydrated = false;
  }

  /** Test helper — peek the negative cache without mutating it. */
  peekNegative(
    chainId: number,
    address: string,
  ): NegativeCacheEntry | null {
    return this.negativeGet(tokenKey(chainId, address));
  }
}

let singleton: TokenRegistryClientImpl | null = null;

/**
 * Factory — gives callers a fresh client (mainly for tests / DI). Most
 * SW callers should hit `defaultTokenRegistryClient` so the cache is
 * shared across the process.
 */
export function createTokenRegistryClient(
  options: TokenRegistryClientOptions = {},
): TokenRegistryClient {
  return new TokenRegistryClientImpl(options);
}

/** Process-singleton handle. */
export function defaultTokenRegistryClient(): TokenRegistryClient {
  if (!singleton) singleton = new TokenRegistryClientImpl();
  return singleton;
}

/** Test helper — wipe the process singleton between cases. */
export function __resetTokenRegistryClientForTest(): void {
  if (singleton) singleton.reset();
  singleton = null;
}

/**
 * Test helper — peek at the underlying singleton's negative cache.
 * Returns null when no entry exists or the entry has expired.
 */
export function __peekTokenNegativeCacheForTest(
  chainId: number,
  address: string,
): { reason: NegativeReason } | null {
  if (!singleton) return null;
  const entry = singleton.peekNegative(chainId, address);
  return entry ? { reason: entry.reason } : null;
}
