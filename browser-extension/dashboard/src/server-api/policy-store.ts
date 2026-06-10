/** ps2:* SW 메시지의 typed 클라이언트 — P2 대시보드의 유일한 정책 스토어 경로.
 *  응답 봉투 해제/에러 throw는 sendToExtension이 수행한다. */
import { sendToExtension } from "./extension-bridge";
import type {
  Binding,
  HoleValue,
  LibraryDoc,
  PackageDef,
  PolicyDef,
  StoreSnapshot,
  WalletPolicyState,
} from "../../../sdk/policy-store-types";

export type {
  Binding,
  HoleSpec,
  HoleValue,
  LibraryDoc,
  PackageDef,
  PolicyDef,
  StoreSnapshot,
  WalletPolicyState,
} from "../../../sdk/policy-store-types";
export { isEffectiveOn, UNCATEGORIZED_PKG } from "../../../sdk/policy-store-types";

export const getLibrary = () =>
  sendToExtension<{ library: LibraryDoc; rev: number }>({ type: "ps2:get-library" });
export const getWalletState = (address: string) =>
  sendToExtension<WalletPolicyState>({ type: "ps2:get-wallet-state", address });
export const getOverview = () => sendToExtension<StoreSnapshot>({ type: "ps2:get-overview" });

export const putDef = (def: PolicyDef) => sendToExtension<null>({ type: "ps2:put-def", def });
export const deleteDef = (defId: string) => sendToExtension<null>({ type: "ps2:delete-def", defId });
export const duplicateDef = (defId: string) => sendToExtension<string>({ type: "ps2:duplicate-def", defId });

export const putPackage = (pkg: PackageDef) => sendToExtension<null>({ type: "ps2:put-package", pkg });
export const deletePackage = (packageId: string) =>
  sendToExtension<null>({ type: "ps2:delete-package", packageId });

export const bindDef = (opts: {
  defId: string;
  packageId: string;
  addresses: string[];
  params?: Record<string, HoleValue>;
  enabled?: boolean;
}) => sendToExtension<null>({ type: "ps2:bind", ...opts });

export const updateBinding = (opts: {
  address: string;
  bindingId: string;
  patch: Partial<Pick<Binding, "enabled" | "params" | "packageId">>;
}) => sendToExtension<null>({ type: "ps2:update-binding", ...opts });

export const removeBinding = (opts: { address: string; bindingId: string }) =>
  sendToExtension<null>({ type: "ps2:remove-binding", ...opts });

export const copyBindings = (opts: { fromAddress: string; toAddress: string; bindingIds: string[] }) =>
  sendToExtension<null>({ type: "ps2:copy-bindings", ...opts });

export const setPackageEnabled = (opts: { address: string; packageId: string; enabled: boolean }) =>
  sendToExtension<null>({ type: "ps2:set-package-enabled", ...opts });

export const provisionWallets = (addresses: string[]) =>
  sendToExtension<null>({ type: "ps2:provision-wallets", addresses });
