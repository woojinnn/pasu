/** 도메인 연산 — 전부 mutate() 한 번으로 끝나는 thin wrapper. */
import { mutate } from "./store";
import {
  UNCATEGORIZED_PKG,
  missingRequiredHoles,
  type Binding,
  type HoleValue,
  type PackageDef,
  type PolicyDef,
  type StoreSnapshot,
  type WalletPolicyState,
} from "./types";

const newBindingId = () => `bind::${crypto.randomUUID()}`;

/** 마켓 게시 때 블랭킹된 required hole이 안 채워진 def는 지갑에 바인딩
 *  (패키지 적용)할 수 없다 — 플레이스홀더(제로주소/0)로 평가되면 조용히
 *  무용지물이거나(양성 비교) 모든 거래에 오발화한다(부정 비교). */
function assertHolesFilled(
  def: PolicyDef | undefined,
  params: Record<string, HoleValue> | undefined,
): void {
  if (!def) return;
  const missing = missingRequiredHoles(def, params);
  if (missing.length > 0) {
    throw new Error(
      `정책 "${def.displayName}"의 빈칸(${missing.join(", ")})을 채워야 적용할 수 있어요`,
    );
  }
}

function walletAt(draft: StoreSnapshot, address: string): WalletPolicyState {
  const addr = address.toLowerCase();
  return (draft.wallets.byAddress[addr] ??= { bindings: {}, packages: {}, packageEnabled: {} });
}

/** 지갑 패키지 실체화: 라이브러리 폴더 id로 바인딩이 들어오면(마켓/시드/범위
 *  프로비저닝) 같은 id·이름의 지갑 패키지를 만들어 소속시킨다. */
function ensureWalletPackage(d: StoreSnapshot, w: WalletPolicyState, packageId: string): void {
  if (packageId === UNCATEGORIZED_PKG || w.packages[packageId]) return;
  w.packages[packageId] = {
    id: packageId,
    displayName: d.library.packages[packageId]?.displayName ?? packageId,
    updatedAtMs: Date.now(),
  };
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
    // 라이브러리 폴더 소속도 미분류로 — 죽은 패키지를 가리키는 def는 디렉토리
    // 뷰에서 통째로 사라져 보인다. 지갑 패키지는 별개 객체라 건드리지 않는다.
    for (const def of Object.values(d.library.defs)) {
      if (def.defaults.packageId === pkgId) delete def.defaults.packageId;
    }
  });
}

/** 지갑 패키지 생성/이름변경 — 지갑 안에서만 존재, 라이브러리 불변. */
export function putWalletPackage(
  uid: string,
  opts: { address: string; pkg: { id: string; displayName: string } },
): Promise<void> {
  return mutate(uid, (d) => {
    if (opts.pkg.id === UNCATEGORIZED_PKG) throw new Error("미분류는 이름을 바꿀 수 없습니다");
    const w = walletAt(d, opts.address);
    w.packages[opts.pkg.id] = { ...opts.pkg, updatedAtMs: Date.now() };
  });
}

/** 지갑 차원 제거: 이 지갑에서 패키지의 바인딩들과 게이트만 걷어낸다 —
 *  계정 패키지 객체와 라이브러리는 건드리지 않는다(지갑 페이지의 휴지통 의미). */
export function removePackageFromWallet(
  uid: string,
  opts: { address: string; packageId: string },
): Promise<void> {
  return mutate(uid, (d) => {
    const w = d.wallets.byAddress[opts.address.toLowerCase()];
    if (!w) return;
    for (const b of Object.values(w.bindings)) {
      if (b.packageId !== opts.packageId) continue;
      // 지갑 전용 정책은 패키지가 사라져도 이 지갑의 미분류로 살아남는다.
      if (isLastBindingOfHiddenDef(d, b)) {
        b.packageId = UNCATEGORIZED_PKG;
        b.updatedAtMs = Date.now();
      } else {
        delete w.bindings[b.id];
      }
    }
    delete w.packages[opts.packageId];
    delete w.packageEnabled[opts.packageId];
    pruneHiddenDefs(d);
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
    alias?: string;
  },
): Promise<void> {
  return mutate(uid, (d) => {
    assertHolesFilled(d.library.defs[opts.defId], opts.params);
    for (const address of opts.addresses) {
      const w = walletAt(d, address);
      ensureWalletPackage(d, w, opts.packageId);
      const b: Binding = {
        id: newBindingId(),
        defId: opts.defId,
        packageId: opts.packageId,
        enabled: opts.enabled ?? true,
        alias: opts.alias,
        params: opts.params,
        updatedAtMs: Date.now(),
      };
      w.bindings[b.id] = b;
    }
  });
}

export function updateBinding(
  uid: string,
  opts: { address: string; bindingId: string; patch: Partial<Pick<Binding, "enabled" | "params" | "packageId" | "alias">> },
): Promise<void> {
  return mutate(uid, (d) => {
    const w = d.wallets.byAddress[opts.address.toLowerCase()];
    const b = w?.bindings[opts.bindingId];
    if (!b) throw new Error(`바인딩이 없습니다: ${opts.bindingId}`);
    // params를 갈아끼우는 패치는 required hole을 다시 비울 수 있다.
    if ("params" in opts.patch) {
      assertHolesFilled(d.library.defs[b.defId], opts.patch.params);
    }
    Object.assign(b, opts.patch, { updatedAtMs: Date.now() });
  });
}

/** 이 바인딩이 지갑 전용(hidden) def의 마지막 바인딩인가 — 빼면 def가 갈 곳을
 *  잃는다. */
function isLastBindingOfHiddenDef(d: StoreSnapshot, binding: Binding): boolean {
  const def = d.library.defs[binding.defId];
  if (def?.hidden !== true) return false;
  let count = 0;
  for (const w of Object.values(d.wallets.byAddress)) {
    for (const b of Object.values(w.bindings)) {
      if (b.defId === binding.defId) count += 1;
    }
  }
  return count <= 1;
}

/** 지갑 전용(hidden) def가 어쩌다 바인딩을 전부 잃으면(지갑 삭제 등) 라이브러리
 *  (미분류)로 승격한다 — 트리에 남아 재적용/명시 삭제가 가능하다. 일반 제거
 *  경로는 removeBinding/removePackageFromWallet이 미분류 이동으로 먼저 막는다.
 *  소리 없는 삭제는 어느 경로에도 없다. */
function pruneHiddenDefs(d: StoreSnapshot): void {
  const bound = new Set<string>();
  for (const w of Object.values(d.wallets.byAddress)) {
    for (const b of Object.values(w.bindings)) bound.add(b.defId);
  }
  for (const def of Object.values(d.library.defs)) {
    if (def.hidden && !bound.has(def.id)) {
      def.hidden = false;
      def.updatedAtMs = Date.now();
    }
  }
}

export function removeBinding(uid: string, opts: { address: string; bindingId: string }): Promise<void> {
  return mutate(uid, (d) => {
    const w = d.wallets.byAddress[opts.address.toLowerCase()];
    if (!w) return;
    const b = w.bindings[opts.bindingId];
    if (!b) return;
    // 지갑 전용 정책은 패키지에서 빼도 지갑을 떠나지 않는다 — 같은 지갑의
    // 미분류로 이동(params/별칭/토글 보존). 미분류에서 또 빼면 그때 지갑을
    // 떠나고, pruneHiddenDefs가 라이브러리로 승격해 트리에 남긴다.
    if (b.packageId !== UNCATEGORIZED_PKG && isLastBindingOfHiddenDef(d, b)) {
      b.packageId = UNCATEGORIZED_PKG;
      b.updatedAtMs = Date.now();
      return;
    }
    delete w.bindings[opts.bindingId];
    pruneHiddenDefs(d);
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
      if (src.packageId !== UNCATEGORIZED_PKG && !to.packages[src.packageId]) {
        to.packages[src.packageId] = {
          ...(from.packages[src.packageId] ?? {
            id: src.packageId,
            displayName: src.packageId,
          }),
          updatedAtMs: Date.now(),
        };
      }
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

export type MarketInstallScope =
  | { kind: "wallets"; addresses: string[] }
  | { kind: "all" }
  | { kind: "library-only" };

/** 마켓 설치/업데이트 — 정의(+패키지) 등록 후 scope에 따라 바인딩.
 *  같은 id의 재설치는 정의 업데이트: 기존 바인딩의 params는 보존하되,
 *  새 정의의 holes에서 사라진 키는 제거한다(렌더 실패 방지). */
export function installMarket(
  uid: string,
  opts: {
    defs: PolicyDef[];
    pkg?: PackageDef | undefined;
    scope: MarketInstallScope;
    /** defId별 설치 시점 hole 파라미터(설치 모달 입력). */
    params?: Record<string, Record<string, HoleValue>> | undefined;
  },
): Promise<void> {
  return mutate(uid, (d) => {
    if (opts.pkg) d.library.packages[opts.pkg.id] = { ...opts.pkg, source: "market" };
    const defaultPkg = opts.pkg?.id ?? UNCATEGORIZED_PKG;

    for (const def of opts.defs) {
      const isUpdate = !!d.library.defs[def.id];
      d.library.defs[def.id] = { ...def, source: "market" };
      if (isUpdate) {
        // 업데이트: 모든 지갑의 해당 바인딩에서 사라진 hole 키를 정리.
        const live = new Set(def.holes.map((h) => h.name));
        for (const w of Object.values(d.wallets.byAddress)) {
          for (const b of Object.values(w.bindings)) {
            if (b.defId !== def.id || !b.params) continue;
            b.params = Object.fromEntries(Object.entries(b.params).filter(([k]) => live.has(k)));
            if (Object.keys(b.params).length === 0) b.params = undefined;
          }
        }
      }
    }

    if (opts.scope.kind === "library-only") return;
    // 바인딩이 생기는 설치는 required hole(비식별 블랭킹 칸)이 채워져 있어야
    // 한다 — 채움값은 defaults.params(클라이언트가 병합) 또는 opts.params.
    for (const def of opts.defs) assertHolesFilled(def, opts.params?.[def.id]);
    const addresses =
      opts.scope.kind === "all" ? Object.keys(d.wallets.byAddress) : opts.scope.addresses.map((a) => a.toLowerCase());
    for (const address of addresses) {
      const w = walletAt(d, address);
      for (const def of opts.defs) {
        const params = opts.params?.[def.id];
        const b: Binding = {
          id: newBindingId(),
          defId: def.id,
          packageId: def.defaults.packageId ?? defaultPkg,
          enabled: true,
          params: params && Object.keys(params).length ? { ...params } : undefined,
          updatedAtMs: Date.now(),
        };
        ensureWalletPackage(d, w, b.packageId);
        w.bindings[b.id] = b;
      }
    }
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
        // 자동 경로라 throw 대신 스킵 — 빈칸이 남은 def는 새 지갑에 적용하지
        // 않는다(채우면 그때 수동 적용).
        if (missingRequiredHoles(def).length > 0) continue;
        const b: Binding = {
          id: newBindingId(),
          defId: def.id,
          packageId: def.defaults.packageId ?? UNCATEGORIZED_PKG,
          enabled: true,
          params: Object.keys(def.defaults.params).length ? { ...def.defaults.params } : undefined,
          updatedAtMs: Date.now(),
        };
        ensureWalletPackage(d, w, b.packageId);
        w.bindings[b.id] = b;
      }
    }
  });
}
