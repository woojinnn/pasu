/**
 * Barrel export for the policy-rpc server API layer.
 *
 * Pages should import from here, not the individual files, so the
 * surface stays small and easy to refactor.
 */

export {
  SERVER_BASE_URL,
  ServerError,
  getStoredToken,
  getStoredRefreshToken,
  setStoredToken,
  setStoredRefreshToken,
  request,
  urlWithTokenQuery,
} from "./client";

export {
  startGoogleLogin,
  consumeTokensFromHash,
  fetchMe,
  logout,
  type Me,
} from "./auth";

export {
  listWallets,
  getWalletState,
  getWalletHoldings,
  getWalletApprovals,
  getWalletBlockHeights,
  patchWallet,
  deleteWallet,
  type WalletId,
  type BlockHeight,
  type WalletState,
} from "./wallets";

// Re-export every shared type so consumers can `import { … } from "@scopeball/api-client"`
// without juggling two packages.
export type {
  Address,
  ChainId,
  Decimal,
  UnixSeconds,
  AuthUser,
  TokenHolding,
  TokenMetadata,
  TokenCatalogRow,
  WalletState as WalletStateView,
  DashboardSummary,
  VerdictRow,
  Verdict,
  I18nString,
} from "@scopeball/types";

export {
  listPolicies,
  createPolicy,
  patchPolicy,
  deletePolicy,
  type InstalledPolicy,
  type CreatePolicyBody,
  type PatchPolicyBody,
} from "./policies";

export { listTransactions, type TxRow } from "./transactions";

export { listTokens } from "./tokens";
