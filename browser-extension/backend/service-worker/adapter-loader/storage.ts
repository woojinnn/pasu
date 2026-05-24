import Browser from "webextension-polyfill";
import type { ParamValues } from "./params-validator";

const KEY = "adapter-loader:bundles";

export interface RenderedPolicyEntry {
  id: string;
  text: string;
  manifest?: unknown;
  manifests?: readonly unknown[];
}

export interface InstalledBundle {
  bundle_id: string;
  version: string;
  author_pubkey: string;
  paramValues: ParamValues;
  renderedPolicySet: RenderedPolicyEntry[];
  installedAtMs: number;
}

export async function listInstalled(): Promise<InstalledBundle[]> {
  const v = ((await Browser.storage.local.get(KEY)) as Record<string, unknown>)[
    KEY
  ] as InstalledBundle[] | undefined;
  return v ?? [];
}

/**
 * Insert or update an installed bundle. First-install pubkey pinning is
 * enforced: subsequent updates MUST be signed by the same author pubkey
 * as the original install.
 *
 * Known limitation: there is no key-rotation escape hatch. If a legitimate
 * author rotates their key, every existing user is locked out of updates
 * until they manually uninstall and reinstall. Plan 6+ may add a signed
 * `key-rotation.json` companion in the catalog (signature by both old +
 * new keys) to authorize rotation; for v1 the user-uninstall-reinstall
 * path is the only route.
 */
export async function upsert(bundle: InstalledBundle): Promise<void> {
  const list = await listInstalled();
  const i = list.findIndex((b) => b.bundle_id === bundle.bundle_id);
  if (i >= 0) {
    if (list[i].author_pubkey !== bundle.author_pubkey) {
      throw new Error(
        `bundle ${bundle.bundle_id} previously installed under a different ` +
          `author pubkey; refuse update (uninstall manually to override)`,
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
export async function aggregatedPolicySet(): Promise<
  RenderedPolicyEntry[]
> {
  const list = await listInstalled();
  return list.flatMap((b) => b.renderedPolicySet);
}
