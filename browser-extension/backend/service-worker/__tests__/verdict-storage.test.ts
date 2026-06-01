import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  localStore: new Map<string, unknown>(),
  browser: {
    storage: {
      local: {
        get: vi.fn(async (key: string) => ({ [key]: mocks.localStore.get(key) })),
        set: vi.fn(async (entries: Record<string, unknown>) => {
          for (const [key, value] of Object.entries(entries)) {
            mocks.localStore.set(key, value);
          }
        }),
        remove: vi.fn(async (key: string) => {
          mocks.localStore.delete(key);
        }),
      },
    },
  },
}));

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

import {
  appendVerdict,
  countVerdicts,
  exportVerdictsAsCsv,
  listVerdicts,
  setVerdictDecision,
  type VerdictInsert,
} from "../verdict-storage";

function insert(overrides: Partial<VerdictInsert> = {}): VerdictInsert {
  return {
    ts: 1_700_000_000,
    wallet: "0x362e7e9e630481631d7c804dfe50e24b53250925",
    verdict: "warn",
    severity: "warn",
    method: "eth_sendTransaction",
    decoded_fn: null,
    dapp_origin: "app.example",
    policy: { id: null, name: "policy::warn", severity: "warn" },
    reason: { ko: null, en: "test reason" },
    delta_id: null,
    ...overrides,
  };
}

describe("verdict-storage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
  });

  it("stores newest-first rows and filters by wallet/verdict/search", async () => {
    await appendVerdict(insert({ verdict: "pass", severity: "info" }));
    await appendVerdict(insert({ wallet: "0xabc", verdict: "fail", severity: "deny" }));

    const rows = await listVerdicts({
      wallet: "0xabc",
      verdict: "fail",
      search: "policy::warn",
    });

    expect(rows).toHaveLength(1);
    expect(rows[0]?.wallet).toBe("0xabc");
    expect(rows[0]?.verdict).toBe("fail");
    expect(rows[0]?.id).toEqual(expect.any(String));
  });

  it("counts, updates decisions, and exports csv", async () => {
    const row = await appendVerdict(insert());

    await expect(countVerdicts()).resolves.toEqual({ pass: 0, warn: 1, fail: 0 });
    await expect(setVerdictDecision(row.id, "trusted")).resolves.toBe(true);
    await expect(setVerdictDecision("missing", "cancelled")).resolves.toBe(false);

    const rows = await listVerdicts();
    expect(rows[0]?.user_decision).toBe("trusted");
    expect(rows[0]?.decided_at).toEqual(expect.any(Number));
    await expect(exportVerdictsAsCsv()).resolves.toContain("policy::warn");
  });
});
