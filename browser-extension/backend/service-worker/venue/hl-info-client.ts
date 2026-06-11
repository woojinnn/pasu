/**
 * Hyperliquid public `/info` client — venue account-state enrichment.
 *
 * The HL `/exchange` `order` wire carries only the order intent; the risk context
 * a policy gates on (effective leverage, notional USD, account margin health, an
 * existing position's PnL / liquidation proximity) lives in per-(user,asset)
 * venue account state. The service-worker fetches that state here and injects it
 * into the v2 evaluate input — mirroring how the registry `token-client.ts`
 * resolves decimals for the `amountNano` enrichment.
 *
 * The `/info` endpoint is a single URL dispatched by the POST body's `type`; this
 * client issues three queries, all unauthenticated `POST {base}/info`:
 *   - `meta` (`{type:"meta"}`) — `universe[i].{name, maxLeverage}` (asset_index →
 *     symbol + max-leverage tier). Near-static → cached for hours.
 *   - `activeAssetData` (`{type:"activeAssetData",user,coin}`) — `leverage.value`
 *     (CONFIGURED leverage even with no open position, verified against the live
 *     API), `leverage.type` (cross/isolated), and `markPx`. Cached per-(user,coin),
 *     short TTL + inflight-dedupe. [`leverageFor`] delegates to [`activeAssetDataFor`]
 *     so the leverage path and the order-enrichment path share ONE fetch.
 *   - `clearinghouseState` (`{type:"clearinghouseState",user}`) — account
 *     `marginSummary` (accountValue / totalMarginUsed) + per-position
 *     `returnOnEquity` / `liquidationPx` / `szi`. Changes every fill → very short
 *     TTL (a single batch POST reads it once), inflight-deduped.
 *
 * NON-FATAL by design (mirrors `token-client.ts`): a fetch error / timeout / miss
 * yields `null`, so the lowering omits the optional field and a `context has …`
 * policy stays dormant — a transient HL hiccup must NOT over-block a venue order
 * (which is otherwise deny-closed).
 *
 * CORS: the SW (extension origin) fetch is covered by the manifest
 * `host_permissions` (`<all_urls>`), so the cross-origin `/info` POST is allowed
 * without a preflight gate.
 */

/** Mainnet / testnet HL info endpoints. */
const HL_INFO_MAINNET = "https://api.hyperliquid.xyz/info";
const HL_INFO_TESTNET = "https://api.hyperliquid-testnet.xyz/info";

/** Universe (asset_index → symbol / maxLeverage) is near-static; cache for 6h. */
const META_TTL_MS = 6 * 60 * 60 * 1000;
/** Leverage / markPx are mutable; short TTL + updateLeverage refresh. */
const ASSET_DATA_TTL_MS = 30 * 1000;
/** clearinghouseState changes every fill → very short TTL (one batch reads once). */
const CLEARINGHOUSE_TTL_MS = 5 * 1000;
/** Per-request timeout — well under orchestrator HARD_TIMEOUT_MS (8000). */
const DEFAULT_TIMEOUT_MS = 1500;

/** Spot asset indices are `10000 + spotIdx`; spot has no leverage. */
const SPOT_INDEX_BASE = 10000;

/** Bound the caches so a hostile page cannot inflate them indefinitely. */
const MAX_ASSET_DATA_ENTRIES = 2048;
const MAX_CLEARINGHOUSE_ENTRIES = 512;
const MAX_INFLIGHT_ENTRIES = 256;

export interface HlInfoClientOptions {
  /** Override the info base URL (tests / testnet). */
  baseUrl?: string;
  timeoutMs?: number;
  /** Injected for tests — defaults to global `fetch`. */
  fetchImpl?: typeof fetch;
}

/** Parsed `activeAssetData` (per user,coin). Fields are `null` when absent. */
export interface ActiveAssetData {
  /** `leverage.value` — configured effective leverage (positive integer). */
  leverage: number | null;
  /** `leverage.type` — `"cross"` | `"isolated"`. */
  leverageType: string | null;
  /** `markPx` — current mark price (numeric). */
  markPx: number | null;
  /**
   * `availableToTrade` — USABLE COLLATERAL in USD for this (user,coin), spot
   * balances INCLUDED (NOT a leveraged notional cap — `maxTradeSzs` is that;
   * proven live: `maxTradeSzs × markPx / availableToTrade == leverage`). HL
   * returns `[buy, sell]`; we keep the conservative `min` (the opening
   * direction). `null` when absent. Used to make the account margin-utilization
   * ratio spot-aware instead of perp-only.
   */
  availableToTrade: number | null;
}

/** One open perp position from `clearinghouseState.assetPositions[].position`. */
export interface ClearinghousePosition {
  /** Return-on-equity as a ratio (e.g. `-0.15` = −15%); signed. */
  returnOnEquity: number | null;
  /** Liquidation price (numeric). */
  liquidationPx: number | null;
  /** Signed position size. */
  szi: number | null;
}

/** Parsed `clearinghouseState` (per user). */
export interface ClearinghouseState {
  /** `marginSummary.accountValue` — account equity (USD). */
  accountValue: number | null;
  /** `marginSummary.totalMarginUsed` (USD). */
  totalMarginUsed: number | null;
  /** Open positions keyed by coin (e.g. `"BTC"`). */
  positions: Map<string, ClearinghousePosition>;
}

interface MetaUniverseEntry {
  name: string;
  maxLeverage: number | null;
}

interface CacheEntry<T> {
  value: T;
  fetchedAtMs: number;
}

interface MetaEntry {
  universe: MetaUniverseEntry[];
  fetchedAtMs: number;
}

function assetDataKey(user: string, coin: string): string {
  return `${user.toLowerCase()}:${coin}`;
}

/** Parse a value that HL returns as a numeric string (`"61866.0"`) to a number. */
function parseNum(value: unknown): number | null {
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  if (typeof value === "string" && value.length > 0) {
    const n = Number(value);
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

/**
 * Parse HL `availableToTrade` — a `[buy, sell]` pair of USD collateral strings —
 * to the conservative `min` (the opening direction is the smaller side when a
 * position already exists). `null` when the shape is unusable.
 */
function parseAvailableToTrade(value: unknown): number | null {
  if (!Array.isArray(value)) return null;
  const nums = value
    .map(parseNum)
    .filter((n): n is number => n !== null && n >= 0);
  return nums.length > 0 ? Math.min(...nums) : null;
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
  private metaInflight: Promise<MetaUniverseEntry[] | null> | null = null;
  private readonly assetData = new Map<string, CacheEntry<ActiveAssetData>>();
  private readonly assetDataInflight = new Map<
    string,
    Promise<ActiveAssetData | null>
  >();
  private readonly clearinghouse = new Map<
    string,
    CacheEntry<ClearinghouseState>
  >();
  private readonly clearinghouseInflight = new Map<
    string,
    Promise<ClearinghouseState | null>
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

  // ── meta (asset_index → symbol / maxLeverage) ─────────────────────────────

  /** Fetch + cache the perp universe (asset_index → {name, maxLeverage}). */
  private async universe(): Promise<MetaUniverseEntry[] | null> {
    const fresh =
      this.meta && Date.now() - this.meta.fetchedAtMs < META_TTL_MS
        ? this.meta.universe
        : null;
    if (fresh) return fresh;
    if (this.metaInflight) return this.metaInflight;

    const p = (async (): Promise<MetaUniverseEntry[] | null> => {
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
    const name = universe[assetIndex]?.name;
    return typeof name === "string" && name.length > 0 ? name : null;
  }

  /** Max-leverage tier for a perp `asset_index` (meta universe), or `null`. */
  async maxLeverageForIndex(assetIndex: number): Promise<number | null> {
    if (!Number.isInteger(assetIndex) || assetIndex < 0) return null;
    if (assetIndex >= SPOT_INDEX_BASE) return null;
    const universe = await this.universe();
    return universe?.[assetIndex]?.maxLeverage ?? null;
  }

  // ── activeAssetData (per user,coin: leverage / type / markPx) ──────────────

  /**
   * Full `activeAssetData` for (user, coin) — `{ leverage, leverageType, markPx }`
   * — or `null` on a fetch failure. Cached per-(user,coin) with a short TTL +
   * inflight-dedupe; [`leverageFor`] delegates here so the leverage path and the
   * order-enrichment path share ONE network fetch.
   */
  async activeAssetDataFor(
    user: string,
    coin: string,
  ): Promise<ActiveAssetData | null> {
    const key = assetDataKey(user, coin);
    const cached = this.assetData.get(key);
    if (cached && Date.now() - cached.fetchedAtMs < ASSET_DATA_TTL_MS) {
      return cached.value;
    }
    const existing = this.assetDataInflight.get(key);
    if (existing) return existing;
    if (this.assetDataInflight.size >= MAX_INFLIGHT_ENTRIES) return null;

    const p = (async (): Promise<ActiveAssetData | null> => {
      const parsed = await this.post({ type: "activeAssetData", user, coin });
      if (parsed === null) return null;
      const data = extractActiveAssetData(parsed);
      // Cache only when something useful resolved (mirrors the old value-gated
      // cache); a fully-empty parse is not cached so a later success can fill it.
      if (data.leverage !== null || data.markPx !== null) {
        this.storeAssetData(key, data);
      }
      return data;
    })().finally(() => {
      this.assetDataInflight.delete(key);
    });
    this.assetDataInflight.set(key, p);
    return p;
  }

  /** Effective leverage for (user, coin) — the leverage path's contract. */
  async leverageFor(user: string, coin: string): Promise<number | null> {
    const data = await this.activeAssetDataFor(user, coin);
    return data?.leverage ?? null;
  }

  private storeAssetData(key: string, value: ActiveAssetData): void {
    this.assetData.delete(key);
    this.assetData.set(key, { value, fetchedAtMs: Date.now() });
    while (this.assetData.size > MAX_ASSET_DATA_ENTRIES) {
      const oldest = this.assetData.keys().next().value;
      if (oldest === undefined) break;
      this.assetData.delete(oldest);
    }
  }

  /**
   * Seed / refresh the leverage cache for (user, coin) — called when the SW
   * intercepts an `updateLeverage` for this pair, so the next order sees the
   * just-set value even within the TTL. Stores a leverage-only entry (markPx /
   * type are left unknown until the next authoritative fetch).
   */
  set(user: string, coin: string, value: number): void {
    this.storeAssetData(assetDataKey(user, coin), {
      leverage: value,
      leverageType: null,
      markPx: null,
      availableToTrade: null,
    });
  }

  /** Drop the cached activeAssetData for (user, coin). */
  invalidate(user: string, coin: string): void {
    this.assetData.delete(assetDataKey(user, coin));
  }

  // ── clearinghouseState (per user: margin health + positions) ──────────────

  /** Account-wide perp state for `user` (margin summary + open positions). */
  async clearinghouseStateFor(user: string): Promise<ClearinghouseState | null> {
    const key = user.toLowerCase();
    const cached = this.clearinghouse.get(key);
    if (cached && Date.now() - cached.fetchedAtMs < CLEARINGHOUSE_TTL_MS) {
      return cached.value;
    }
    const existing = this.clearinghouseInflight.get(key);
    if (existing) return existing;
    if (this.clearinghouseInflight.size >= MAX_INFLIGHT_ENTRIES) return null;

    const p = (async (): Promise<ClearinghouseState | null> => {
      const parsed = await this.post({ type: "clearinghouseState", user });
      if (parsed === null) return null;
      const state = extractClearinghouseState(parsed);
      this.clearinghouse.delete(key);
      this.clearinghouse.set(key, { value: state, fetchedAtMs: Date.now() });
      while (this.clearinghouse.size > MAX_CLEARINGHOUSE_ENTRIES) {
        const oldest = this.clearinghouse.keys().next().value;
        if (oldest === undefined) break;
        this.clearinghouse.delete(oldest);
      }
      return state;
    })().finally(() => {
      this.clearinghouseInflight.delete(key);
    });
    this.clearinghouseInflight.set(key, p);
    return p;
  }

  /** Test helper — clear every cache. */
  reset(): void {
    this.meta = null;
    this.metaInflight = null;
    this.assetData.clear();
    this.assetDataInflight.clear();
    this.clearinghouse.clear();
    this.clearinghouseInflight.clear();
  }
}

/** `{type:"meta"}` → `universe[i].{name, maxLeverage}`, or `null` on a bad shape. */
function extractUniverse(parsed: unknown): MetaUniverseEntry[] | null {
  if (!parsed || typeof parsed !== "object") return null;
  const u = (parsed as { universe?: unknown }).universe;
  if (!Array.isArray(u)) return null;
  return u.map((e) => {
    const o = e && typeof e === "object" ? (e as Record<string, unknown>) : {};
    return {
      name: typeof o.name === "string" ? o.name : "",
      maxLeverage: parseNum(o.maxLeverage),
    };
  });
}

/** `{type:"activeAssetData"}` → `{ leverage, leverageType, markPx }`. */
function extractActiveAssetData(parsed: unknown): ActiveAssetData {
  const empty: ActiveAssetData = {
    leverage: null,
    leverageType: null,
    markPx: null,
    availableToTrade: null,
  };
  if (!parsed || typeof parsed !== "object") return empty;
  const o = parsed as Record<string, unknown>;
  let leverage: number | null = null;
  let leverageType: string | null = null;
  const lev = o.leverage;
  if (lev && typeof lev === "object") {
    const v = (lev as { value?: unknown }).value;
    if (typeof v === "number" && Number.isFinite(v) && v > 0) {
      leverage = Math.trunc(v);
    }
    const t = (lev as { type?: unknown }).type;
    if (typeof t === "string" && t.length > 0) leverageType = t;
  }
  return {
    leverage,
    leverageType,
    markPx: parseNum(o.markPx),
    availableToTrade: parseAvailableToTrade(o.availableToTrade),
  };
}

/** `{type:"clearinghouseState"}` → margin summary + per-coin positions. */
function extractClearinghouseState(parsed: unknown): ClearinghouseState {
  const positions = new Map<string, ClearinghousePosition>();
  const out: ClearinghouseState = {
    accountValue: null,
    totalMarginUsed: null,
    positions,
  };
  if (!parsed || typeof parsed !== "object") return out;
  const o = parsed as Record<string, unknown>;

  const ms = o.marginSummary;
  if (ms && typeof ms === "object") {
    const m = ms as Record<string, unknown>;
    out.accountValue = parseNum(m.accountValue);
    out.totalMarginUsed = parseNum(m.totalMarginUsed);
  }

  const aps = o.assetPositions;
  if (Array.isArray(aps)) {
    for (const ap of aps) {
      const pos =
        ap && typeof ap === "object"
          ? (ap as { position?: unknown }).position
          : null;
      if (!pos || typeof pos !== "object") continue;
      const p = pos as Record<string, unknown>;
      const coin = typeof p.coin === "string" ? p.coin : null;
      if (!coin) continue;
      positions.set(coin, {
        returnOnEquity: parseNum(p.returnOnEquity),
        liquidationPx: parseNum(p.liquidationPx),
        szi: parseNum(p.szi),
      });
    }
  }
  return out;
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
