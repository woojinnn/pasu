/**
 * Per-origin connected-account store for the HL venue leverage enrichment.
 *
 * The HL `/exchange` body carries no master account address and orders are
 * signed by an agent key, so the SW cannot recover the master from the request
 * alone. The provider proxy captures the EVM account from `eth_requestAccounts`
 * and writes it here keyed by origin. `resolve-hl-master.ts` reads it to key
 * the `activeAssetData` leverage lookup.
 *
 * Can be seeded manually from the SW console for testing:
 *   chrome.storage.local.set({ "venue:hl-connected-accounts":
 *     { "app.hyperliquid.xyz": "0x<master>" } })
 */
import Browser from "webextension-polyfill";

const STORAGE_KEY = "venue:hl-connected-accounts";
/** Bound the map so a hostile page cannot inflate storage with origins. */
const MAX_ORIGINS = 64;
const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/i;

type AccountMap = Record<string, string>;

async function readMap(): Promise<AccountMap> {
  try {
    const got = (await Browser.storage.local.get(STORAGE_KEY)) as Record<
      string,
      unknown
    >;
    const stored = got[STORAGE_KEY];
    if (stored && typeof stored === "object") return stored as AccountMap;
  } catch {
    // Storage read failure → treat as no known account (best-effort).
  }
  return {};
}

/** The connected master account for `hostname`, lowercased, or `null`. */
export async function getConnectedAccount(
  hostname: string,
): Promise<string | null> {
  if (!hostname) return null;
  const map = await readMap();
  const addr = map[hostname];
  return typeof addr === "string" && ADDRESS_RE.test(addr)
    ? addr.toLowerCase()
    : null;
}

/**
 * Record the connected master account for `hostname` (idempotent). Invalid
 * addresses are ignored; the map is bounded by {@link MAX_ORIGINS}.
 */
export async function setConnectedAccount(
  hostname: string,
  address: string,
): Promise<void> {
  if (!hostname || !ADDRESS_RE.test(address)) return;
  try {
    const map = await readMap();
    if (map[hostname] === address.toLowerCase()) return;
    // Bound: drop an arbitrary existing origin when at capacity (the active
    // origin is re-added below, so the current trader is never evicted).
    const keys = Object.keys(map);
    if (keys.length >= MAX_ORIGINS && !(hostname in map)) {
      delete map[keys[0]];
    }
    map[hostname] = address.toLowerCase();
    await Browser.storage.local.set({ [STORAGE_KEY]: map });
  } catch {
    // Persist failure is non-fatal — leverage enrichment just stays dormant.
  }
}

/** Clear an origin's recorded account (e.g. on `accountsChanged` to empty). */
export async function clearConnectedAccount(hostname: string): Promise<void> {
  if (!hostname) return;
  try {
    const map = await readMap();
    if (hostname in map) {
      delete map[hostname];
      await Browser.storage.local.set({ [STORAGE_KEY]: map });
    }
  } catch {
    // Non-fatal.
  }
}
