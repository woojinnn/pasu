/** ps2:* SW 메시지 패밀리 — 메시지 모양을 ops/스토어 호출로 위임하는 thin layer.
 *  uid 해석과 시드 보장은 여기서 한 번만 처리한다. */
import { getCurrentUserId } from "../dashboard/current-user";
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
  putWalletPackage,
  removeBinding,
  removePackageFromWallet,
  setPackageEnabled,
  updateBinding,
  type MarketInstallScope,
} from "./ops";
import { ensureSeeded } from "./seed";
import { readStore } from "./store";
import type { Binding, HoleValue, PackageDef, PolicyDef } from "./types";

export type Ps2Request =
  | { type: "ps2:get-library" }
  | { type: "ps2:get-wallet-state"; address: string }
  | { type: "ps2:get-overview" }
  | { type: "ps2:put-def"; def: PolicyDef }
  | { type: "ps2:delete-def"; defId: string }
  | { type: "ps2:duplicate-def"; defId: string }
  | { type: "ps2:put-package"; pkg: PackageDef }
  | { type: "ps2:delete-package"; packageId: string }
  | {
      type: "ps2:bind";
      defId: string;
      packageId: string;
      addresses: string[];
      params?: Record<string, HoleValue>;
      enabled?: boolean;
      alias?: string;
    }
  | {
      type: "ps2:update-binding";
      address: string;
      bindingId: string;
      patch: Partial<Pick<Binding, "enabled" | "params" | "packageId" | "alias">>;
    }
  | { type: "ps2:remove-binding"; address: string; bindingId: string }
  | { type: "ps2:remove-wallet-package"; address: string; packageId: string }
  | { type: "ps2:put-wallet-package"; address: string; pkg: { id: string; displayName: string } }
  | { type: "ps2:copy-bindings"; fromAddress: string; toAddress: string; bindingIds: string[] }
  | { type: "ps2:set-package-enabled"; address: string; packageId: string; enabled: boolean }
  | { type: "ps2:provision-wallets"; addresses: string[] }
  | {
      type: "ps2:install-market";
      defs: PolicyDef[];
      pkg?: PackageDef;
      scope: MarketInstallScope;
      params?: Record<string, Record<string, HoleValue>>;
    };

export function isPs2Request(req: { type?: unknown }): req is Ps2Request {
  return typeof req.type === "string" && req.type.startsWith("ps2:");
}

async function uidOrAnonymous(): Promise<string> {
  return (await getCurrentUserId()) ?? "anonymous";
}

/** 메시지 한 건 처리. 응답 봉투({ok, data|error})는 index.ts 쪽 공통 패턴이 입힌다. */
export async function handlePs2Request(req: Ps2Request): Promise<unknown> {
  const uid = await uidOrAnonymous();
  await ensureSeeded(uid);

  switch (req.type) {
    case "ps2:get-library": {
      const s = await readStore(uid);
      return { library: s.library, rev: s.rev };
    }
    case "ps2:get-wallet-state": {
      const s = await readStore(uid);
      return s.wallets.byAddress[req.address.toLowerCase()] ?? { bindings: {}, packages: {}, packageEnabled: {} };
    }
    case "ps2:get-overview": {
      // 계정 전체 뷰(지갑×패키지 매트릭스)용 스냅샷.
      return readStore(uid);
    }
    case "ps2:put-def":
      return putDef(uid, req.def);
    case "ps2:delete-def":
      return deleteDef(uid, req.defId);
    case "ps2:duplicate-def":
      return duplicateDef(uid, req.defId);
    case "ps2:put-package":
      return putPackage(uid, req.pkg);
    case "ps2:delete-package":
      return deletePackage(uid, req.packageId);
    case "ps2:bind":
      return bind(uid, req);
    case "ps2:update-binding":
      return updateBinding(uid, req);
    case "ps2:remove-binding":
      return removeBinding(uid, req);
    case "ps2:remove-wallet-package":
      return removePackageFromWallet(uid, req);
    case "ps2:put-wallet-package":
      return putWalletPackage(uid, req);
    case "ps2:copy-bindings":
      return copyBindings(uid, req);
    case "ps2:set-package-enabled":
      return setPackageEnabled(uid, req);
    case "ps2:provision-wallets":
      return provisionWallets(uid, req.addresses);
    case "ps2:install-market":
      return installMarket(uid, { defs: req.defs, pkg: req.pkg, scope: req.scope, params: req.params });
    default:
      // 새 메시지를 유니언에만 추가하고 케이스를 빠뜨리면 조용한 no-op이 된다 —
      // 시끄럽게 실패시킨다.
      throw new Error(`알 수 없는 ps2 메시지: ${(req as { type: string }).type}`);
  }
}

/** 지갑 동기화 프로비저닝 훅 — pasu-list-wallets 응답 직전에 호출된다.
 *  로그인 상태에서만(서버 지갑 목록이 있을 때만) 의미가 있다. */
export async function provisionFromWalletSync(addresses: string[]): Promise<void> {
  const uid = await getCurrentUserId();
  if (!uid || addresses.length === 0) return;
  await ensureSeeded(uid);
  await provisionWallets(uid, addresses);
}
