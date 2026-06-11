import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    browser: {
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

import {
  bind,
  copyBindings,
  deleteDef,
  deletePackage,
  duplicateDef,
  installMarket,
  provisionWallets,
  putDef,
  putPackage,
  removeBinding,
  putWalletPackage,
  removePackageFromWallet,
  setPackageEnabled,
  updateBinding,
} from "./ops";
import { readStore } from "./store";
import { isEffectiveOn, UNCATEGORIZED_PKG, type PolicyDef } from "./types";

const def = (id: string): PolicyDef => ({
  id,
  displayName: id,
  skeleton: { ir: { kind: "policy" } },
  holes: [],
  defaults: { enabled: true, params: {} },
  source: "mine",
  updatedAtMs: 1,
});

beforeEach(() => mocks.localStore.clear());

describe("policy-store ops", () => {
  it("deleteDef cascades bindings on every wallet", async () => {
    await putDef("u", def("def::a"));
    await bind("u", { defId: "def::a", packageId: UNCATEGORIZED_PKG, addresses: ["0xA1", "0xa2"] });
    await deleteDef("u", "def::a");
    const s = await readStore("u");
    expect(Object.values(s.wallets.byAddress).flatMap((w) => Object.keys(w.bindings))).toEqual([]);
    expect(s.library.defs["def::a"]).toBeUndefined();
  });

  it("deletePackage(라이브러리 폴더 삭제)는 지갑 패키지/바인딩을 건드리지 않는다", async () => {
    await putDef("u", def("def::a"));
    await putPackage("u", { id: "pkg::x", displayName: "X", source: "mine", updatedAtMs: 1 });
    // bind가 같은 id의 지갑 패키지를 실체화한다 — 이후 폴더가 사라져도 지갑은 그대로.
    await bind("u", { defId: "def::a", packageId: "pkg::x", addresses: ["0xa1"] });
    await deletePackage("u", "pkg::x");
    const s = await readStore("u");
    const w = s.wallets.byAddress["0xa1"];
    const b = Object.values(w.bindings)[0];
    expect(b.packageId).toBe("pkg::x");
    expect(w.packages["pkg::x"]).toEqual(expect.objectContaining({ displayName: "X" }));
    expect(s.library.packages["pkg::x"]).toBeUndefined();
  });

  it("deleting 미분류 package throws", async () => {
    await expect(deletePackage("u", UNCATEGORIZED_PKG)).rejects.toThrow();
  });

  it("same def in different packages = independent instances", async () => {
    await putDef("u", def("def::a"));
    await putPackage("u", { id: "pkg::x", displayName: "X", source: "mine", updatedAtMs: 1 });
    await bind("u", { defId: "def::a", packageId: "pkg::x", addresses: ["0xa1"] });
    await bind("u", { defId: "def::a", packageId: UNCATEGORIZED_PKG, addresses: ["0xa1"] });
    const s = await readStore("u");
    expect(Object.keys(s.wallets.byAddress["0xa1"].bindings)).toHaveLength(2);
  });

  it("package toggle composes with binding toggle and restores partial state", async () => {
    await putDef("u", def("def::a"));
    await putDef("u", def("def::b"));
    await putPackage("u", { id: "pkg::x", displayName: "X", source: "mine", updatedAtMs: 1 });
    await bind("u", { defId: "def::a", packageId: "pkg::x", addresses: ["0xa1"] });
    await bind("u", { defId: "def::b", packageId: "pkg::x", addresses: ["0xa1"] });
    const s1 = await readStore("u");
    const all = Object.values(s1.wallets.byAddress["0xa1"].bindings);
    const ba = all.find((b) => b.defId === "def::a")!;
    const bb = all.find((b) => b.defId === "def::b")!;
    await updateBinding("u", { address: "0xa1", bindingId: bb.id, patch: { enabled: false } });
    await setPackageEnabled("u", { address: "0xa1", packageId: "pkg::x", enabled: false });
    let s = await readStore("u");
    let w = s.wallets.byAddress["0xa1"];
    expect(Object.values(w.bindings).some((b) => isEffectiveOn(w, b))).toBe(false);
    await setPackageEnabled("u", { address: "0xa1", packageId: "pkg::x", enabled: true });
    s = await readStore("u");
    w = s.wallets.byAddress["0xa1"];
    expect(isEffectiveOn(w, w.bindings[ba.id])).toBe(true); // 복원
    expect(isEffectiveOn(w, w.bindings[bb.id])).toBe(false); // 부분 끔 유지
  });

  it("provisionWallets auto-binds defaults.enabled defs once (idempotent, lowercases)", async () => {
    await putDef("u", def("def::a"));
    await putDef("u", { ...def("def::off"), defaults: { enabled: false, params: {} } });
    await provisionWallets("u", ["0xAbC"]);
    await provisionWallets("u", ["0xabc"]);
    const s = await readStore("u");
    expect(Object.keys(s.wallets.byAddress)).toEqual(["0xabc"]);
    const w = s.wallets.byAddress["0xabc"];
    expect(Object.values(w.bindings).map((b) => b.defId)).toEqual(["def::a"]);
  });

  it("provisioning respects defaults.packageId", async () => {
    await putPackage("u", { id: "pkg::safe", displayName: "안전팩", source: "builtin", updatedAtMs: 1 });
    await putDef("u", { ...def("def::a"), defaults: { enabled: true, params: {}, packageId: "pkg::safe" } });
    await provisionWallets("u", ["0xa1"]);
    const s = await readStore("u");
    expect(Object.values(s.wallets.byAddress["0xa1"].bindings)[0].packageId).toBe("pkg::safe");
  });

  it("duplicateDef makes an independent definition", async () => {
    await putDef("u", def("def::a"));
    const newId = await duplicateDef("u", "def::a");
    const s = await readStore("u");
    expect(newId).not.toBe("def::a");
    expect(s.library.defs[newId].displayName).toContain("복제");
    expect(s.library.defs[newId].source).toBe("mine");
  });

  it("copyBindings copies instances to another wallet (params preserved)", async () => {
    await putDef("u", def("def::a"));
    await bind("u", { defId: "def::a", packageId: UNCATEGORIZED_PKG, addresses: ["0xa1"], params: { x: 1 } });
    const src = await readStore("u");
    const ids = Object.keys(src.wallets.byAddress["0xa1"].bindings);
    await copyBindings("u", { fromAddress: "0xa1", toAddress: "0xA2", bindingIds: ids });
    const s = await readStore("u");
    const copied = Object.values(s.wallets.byAddress["0xa2"].bindings)[0];
    expect(copied.params).toEqual({ x: 1 });
    expect(copied.id).not.toBe(ids[0]); // 새 인스턴스
  });

  it("removeBinding deletes just that instance", async () => {
    await putDef("u", def("def::a"));
    await bind("u", { defId: "def::a", packageId: UNCATEGORIZED_PKG, addresses: ["0xa1"] });
    const s1 = await readStore("u");
    const id = Object.keys(s1.wallets.byAddress["0xa1"].bindings)[0];
    await removeBinding("u", { address: "0xa1", bindingId: id });
    const s = await readStore("u");
    expect(Object.keys(s.wallets.byAddress["0xa1"].bindings)).toEqual([]);
  });
});

describe("패키지 삭제/지갑 차원 제거의 분리", () => {
  it("deletePackage: def의 라이브러리 폴더 소속도 미분류로 돌린다", async () => {
    await putPackage("u", { id: "pkg::x", displayName: "X", source: "mine", updatedAtMs: 1 });
    await putDef("u", {
      ...def("def::a"),
      defaults: { enabled: true, params: {}, packageId: "pkg::x" },
    });
    await deletePackage("u", "pkg::x");
    const snap = await readStore("u");
    expect(snap.library.defs["def::a"].defaults.packageId).toBeUndefined();
  });

  it("removePackageFromWallet: 이 지갑의 바인딩+게이트만 제거, 계정 패키지/def 불변", async () => {
    await putPackage("u", { id: "pkg::x", displayName: "X", source: "mine", updatedAtMs: 1 });
    await putDef("u", def("def::a"));
    await bind("u", { defId: "def::a", packageId: "pkg::x", addresses: ["0xa1", "0xb2"] });
    await removePackageFromWallet("u", { address: "0xA1", packageId: "pkg::x" });
    const snap = await readStore("u");
    expect(Object.keys(snap.wallets.byAddress["0xa1"].bindings)).toHaveLength(0);
    expect(snap.wallets.byAddress["0xa1"].packageEnabled["pkg::x"]).toBeUndefined();
    // 다른 지갑과 계정 차원 객체는 그대로
    expect(Object.keys(snap.wallets.byAddress["0xb2"].bindings)).toHaveLength(1);
    expect(snap.library.packages["pkg::x"]).toBeDefined();
    expect(snap.library.defs["def::a"]).toBeDefined();
  });
});

describe("지갑 패키지 분리", () => {
  it("putWalletPackage는 지갑 안에서만 — 라이브러리 packages 불변", async () => {
    await putWalletPackage("u", { address: "0xA1", pkg: { id: "pkg::w1", displayName: "콜드 전용" } });
    const s = await readStore("u");
    expect(s.wallets.byAddress["0xa1"].packages["pkg::w1"].displayName).toBe("콜드 전용");
    expect(s.library.packages["pkg::w1"]).toBeUndefined();
  });

  it("bind는 라이브러리 폴더 id를 받으면 같은 이름의 지갑 패키지를 실체화한다", async () => {
    await putDef("u", def("def::a"));
    await putPackage("u", { id: "pkg::x", displayName: "안전팩", source: "mine", updatedAtMs: 1 });
    await bind("u", { defId: "def::a", packageId: "pkg::x", addresses: ["0xa1"] });
    const s = await readStore("u");
    expect(s.wallets.byAddress["0xa1"].packages["pkg::x"].displayName).toBe("안전팩");
  });
});

describe("지갑 전용 정책 (hidden def)", () => {
  it("마지막 바인딩 제거 시 def도 함께 정리된다", async () => {
    await putDef("u", { ...def("def::w"), hidden: true });
    await bind("u", { defId: "def::w", packageId: UNCATEGORIZED_PKG, addresses: ["0xa1"] });
    const bid = Object.keys((await readStore("u")).wallets.byAddress["0xa1"].bindings)[0];
    await removeBinding("u", { address: "0xa1", bindingId: bid });
    const s = await readStore("u");
    expect(s.library.defs["def::w"]).toBeUndefined();
  });

  it("지갑 패키지 제거로 바인딩이 사라져도 cascade", async () => {
    await putDef("u", { ...def("def::w"), hidden: true });
    await putWalletPackage("u", { address: "0xa1", pkg: { id: "pkg::wp", displayName: "P" } });
    await bind("u", { defId: "def::w", packageId: "pkg::wp", addresses: ["0xa1"] });
    await removePackageFromWallet("u", { address: "0xa1", packageId: "pkg::wp" });
    const s = await readStore("u");
    expect(s.library.defs["def::w"]).toBeUndefined();
  });

  it("다른 지갑에 바인딩이 남아 있으면 정리하지 않는다", async () => {
    await putDef("u", { ...def("def::w"), hidden: true });
    await bind("u", { defId: "def::w", packageId: UNCATEGORIZED_PKG, addresses: ["0xa1", "0xb2"] });
    const bid = Object.keys((await readStore("u")).wallets.byAddress["0xa1"].bindings)[0];
    await removeBinding("u", { address: "0xa1", bindingId: bid });
    const s = await readStore("u");
    expect(s.library.defs["def::w"]).toBeDefined();
  });
});

describe("required hole guard (마켓 비식별 빈칸)", () => {
  const holed = (id: string): PolicyDef => ({
    ...def(id),
    holes: [
      { name: "v1", type: "address", label: "받는 주소", required: true },
      { name: "v2", type: "long", label: "한도" },
    ],
    defaults: { enabled: true, params: { v2: 150 } }, // v1은 미충전
  });

  it("bind: required hole이 안 채워진 def는 거부한다", async () => {
    await putDef("u", holed("def::m"));
    await expect(
      bind("u", { defId: "def::m", packageId: UNCATEGORIZED_PKG, addresses: ["0xa1"] }),
    ).rejects.toThrow(/받는 주소/);
    const s = await readStore("u");
    expect(Object.keys(s.wallets.byAddress["0xa1"]?.bindings ?? {})).toHaveLength(0);
  });

  it("bind: 바인딩 params가 required를 덮으면 통과한다", async () => {
    await putDef("u", holed("def::m"));
    await bind("u", {
      defId: "def::m",
      packageId: UNCATEGORIZED_PKG,
      addresses: ["0xa1"],
      params: { v1: "0xabc4000000000000000000000000000000007e29" },
    });
    const s = await readStore("u");
    expect(Object.keys(s.wallets.byAddress["0xa1"].bindings)).toHaveLength(1);
  });

  it("installMarket: 바인딩이 생기는 scope에서 미충전이면 전체 거부(원자적)", async () => {
    await provisionWallets("u", ["0xa1"]);
    await expect(
      installMarket("u", { defs: [holed("def::m")], scope: { kind: "all" } }),
    ).rejects.toThrow(/빈칸/);
    // mutate가 draft에서 실패 → 라이브러리 등록까지 함께 롤백된다.
    const s = await readStore("u");
    expect(s.library.defs["def::m"]).toBeUndefined();
  });

  it("installMarket: library-only는 미충전이어도 들어간다 (바인딩이 없으니 안전)", async () => {
    await installMarket("u", { defs: [holed("def::m")], scope: { kind: "library-only" } });
    const s = await readStore("u");
    expect(s.library.defs["def::m"]).toBeDefined();
  });

  it("installMarket: opts.params 또는 defaults.params가 덮으면 통과한다", async () => {
    await provisionWallets("u", ["0xa1"]);
    await installMarket("u", {
      defs: [holed("def::m")],
      scope: { kind: "all" },
      params: { "def::m": { v1: "0xabc4000000000000000000000000000000007e29" } },
    });
    const s = await readStore("u");
    const b = Object.values(s.wallets.byAddress["0xa1"].bindings)[0];
    expect(b.params).toEqual({ v1: "0xabc4000000000000000000000000000000007e29" });
  });

  it("provisionWallets: 미충전 def는 새 지갑에 적용하지 않고 건너뛴다", async () => {
    await putDef("u", def("def::ok"));
    await putDef("u", holed("def::m"));
    await provisionWallets("u", ["0xNEW"]);
    const s = await readStore("u");
    const bound = Object.values(s.wallets.byAddress["0xnew"].bindings).map((b) => b.defId);
    expect(bound).toEqual(["def::ok"]);
  });

  it("updateBinding: params 패치로 required를 다시 비우는 것을 거부한다", async () => {
    await putDef("u", holed("def::m"));
    await bind("u", {
      defId: "def::m",
      packageId: UNCATEGORIZED_PKG,
      addresses: ["0xa1"],
      params: { v1: "0xabc4000000000000000000000000000000007e29" },
    });
    const s = await readStore("u");
    const b = Object.values(s.wallets.byAddress["0xa1"].bindings)[0];
    await expect(
      updateBinding("u", { address: "0xa1", bindingId: b.id, patch: { params: {} } }),
    ).rejects.toThrow(/받는 주소/);
  });
});
