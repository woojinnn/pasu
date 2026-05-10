import Browser from "webextension-polyfill";

const KEY = "windows:pending-deltas";
const COMMITTED_KEY = "windows:committed";
const TTL_MS = 5 * 60_000;

export interface PendingDelta {
  requestId: string;
  /** EVM chain id of the underlying request — required so the receipt
   *  poller queries the correct RPC. */
  chainId: number;
  actor: string;
  windowEntries: { name: string; value: string }[];
  enqueuedAtMs: number;
  txHash?: string;
}

async function load(): Promise<PendingDelta[]> {
  const v = ((await Browser.storage.local.get(KEY)) as Record<string, unknown>)[
    KEY
  ] as PendingDelta[] | undefined;
  return v ?? [];
}
async function save(list: PendingDelta[]): Promise<void> {
  await Browser.storage.local.set({ [KEY]: list });
}

export async function reservePending(req: PendingDelta): Promise<void> {
  const list = await load();
  list.push(req);
  await save(list);
}

export async function setTxHash(
  requestId: string,
  txHash: string,
): Promise<void> {
  const list = await load();
  for (const d of list) if (d.requestId === requestId) d.txHash = txHash;
  await save(list);
}

export async function commitByTxHash(
  txHash: string,
  entry: {
    chainId: number;
    actor: string;
    windowEntries: { name: string; value: string }[];
  },
): Promise<void> {
  const list = await load();
  await save(list.filter((d) => d.txHash !== txHash));

  const committed =
    ((
      (await Browser.storage.local.get(COMMITTED_KEY)) as Record<
        string,
        unknown
      >
    )[COMMITTED_KEY] as Record<string, Record<string, string>> | undefined) ??
    {};
  const actor = entry.actor.toLowerCase();
  committed[actor] = committed[actor] ?? {};
  for (const w of entry.windowEntries) {
    committed[actor][w.name] = addWindowValue(
      w.name,
      committed[actor][w.name] ?? zeroWindowValue(w.name),
      w.value,
    );
  }
  await Browser.storage.local.set({ [COMMITTED_KEY]: committed });
}

export async function discardExpired(
  nowMs: number = Date.now(),
): Promise<void> {
  const list = await load();
  await save(list.filter((d) => nowMs - d.enqueuedAtMs < TTL_MS));
}

export async function pendingForActor(
  actor: string,
): Promise<{ name: string; value: string }[]> {
  const list = await load();
  const sums = new Map<string, string>();
  for (const d of list) {
    if (d.actor.toLowerCase() !== actor.toLowerCase()) continue;
    for (const e of d.windowEntries) {
      sums.set(
        e.name,
        addWindowValue(
          e.name,
          sums.get(e.name) ?? zeroWindowValue(e.name),
          e.value,
        ),
      );
    }
  }
  return [...sums.entries()].map(([name, value]) => ({ name, value }));
}

export async function committedForActor(
  actor: string,
): Promise<{ name: string; value: string }[]> {
  const committed =
    ((
      (await Browser.storage.local.get(COMMITTED_KEY)) as Record<
        string,
        unknown
      >
    )[COMMITTED_KEY] as Record<string, Record<string, string>> | undefined) ??
    {};
  const entries = committed[actor.toLowerCase()] ?? {};
  return Object.entries(entries).map(([name, value]) => ({ name, value }));
}

export async function listPending(): Promise<PendingDelta[]> {
  return load();
}

function addWindowValue(name: string, left: string, right: string): string {
  if (name === "swapVolumeUsd24h") {
    const leftFixed = decimalToFixed(left);
    const rightFixed = decimalToFixed(right);
    if (leftFixed === undefined) return right;
    if (rightFixed === undefined) return left;
    return fixedToDecimal(leftFixed + rightFixed);
  }
  return (BigInt(left) + BigInt(right)).toString();
}

function zeroWindowValue(name: string): string {
  return name === "swapVolumeUsd24h" ? "0.0000" : "0";
}

function decimalToFixed(value: string): bigint | undefined {
  const parts = value.split(".");
  if (parts.length > 2) return undefined;
  const [whole, fraction = ""] = parts;
  if (whole === "" && fraction === "") return undefined;
  if (!/^\d*$/.test(whole) || !/^\d*$/.test(fraction)) return undefined;
  const padded = `${fraction}0000`.slice(0, 4);
  try {
    return BigInt(`${whole || "0"}${padded}`);
  } catch {
    return undefined;
  }
}

function fixedToDecimal(value: bigint): string {
  const raw = value.toString().padStart(5, "0");
  const whole = raw.slice(0, -4);
  const fraction = raw.slice(-4);
  return `${whole}.${fraction}`;
}
