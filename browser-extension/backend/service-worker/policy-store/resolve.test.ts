import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    browser: {
      runtime: { getURL: vi.fn((p: string) => `chrome-extension://x/${p}`) },
      storage: {
        local: {
          get: vi.fn(async (key?: string | string[] | null) => {
            if (key == null) return Object.fromEntries(localStore);
            const keys = Array.isArray(key) ? key : [key];
            return Object.fromEntries(keys.filter((k) => localStore.has(k)).map((k) => [k, localStore.get(k)]));
          }),
          set: vi.fn(async (obj: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(obj)) localStore.set(k, v);
          }),
          remove: vi.fn(async (key: string | string[]) => {
            for (const k of Array.isArray(key) ? key : [key]) localStore.delete(k);
          }),
        },
      },
    },
  };
});
vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

vi.mock("./render", () => ({
  renderDef: vi.fn(async (def: { id: string; skeleton: { manifest?: unknown } }, params: unknown) => ({
    text: `P(${def.id})${JSON.stringify(params)}`,
    manifest: def.skeleton.manifest,
  })),
}));
vi.mock("./seed", () => ({ ensureSeeded: vi.fn(async () => undefined) }));

import { bind, putDef, setPackageEnabled, updateBinding } from "./ops";
import {
  collectActionMetas,
  defRefForPolicyId,
  extractTrigger,
  filterForAction,
  isWalletRegistered,
  resolveBundlesForWallet,
  type ResolvedBundle,
} from "./resolve";
import { readStore } from "./store";
import { UNCATEGORIZED_PKG, type PolicyDef } from "./types";

const def = (id: string, manifest?: unknown, enabled = true): PolicyDef => ({
  id,
  displayName: id,
  skeleton: { ir: { kind: "policy" }, manifest },
  holes: [{ name: "cap", type: "long", label: "한도" }],
  defaults: { enabled, params: { cap: 1 } },
  source: "mine",
  updatedAtMs: 1,
});

beforeEach(() => mocks.localStore.clear());

describe("resolveBundlesForWallet", () => {
  it("registered wallet → effective bindings only, with merged params", async () => {
    await putDef("u", def("def::a"));
    await putDef("u", def("def::b"));
    await bind("u", { defId: "def::a", packageId: UNCATEGORIZED_PKG, addresses: ["0xA1"], params: { cap: 9 } });
    await bind("u", { defId: "def::b", packageId: UNCATEGORIZED_PKG, addresses: ["0xa1"] });
    const s = await readStore("u");
    const bb = Object.values(s.wallets.byAddress["0xa1"].bindings).find((b) => b.defId === "def::b")!;
    await updateBinding("u", { address: "0xa1", bindingId: bb.id, patch: { enabled: false } });

    const out = await resolveBundlesForWallet("u", "0xA1");
    expect(out.map((b) => b.id)).toEqual(["def::a"]);
    expect(out[0].policy).toContain('{"cap":9}'); // binding params가 default를 덮음
  });

  it("unregistered wallet → defaults.enabled defs with default params (안전 우선)", async () => {
    await putDef("u", def("def::on"));
    await putDef("u", def("def::off", undefined, false));
    const out = await resolveBundlesForWallet("u", "0xUNKNOWN");
    expect(out.map((b) => b.id)).toEqual(["def::on"]);
    expect(out[0].policy).toContain('{"cap":1}');
  });

  it("package-off binding is excluded", async () => {
    await putDef("u", def("def::a"));
    await bind("u", { defId: "def::a", packageId: UNCATEGORIZED_PKG, addresses: ["0xa1"] });
    await setPackageEnabled("u", { address: "0xa1", packageId: UNCATEGORIZED_PKG, enabled: false });
    expect(await resolveBundlesForWallet("u", "0xa1")).toEqual([]);
  });

  it("isWalletRegistered: true for a wallet with bindings (case-insensitive), false otherwise", async () => {
    await putDef("u", def("def::a"));
    await bind("u", { defId: "def::a", packageId: UNCATEGORIZED_PKG, addresses: ["0xA1"] });
    expect(await isWalletRegistered("u", "0xA1")).toBe(true); // 대소문자 무관(소문자 키)
    expect(await isWalletRegistered("u", "0xUNKNOWN")).toBe(false); // 미등록(=defaults.enabled 폴백)
  });

  it("manifest 없는 def는 빈 ManifestV2를 합성한다 (null이 plan 입력에 섞이면 평가 전체가 죽음)", async () => {
    const noManifest: PolicyDef = {
      ...def("def::base"),
      skeleton: {
        ir: { kind: "policy", annotations: [{ name: "id", value: "swap-cap" }] },
        // manifest 없음 — 보강 필드가 없는 폼 정책의 저장 형태.
      },
    };
    await putDef("u", noManifest);
    await bind("u", { defId: "def::base", packageId: UNCATEGORIZED_PKG, addresses: ["0xa1"] });

    const out = await resolveBundlesForWallet("u", "0xa1");
    expect(out).toHaveLength(1);
    expect(out[0].manifest).toEqual({ id: "swap-cap", schema_version: 2 });
    expect(out[0].trigger).toBeUndefined(); // 빈 trigger → 항상 평가(엔진이 정밀 게이트)
  });

  it("a def that fails to render is skipped, others survive", async () => {
    const { renderDef } = await import("./render");
    vi.mocked(renderDef).mockImplementationOnce(async () => {
      throw new Error("broken skeleton");
    });
    await putDef("u", def("def::bad"));
    await putDef("u", def("def::good"));
    const out = await resolveBundlesForWallet("u", "0xa1");
    expect(out).toHaveLength(1);
  });
});

describe("trigger pre-filter", () => {
  it("extractTrigger reads eq/in and ignores ne/tx.* constraints", () => {
    expect(
      extractTrigger({
        trigger: {
          where: {
            "action.tag": { eq: "swap" },
            "action.venue": { in: ["uniswap_v3", "aave_v3"] },
            "action.domain": { ne: "perp" },
            "tx.chain_id": { eq: "eip155:1" },
          },
        },
      }),
    ).toEqual({ tags: ["swap"], venues: ["uniswap_v3", "aave_v3"] });
    expect(extractTrigger({ trigger: { where: {} } })).toBeUndefined();
    expect(extractTrigger(undefined)).toBeUndefined();
  });

  it("collectActionMetas unions multicall leaves with the parent", () => {
    const metas = collectActionMetas({
      domain: "multicall",
      actions: [
        { domain: "token", action: "erc20_approve" },
        { domain: "amm", action: "swap", venue: { name: "uniswap_v3" } },
      ],
    });
    expect(metas).toEqual([
      { domain: "multicall", tag: null, venue: null },
      { domain: "token", tag: "erc20_approve", venue: null },
      { domain: "amm", tag: "swap", venue: "uniswap_v3" },
    ]);
  });

  it("filterForAction drops only definite misses; unknown dims pass", () => {
    const bundles: ResolvedBundle[] = [
      { id: "approve-only", policy: "p", manifest: undefined, trigger: { tags: ["erc20_approve"] } },
      { id: "swap-only", policy: "p", manifest: undefined, trigger: { tags: ["swap"] } },
      { id: "untriggered", policy: "p", manifest: undefined, trigger: undefined },
    ];
    // 단일 send 액션: approve/swap 번들은 확실 미스 → 드롭
    expect(filterForAction(bundles, [{ domain: "token", tag: "send" }]).map((b) => b.id)).toEqual(["untriggered"]);
    // multicall에 approve leaf 포함: approve 번들 유지
    const metas = collectActionMetas({
      domain: "multicall",
      actions: [{ domain: "token", action: "erc20_approve" }],
    });
    expect(filterForAction(bundles, metas).map((b) => b.id)).toEqual(["approve-only", "untriggered"]);
    // 메타를 읽지 못한 액션(tag unknown): 아무것도 드롭하지 않음
    expect(filterForAction(bundles, [{}])).toHaveLength(3);
  });
});

describe("defRefForPolicyId", () => {
  it("matches IR @id annotation first, then def id, else null", async () => {
    await putDef("u", {
      ...def("def::1"),
      displayName: "한도",
      skeleton: { ir: { kind: "policy", annotations: [{ name: "id", value: "swap-cap" }] } },
    });
    await putDef("u", def("def::2"));
    expect(await defRefForPolicyId("u", "swap-cap")).toEqual({ defId: "def::1", displayName: "한도" });
    expect(await defRefForPolicyId("u", "def::2")).toEqual({ defId: "def::2", displayName: "def::2" });
    expect(await defRefForPolicyId("u", "nope")).toBeNull();
  });
});

describe("stale param guard", () => {
  it("binding params for holes that no longer exist are dropped before render", async () => {
    await putDef("u", { ...def("def::a"), holes: [{ name: "cap", type: "long", label: "한도" }] });
    await bind("u", {
      defId: "def::a",
      packageId: UNCATEGORIZED_PKG,
      addresses: ["0xa1"],
      params: { cap: 9, ghost: 1 },
    });
    const out = await resolveBundlesForWallet("u", "0xa1");
    expect(out[0].policy).toContain('"cap":9');
    expect(out[0].policy).not.toContain("ghost");
  });
});

describe("required hole 미충전 def 방어 스킵", () => {
  const holedDef = (id: string, enabled = true): PolicyDef => ({
    ...def(id, undefined, enabled),
    holes: [{ name: "v1", type: "address", label: "받는 주소", required: true }],
    defaults: { enabled, params: {} },
  });

  it("바인딩이 있어도 required 미충전이면 평가에서 뺀다 (가드 이전 상태 방어)", async () => {
    // bind 가드를 우회해 미충전 바인딩을 만들기 위해 def를 나중에 교체한다.
    await putDef("u", def("def::m"));
    await bind("u", { defId: "def::m", packageId: UNCATEGORIZED_PKG, addresses: ["0xa1"] });
    await putDef("u", holedDef("def::m"));
    const out = await resolveBundlesForWallet("u", "0xa1");
    expect(out).toEqual([]);
  });

  it("미등록 지갑 defaults 경로도 동일하게 뺀다", async () => {
    await putDef("u", holedDef("def::m"));
    await putDef("u", def("def::ok"));
    const out = await resolveBundlesForWallet("u", "0xUNKNOWN");
    expect(out.map((b) => b.id)).toEqual(["def::ok"]);
  });

  it("바인딩 params가 required를 덮으면 평가에 들어간다", async () => {
    await putDef("u", holedDef("def::m"));
    await bind("u", {
      defId: "def::m",
      packageId: UNCATEGORIZED_PKG,
      addresses: ["0xa1"],
      params: { v1: "0xabc4000000000000000000000000000000007e29" },
    });
    const out = await resolveBundlesForWallet("u", "0xa1");
    expect(out.map((b) => b.id)).toEqual(["def::m"]);
  });
});
