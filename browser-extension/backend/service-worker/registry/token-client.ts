/**
 * Token metadata registry client.
 *
 * Spec: `ADAPTER_LOADER_ARCHITECTURE.md` §8 (`host:token_metadata` enrichment)
 * and §7 (3-layer loading).
 *
 * Lookup order:
 *   1. In-process map + persistent `Browser.storage.local` cache — hit returns
 *      immediately.
 *   2. Negative cache keyed by `${chainId}__${address}`.
 *      TTL: `no_publisher` / `integrity_failed` 5 min, `timeout` 30 s.
 *   3. Inflight dedupe — concurrent lookups for the same key share one Promise.
 *   4. HTTP fetch from `${REGISTRY_BASE_URL}/tokens/${chainId}/${address}.json`.
 *
 * Every input address is lowercased before use as a cache key, URL component,
 * or persisted payload — the registry filenames are lowercase, so we must
 * normalise to avoid case-sensitive 404s.
 */
import Browser from "webextension-polyfill";
import { fetchStarted, fetchEnded } from "../diagnostics";

/**
 * Tagged metadata record returned for a registered token. `kind` is a
 * discriminator so we can extend the registry to ERC-721 / native /
 * other token kinds without changing the cache layout.
 */
export interface TokenMetadata {
  // Wire field is `erc_kind` (registry JSON uses `erc_kind`, not `kind`).
  // Must match exactly so `isTokenMetadata` accepts live payloads.
  erc_kind: "erc20";
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
  typeof process !== "undefined" && process.env && process.env.REGISTRY_BASE_URL
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
    o.erc_kind === "erc20" &&
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

// In-process cache is bounded so a hostile dapp cannot inflate it by
// submitting calldata with thousands of unique addresses. Insertion-order
// LRU via `Map`: re-inserting on hit moves the entry to the back;
// `keys().next()` evicts the coldest. 2048 entries × ~150 bytes ≈ 300 KiB.
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
   * `adapter-loader/negative-cache.ts`.
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
    // Bound the negative cache (same hostile-dapp concern as memCache).
    // LRU eviction: the cache only suppresses repeat fetches for the same key.
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

    // Refuse new fetches when the inflight table is saturated — a hostile
    // dapp pumping unique addresses would otherwise hold unbounded Promises.
    // `null` degrades to the "registry hiccup" path without crashing the SW.
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

    const sentAtMs = Date.now();
    const startedAt = performance.now();
    const traceSeq = fetchStarted("token", url);
    console.info("[Pasu] registry-fetch → sent", {
      label: "token",
      url,
      sentAt: new Date(sentAtMs).toISOString(),
    });

    let response: Response;
    try {
      response = await doFetch(url, { signal: controller.signal });
      fetchEnded(
        traceSeq,
        response.status,
        Math.round(performance.now() - startedAt),
      );
      console.info("[Pasu] registry-fetch ← recv", {
        label: "token",
        url,
        sentAt: new Date(sentAtMs).toISOString(),
        receivedAt: new Date().toISOString(),
        durationMs: Math.round(performance.now() - startedAt),
        status: response.status,
      });
    } catch (err) {
      clearTimeout(timeoutHandle);
      fetchEnded(
        traceSeq,
        `error:${err instanceof Error ? err.message : String(err)}`,
        Math.round(performance.now() - startedAt),
      );
      console.warn("[Pasu] registry-fetch ✗ error", {
        label: "token",
        url,
        sentAt: new Date(sentAtMs).toISOString(),
        durationMs: Math.round(performance.now() - startedAt),
        error: err instanceof Error ? err.message : String(err),
      });
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
