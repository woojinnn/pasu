import Browser from 'webextension-polyfill';
import { aggregatedPolicySet } from './marketplace/storage';
import { getEnabledIds } from './policy-selection';
import { installPolicies } from './wasm-bridge';

let installed = false;
let inflight: Promise<void> | null = null;

interface PolicyEntry {
  id: string;
  text: string;
}

async function loadDefaultPolicySet(): Promise<PolicyEntry[]> {
  const setUrl = Browser.runtime.getURL('default-policies/policy-set.json');
  const policySetRaw = await (await fetch(setUrl)).text();
  return JSON.parse(policySetRaw) as PolicyEntry[];
}

/**
 * Build the union of (defaults ∪ marketplace) and call installPolicies()
 * with the subset whose ids appear in `enabledIds`. Empty `enabledIds`
 * ⇒ install with no policies (the engine's `engine/baseline-allow` rule
 * is auto-injected).
 *
 * NOTE on schema_text: we pass an empty string. The WASM builder
 * (`PolicyEngineBuilder::new`) already preloads the bundled schema
 * (`core + dex + other`), which declares `Wallet`/`Protocol`/etc. The
 * on-disk `default-policies/schema.cedarschema` redeclares the same
 * entities, so handing it back to `add_schema_text` would error with
 * "`Wallet` is declared twice" and kill SW boot.
 */
async function installFiltered(enabledIds: readonly string[]): Promise<void> {
  const [defaults, marketplacePolicies] = await Promise.all([
    loadDefaultPolicySet(),
    aggregatedPolicySet(),
  ]);
  const enabledSet = new Set(enabledIds);
  const union = [...defaults, ...marketplacePolicies];
  const filtered = union.filter((p) => enabledSet.has(p.id));
  await installPolicies({ schema_text: '', policy_set: filtered });
}

/**
 * Run `work()` after the previous `inflight` settles, so consecutive
 * loader calls hit `installPolicies()` in arrival order. Both call
 * sites (boot + reinstall) use this — closes the race where an older
 * IIFE's WASM call lands AFTER a newer one and silently overwrites it.
 *
 * The previous `inflight`'s rejection is intentionally swallowed here:
 * the prior call already surfaced its error to its own caller, and we
 * don't want to fail the new request because of unrelated old work.
 */
async function withSerialization(work: () => Promise<void>): Promise<void> {
  const previous = inflight;
  const promise = (async () => {
    if (previous) {
      try {
        await previous;
      } catch {
        /* prior caller already received this error */
      }
    }
    await work();
  })();
  inflight = promise;
  try {
    await promise;
  } finally {
    if (inflight === promise) inflight = null;
  }
}

/**
 * One-shot install at SW boot. Reads enabled-ids from storage. The boot
 * call can overlap with popup-driven `reinstallAllPolicies` calls if
 * the popup opens before prewarm finishes; both go through
 * `withSerialization` so WASM sees them in arrival order.
 *
 * On reject, clears the `installed` flag so the next call retries.
 */
export async function ensureDefaultPoliciesInstalled(): Promise<void> {
  if (installed) return;
  await withSerialization(async () => {
    if (installed) return; // already done by an interleaved reinstall
    try {
      const enabledIds = await getEnabledIds();
      await installFiltered(enabledIds);
      installed = true;
    } catch (err) {
      installed = false;
      throw err;
    }
  });
}

/**
 * Reinstall the engine with exactly the passed `ids` enabled. Used by
 * the popup's apply queue (`policy-selection.ts`) — the queue passes
 * the desired ids verbatim to avoid storage races. Serialized via
 * `withSerialization` so it can never race ahead of (or behind) a
 * still-resolving boot install.
 *
 * On reject, clears the `installed` flag so the next call retries.
 */
export async function reinstallAllPolicies(ids: readonly string[]): Promise<void> {
  await withSerialization(async () => {
    try {
      await installFiltered(ids);
      installed = true;
    } catch (err) {
      installed = false;
      throw err;
    }
  });
}
