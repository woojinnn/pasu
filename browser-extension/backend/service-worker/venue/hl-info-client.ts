/**
 * Hyperliquid public `/info` client — venue account-state enrichment.
 *
 * The HL `/exchange` `order` wire carries NO leverage; the effective leverage is
 * per-(user,asset) account state the venue applies at fill (set via
 * `updateLeverage`). To let a policy gate an ORDER on its effective leverage,
 * the service-worker fetches that state here and injects it into the v2
 * evaluate input (`account_leverage`) — exactly mirroring how the registry
 * `token-client.ts` resolves decimals for the `amountNano` enrichment.
 *
 * Two cached lookups, both against the unauthenticated `POST {base}/info`:
 *   - `coinForIndex(i)` — `{type:"meta"}` `universe[i].name` (asset_index →
 *     symbol). The universe is near-static, so it is cached for hours.
 *   - `leverageFor(user, coin)` — `{type:"activeAssetData",user,coin}`
 *     `leverage.value`. activeAssetData returns the CONFIGURED leverage even
 *     when the user holds NO position in `coin` (verified against the live
 *     API), which `clearinghouseState.assetPositions` (open-position-only) does
 *     not. Cached per-(user,coin) with a short TTL + inflight-dedupe; the SW
 *     refreshes the entry when it intercepts an `updateLeverage` for that pair.
 *
 * NON-FATAL by design (mirrors `token-client.ts`): a fetch error / timeout /
 * miss yields `null`, so the lowering omits the optional `leverage` field and a
 * `context has leverage` policy stays dormant — a transient HL hiccup must NOT
 * over-block a venue order (which is otherwise deny-closed).
 *
 * CORS: the SW (extension origin) fetch is covered by the manifest
 * `host_permissions` (`<all_urls>`), so the cross-origin `/info` POST is allowed
 * without a preflight gate.
 */

/** Mainnet / testnet HL info endpoints. */
const HL_INFO_MAINNET = "https://api.hyperliquid.xyz/info";
const HL_INFO_TESTNET = "https://api.hyperliquid-testnet.xyz/info";

/** Universe (asset_index → symbol) is near-static; cache for 6h. */
const META_TTL_MS = 6 * 60 * 60 * 1000;
/** Leverage is mutable (updateLeverage); short TTL + updateLeverage refresh. */
const LEVERAGE_TTL_MS = 30 * 1000;
/** Per-request timeout — well under orchestrator HARD_TIMEOUT_MS (8000). */
const DEFAULT_TIMEOUT_MS = 1500;

/** Spot asset indices are `10000 + spotIdx`; spot has no leverage. */
const SPOT_INDEX_BASE = 10000;

/** Bound the leverage cache so a hostile page cannot inflate it indefinitely. */
const MAX_LEVERAGE_ENTRIES = 2048;
const MAX_INFLIGHT_ENTRIES = 256;

export interface HlInfoClientOptions {
  /** Override the info base URL (tests / testnet). */
  baseUrl?: string;
  timeoutMs?: number;
  /** Injected for tests — defaults to global `fetch`. */
  fetchImpl?: typeof fetch;
}

interface LeverageEntry {
  value: number;
  fetchedAtMs: number;
}

interface MetaEntry {
  universe: string[];
  fetchedAtMs: number;
}

function leverageKey(user: string, coin: string): string {
  return `${user.toLowerCase()}:${coin}`;
}

/**
 * Pick the info endpoint for a given `/exchange` endpoint/hostname. Mainnet by
 * default; testnet when the venue host carries `-testnet` (matches the
 * fetch-hook venue regex `api(-ui)?\.hyperliquid(-testnet)?\.xyz`).
 */
export function infoBaseForEndpoint(endpointOrHost: string | undefined): string {
  return endpointOrHost && /hyperliquid-testnet/.test(endpointOrHost)
    ? HL_INFO_TESTNET
    : HL_INFO_MAINNET;
}

export class HlInfoClient {
  private meta: MetaEntry | null = null;
  private metaInflight: Promise<string[] | null> | null = null;
  private readonly leverage = new Map<string, LeverageEntry>();
  private readonly leverageInflight = new Map<
    string,
    Promise<number | null>
  >();

  constructor(private readonly options: HlInfoClientOptions = {}) {}

  private base(): string {
    return this.options.baseUrl ?? HL_INFO_MAINNET;
  }

  /** POST a `/info` query, returning the parsed JSON or `null` on any failure. */
  private async post(body: unknown): Promise<unknown | null> {
    const timeoutMs = this.options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    const doFetch = this.options.fetchImpl ?? fetch;
    const controller = new AbortController();
    const handle = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const res = await doFetch(this.base(), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
        signal: controller.signal,
      });
      if (!res.ok) return null;
      return (await res.json()) as unknown;
    } catch {
      // Network error / timeout / abort → treated as a miss (best-effort).
      return null;
    } finally {
      clearTimeout(handle);
    }
  }

  /** Fetch + cache the perp universe (asset_index → symbol). */
  private async universe(): Promise<string[] | null> {
    const fresh =
      this.meta && Date.now() - this.meta.fetchedAtMs < META_TTL_MS
        ? this.meta.universe
        : null;
    if (fresh) return fresh;
    if (this.metaInflight) return this.metaInflight;

    const p = (async (): Promise<string[] | null> => {
      const parsed = await this.post({ type: "meta" });
      const universe = extractUniverse(parsed);
      if (universe) this.meta = { universe, fetchedAtMs: Date.now() };
      return universe;
    })().finally(() => {
      this.metaInflight = null;
    });
    this.metaInflight = p;
    return p;
  }

  /** Resolve a perp `asset_index` to its symbol (e.g. 0 → "BTC"), or `null`. */
  async coinForIndex(assetIndex: number): Promise<string | null> {
    // Spot indices (>= 10000) have no perp leverage — skip (caller omits).
    if (!Number.isInteger(assetIndex) || assetIndex < 0) return null;
    if (assetIndex >= SPOT_INDEX_BASE) return null;
    const universe = await this.universe();
    if (!universe) return null;
    const name = universe[assetIndex];
    return typeof name === "string" && name.length > 0 ? name : null;
  }

  /** Effective leverage for (user, coin) from `activeAssetData`, or `null`. */
  async leverageFor(user: string, coin: string): Promise<number | null> {
    const key = leverageKey(user, coin);
    const cached = this.leverage.get(key);
    if (cached && Date.now() - cached.fetchedAtMs < LEVERAGE_TTL_MS) {
      return cached.value;
    }
    const existing = this.leverageInflight.get(key);
    if (existing) return existing;
    if (this.leverageInflight.size >= MAX_INFLIGHT_ENTRIES) return null;

    const p = (async (): Promise<number | null> => {
      const parsed = await this.post({
        type: "activeAssetData",
        user,
        coin,
      });
      const value = extractLeverageValue(parsed);
      if (value !== null) this.set(user, coin, value);
      return value;
    })().finally(() => {
      this.leverageInflight.delete(key);
    });
    this.leverageInflight.set(key, p);
    return p;
  }

  /**
   * Seed / refresh the leverage cache for (user, coin) — called when the SW
   * intercepts an `updateLeverage` for this pair, so the next order sees the
   * just-set value even within the TTL (free invalidation, no extra fetch).
   */
  set(user: string, coin: string, value: number): void {
    const key = leverageKey(user, coin);
    this.leverage.delete(key);
    this.leverage.set(key, { value, fetchedAtMs: Date.now() });
    while (this.leverage.size > MAX_LEVERAGE_ENTRIES) {
      const oldest = this.leverage.keys().next().value;
      if (oldest === undefined) break;
      this.leverage.delete(oldest);
    }
  }

  /** Drop the cached leverage for (user, coin). */
  invalidate(user: string, coin: string): void {
    this.leverage.delete(leverageKey(user, coin));
  }

  /** Test helper — clear every cache. */
  reset(): void {
    this.meta = null;
    this.metaInflight = null;
    this.leverage.clear();
    this.leverageInflight.clear();
  }
}

/** `{type:"meta"}` → `universe[i].name` array, or `null` on a bad shape. */
function extractUniverse(parsed: unknown): string[] | null {
  if (!parsed || typeof parsed !== "object") return null;
  const u = (parsed as { universe?: unknown }).universe;
  if (!Array.isArray(u)) return null;
  const names = u.map((e) =>
    e && typeof e === "object" && typeof (e as { name?: unknown }).name === "string"
      ? (e as { name: string }).name
      : "",
  );
  return names;
}

/** `{type:"activeAssetData"}` → integer `leverage.value`, or `null`. */
function extractLeverageValue(parsed: unknown): number | null {
  if (!parsed || typeof parsed !== "object") return null;
  const lev = (parsed as { leverage?: unknown }).leverage;
  if (!lev || typeof lev !== "object") return null;
  const value = (lev as { value?: unknown }).value;
  return typeof value === "number" && Number.isFinite(value) && value > 0
    ? Math.trunc(value)
    : null;
}

let singleton: HlInfoClient | null = null;

/** Process-singleton handle (shared caches across the SW lifetime). */
export function defaultHlInfoClient(): HlInfoClient {
  if (!singleton) singleton = new HlInfoClient();
  return singleton;
}

/** Test helper — reset the process singleton between cases. */
export function __resetHlInfoClientForTest(): void {
  if (singleton) singleton.reset();
  singleton = null;
}
