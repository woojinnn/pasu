import Browser from 'webextension-polyfill';
import type { ParamValues } from './params-validator';

const KEY = 'marketplace:bundles';

export interface InstalledBundle {
  bundle_id: string;
  version: string;
  author_pubkey: string;
  paramValues: ParamValues;
  renderedPolicySet: { id: string; text: string }[];
  installedAtMs: number;
}

export async function listInstalled(): Promise<InstalledBundle[]> {
  const v = ((await Browser.storage.local.get(KEY)) as Record<string, unknown>)[KEY] as
    | InstalledBundle[]
    | undefined;
  return v ?? [];
}

export async function upsert(bundle: InstalledBundle): Promise<void> {
  const list = await listInstalled();
  const i = list.findIndex((b) => b.bundle_id === bundle.bundle_id);
  if (i >= 0) {
    // First-install pubkey pinning: refuse if pubkey differs from the
    // previously-pinned value.
    if (list[i].author_pubkey !== bundle.author_pubkey) {
      throw new Error(
        `bundle ${bundle.bundle_id} previously installed under a different ` +
          `author pubkey; refuse update`,
      );
    }
    list[i] = bundle;
  } else {
    list.push(bundle);
  }
  await Browser.storage.local.set({ [KEY]: list });
}

export async function uninstall(bundleId: string): Promise<void> {
  const list = await listInstalled();
  await Browser.storage.local.set({
    [KEY]: list.filter((b) => b.bundle_id !== bundleId),
  });
}

/** Aggregate the renderedPolicySet from every installed bundle, suitable
 *  for handing to the WASM bridge's installPolicies. */
export async function aggregatedPolicySet(): Promise<{ id: string; text: string }[]> {
  const list = await listInstalled();
  return list.flatMap((b) => b.renderedPolicySet);
}
