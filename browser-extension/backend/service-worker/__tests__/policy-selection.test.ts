import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  const get = async (keys?: string | string[] | Record<string, unknown>) => {
    if (keys === undefined || keys === null)
      return Object.fromEntries(localStore.entries());
    const out: Record<string, unknown> = {};
    if (typeof keys === 'string') {
      out[keys] = localStore.get(keys);
      return out;
    }
    if (Array.isArray(keys))
      for (const k of keys) out[k] = localStore.get(k);
    else
      for (const [k, fb] of Object.entries(keys))
        out[k] = localStore.has(k) ? localStore.get(k) : fb;
    return out;
  };
  return {
    localStore,
    listInstalled: vi.fn(async () => [] as unknown[]),
    fetched: { defaults: '[]' as string, schema: '' as string },
    browser: {
      runtime: { getURL: (p: string) => `chrome-extension://x/${p}` },
      storage: {
        local: {
          get: vi.fn(get),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(entries)) localStore.set(k, v);
          }),
        },
      },
    },
  };
});

vi.mock('webextension-polyfill', () => ({ default: mocks.browser }));
vi.mock('@background/adapter-loader/storage', () => ({
  listInstalled: mocks.listInstalled,
}));
vi.mock('@background/dashboard/storage', () => ({
  listManaged: vi.fn(async () => []),
}));

const fetchMock = vi.fn(async (url: string) => {
  if (url.endsWith('policy-set-v2.json')) return new Response(mocks.fetched.defaults);
  return new Response(mocks.fetched.schema);
});
vi.stubGlobal('fetch', fetchMock);

import {
  applyEnabledIds,
  getAppliedIds,
  getCatalog,
  getEnabledIds,
} from '../policy-selection';

const POLICY_A = `@id("default::dex/a")
@severity("deny") @reason("a")
forbid (principal, action, resource);`;
const POLICY_B = `@id("default::dex/b")
@severity("warn") @reason("b")
forbid (principal, action, resource);`;

describe('policy-selection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    mocks.fetched.defaults = JSON.stringify([
      { id: 'default::dex/a', policy: POLICY_A },
      { id: 'default::dex/b', policy: POLICY_B },
    ]);
    mocks.listInstalled.mockResolvedValue([]);
  });

  it('returns empty enabled and applied ids on a fresh install', async () => {
    expect(await getEnabledIds()).toEqual([]);
    expect(await getAppliedIds()).toEqual([]);
  });

  it('persists desired ids and runs the reinstall callback', async () => {
    const reinstall = vi.fn(async (_ids: string[]) => {});
    const result = await applyEnabledIds(['default::dex/a'], reinstall);
    expect(result).toEqual({ ok: true });
    expect(reinstall).toHaveBeenCalledOnce();
    expect(reinstall).toHaveBeenCalledWith(['default::dex/a']);
    expect(await getEnabledIds()).toEqual(['default::dex/a']);
    expect(await getAppliedIds()).toEqual(['default::dex/a']);
  });

  it('collapses three rapid applyEnabledIds calls into one in-flight + one tail', async () => {
    const reinstall = vi.fn(
      (_ids: string[]) => new Promise<void>((resolve) => setTimeout(resolve, 5)),
    );
    const p1 = applyEnabledIds(['default::dex/a'], reinstall);
    const p2 = applyEnabledIds(['default::dex/b'], reinstall);
    const p3 = applyEnabledIds(['default::dex/a', 'default::dex/b'], reinstall);
    const [r1, r2, r3] = await Promise.all([p1, p2, p3]);
    expect(r1).toEqual({ ok: true });
    expect(r2).toEqual({ ok: true });
    expect(r3).toEqual({ ok: true });
    expect(reinstall).toHaveBeenCalledTimes(2);
    expect(await getAppliedIds()).toEqual(['default::dex/a', 'default::dex/b']);
  });

  it('leaves applied-ids unchanged when reinstall rejects, and recovers next call', async () => {
    const failing = vi.fn(async (_ids: string[]) => {
      throw new Error('install_failed: boom');
    });
    const r1 = await applyEnabledIds(['default::dex/a'], failing);
    expect(r1.ok).toBe(false);
    if (!r1.ok) {
      expect(r1.error.kind).toBe('install_failed');
      expect(r1.error.message).toContain('boom');
    }
    expect(await getAppliedIds()).toEqual([]);

    const ok = vi.fn(async (_ids: string[]) => {});
    const r2 = await applyEnabledIds(['default::dex/b'], ok);
    expect(r2).toEqual({ ok: true });
    expect(await getAppliedIds()).toEqual(['default::dex/b']);
  });

  it('builds a catalog with per-bundle sections and stale ids filtered from the count', async () => {
    mocks.listInstalled.mockResolvedValue([
      {
        bundle_id: 'acme/v1',
        version: '0.1.0',
        author_pubkey: 'k',
        paramValues: {},
        renderedPolicySet: [
          { id: 'acme/v1::guard', text: '@id("acme/v1/guard") @severity("warn") @reason("g") forbid (principal, action, resource);' },
        ],
        installedAtMs: 0,
      },
    ]);
    await applyEnabledIds(
      ['default::dex/a', 'stale::id/gone'],
      vi.fn(async (_ids: string[]) => {}),
    );
    const cat = await getCatalog();
    expect(cat.policies.map((p) => p.id)).toEqual([
      'default::dex/a',
      'default::dex/b',
      'acme/v1::guard',
    ]);
    expect(cat.policies[0].sourceLabel).toBe('default::dex');
    expect(cat.policies[2].sourceLabel).toBe('acme/v1@0.1.0');
    expect(cat.enabled).toEqual(['default::dex/a']);
    expect(cat.applied).toEqual(['default::dex/a']);
  });

  it('settles queued resolvers even when a queued runApply fails', async () => {
    // The IIFE closes over the first call's reinstall; queued calls' reinstall
    // args are discarded by design. To genuinely exercise Bug 3 (resolver hang
    // when the *queued* runApply throws), use one reinstall whose 2nd
    // invocation (the tail) fails.
    let n = 0;
    const reinstall = vi.fn(async (_ids: string[]) => {
      n++;
      if (n === 1) {
        await new Promise<void>((resolve) => setTimeout(resolve, 5));
        return;
      }
      throw new Error('install_failed: tail-broke');
    });
    const p1 = applyEnabledIds(['default::dex/a'], reinstall);
    // queue a second call before p1 finishes
    const p2 = applyEnabledIds(['default::dex/b'], reinstall);
    // The fact that Promise.all resolves at all is the regression check
    // (Bug 3: queued resolvers must settle, not hang). Both head and queued
    // resolvers receive the tail's result by design (see runApply loop).
    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1.ok).toBe(false);
    expect(r2.ok).toBe(false);
    if (!r2.ok) expect(r2.error.kind).toBe('install_failed');
  });

  it('settles queued resolvers when storage.local.set throws inside runApply', async () => {
    const reinstall = vi.fn(
      (_ids: string[]) => new Promise<void>((resolve) => setTimeout(resolve, 5)),
    );
    // Make the SECOND set() call (the tail's ENABLED_KEY write) throw.
    let setCount = 0;
    const originalSet = mocks.browser.storage.local.set;
    mocks.browser.storage.local.set = vi.fn(async (entries: Record<string, unknown>) => {
      setCount += 1;
      if (setCount === 3) throw new Error('storage_failed: quota exceeded');
      return originalSet(entries);
    });

    const p1 = applyEnabledIds(['default::dex/a'], reinstall);
    const p2 = applyEnabledIds(['default::dex/b'], reinstall);
    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1.ok).toBe(false);
    expect(r2.ok).toBe(false);
    if (!r2.ok) expect(r2.error.kind).toBe('storage_failed');

    mocks.browser.storage.local.set = originalSet;
  });
});
