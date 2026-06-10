/** 도메인 연산 — 전부 mutate() 한 번으로 끝나는 thin wrapper. */
import { mutate } from "./store";
import {
  UNCATEGORIZED_PKG,
  type Binding,
  type HoleValue,
  type PackageDef,
  type PolicyDef,
  type StoreSnapshot,
  type WalletPolicyState,
} from "./types";

const newBindingId = () => `bind::${crypto.randomUUID()}`;

function walletAt(draft: StoreSnapshot, address: string): WalletPolicyState {
  const addr = address.toLowerCase();
  return (draft.wallets.byAddress[addr] ??= { bindings: {}, packageEnabled: {} });
}

export function putDef(uid: string, def: PolicyDef): Promise<void> {
  return mutate(uid, (d) => {
    d.library.defs[def.id] = def;
  });
}

/** 정의 삭제 — 모든 지갑의 해당 바인딩도 함께 삭제(cascade). */
export function deleteDef(uid: string, defId: string): Promise<void> {
  return mutate(uid, (d) => {
    delete d.library.defs[defId];
    for (const w of Object.values(d.wallets.byAddress)) {
      for (const [bid, b] of Object.entries(w.bindings)) {
        if (b.defId === defId) delete w.bindings[bid];
      }
    }
  });
}

/** 명시적 분기: 정의를 복제해 독립 정의로. 바인딩은 복사하지 않는다. */
export function duplicateDef(uid: string, defId: string): Promise<string> {
  return mutate(uid, (d) => {
    const src = d.library.defs[defId];
    if (!src) throw new Error(`정의가 없습니다: ${defId}`);
    const newId = `def::${crypto.randomUUID()}`;
    d.library.defs[newId] = {
      ...structuredClone(src),
      id: newId,
      displayName: `${src.displayName} (복제)`,
      source: "mine",
      sourceListingId: undefined,
      sourceVersion: undefined,
      updatedAtMs: Date.now(),
    };
    return newId;
  });
}

export function putPackage(uid: string, pkg: PackageDef): Promise<void> {
  return mutate(uid, (d) => {
    d.library.packages[pkg.id] = pkg;
  });
}

/** 패키지 삭제 — 멤버 바인딩은 미분류로 이동(정책 인스턴스는 살아남는다). */
export function deletePackage(uid: string, pkgId: string): Promise<void> {
  return mutate(uid, (d) => {
    if (pkgId === UNCATEGORIZED_PKG) throw new Error("미분류 패키지는 삭제할 수 없습니다");
    delete d.library.packages[pkgId];
    for (const w of Object.values(d.wallets.byAddress)) {
      for (const b of Object.values(w.bindings)) {
        if (b.packageId === pkgId) b.packageId = UNCATEGORIZED_PKG;
      }
      delete w.packageEnabled[pkgId];
    }
  });
}

export function bind(
  uid: string,
  opts: {
    defId: string;
    packageId: string;
    addresses: string[];
    params?: Record<string, HoleValue>;
    enabled?: boolean;
  },
): Promise<void> {
  return mutate(uid, (d) => {
    for (const address of opts.addresses) {
      const w = walletAt(d, address);
      const b: Binding = {
        id: newBindingId(),
        defId: opts.defId,
        packageId: opts.packageId,
        enabled: opts.enabled ?? true,
        params: opts.params,
        updatedAtMs: Date.now(),
      };
      w.bindings[b.id] = b;
    }
  });
}

export function updateBinding(
  uid: string,
  opts: { address: string; bindingId: string; patch: Partial<Pick<Binding, "enabled" | "params" | "packageId">> },
): Promise<void> {
  return mutate(uid, (d) => {
    const w = d.wallets.byAddress[opts.address.toLowerCase()];
    const b = w?.bindings[opts.bindingId];
    if (!b) throw new Error(`바인딩이 없습니다: ${opts.bindingId}`);
    Object.assign(b, opts.patch, { updatedAtMs: Date.now() });
  });
}

export function removeBinding(uid: string, opts: { address: string; bindingId: string }): Promise<void> {
  return mutate(uid, (d) => {
    const w = d.wallets.byAddress[opts.address.toLowerCase()];
    if (w) delete w.bindings[opts.bindingId];
  });
}

/** 바인딩 인스턴스를 다른 지갑으로 복사(새 id, params/패키지/토글 유지). */
export function copyBindings(
  uid: string,
  opts: { fromAddress: string; toAddress: string; bindingIds: string[] },
): Promise<void> {
  return mutate(uid, (d) => {
    const from = d.wallets.byAddress[opts.fromAddress.toLowerCase()];
    if (!from) throw new Error(`지갑이 없습니다: ${opts.fromAddress}`);
    const to = walletAt(d, opts.toAddress);
    for (const bid of opts.bindingIds) {
      const src = from.bindings[bid];
      if (!src) continue;
      const copy: Binding = { ...structuredClone(src), id: newBindingId(), updatedAtMs: Date.now() };
      to.bindings[copy.id] = copy;
    }
  });
}

export function setPackageEnabled(
  uid: string,
  opts: { address: string; packageId: string; enabled: boolean },
): Promise<void> {
  return mutate(uid, (d) => {
    const w = walletAt(d, opts.address);
    w.packageEnabled[opts.packageId] = opts.enabled;
  });
}

/** 새 지갑 프로비저닝 — defaults.enabled 정의를 기본 파라미터/패키지로 바인딩.
 *  멱등 단위는 지갑: byAddress에 이미 있는 주소는 건드리지 않는다. */
export function provisionWallets(uid: string, addresses: string[]): Promise<void> {
  return mutate(uid, (d) => {
    for (const address of addresses) {
      const addr = address.toLowerCase();
      if (d.wallets.byAddress[addr]) continue;
      const w = walletAt(d, addr);
      for (const def of Object.values(d.library.defs)) {
        if (!def.defaults.enabled) continue;
        const b: Binding = {
          id: newBindingId(),
          defId: def.id,
          packageId: def.defaults.packageId ?? UNCATEGORIZED_PKG,
          enabled: true,
          params: Object.keys(def.defaults.params).length ? { ...def.defaults.params } : undefined,
          updatedAtMs: Date.now(),
        };
        w.bindings[b.id] = b;
      }
    }
  });
}
