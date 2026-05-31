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
  type WalletId,
  type BlockHeight,
  type WalletStateView,
} from "./wallets";

export { listPolicies, type InstalledPolicy } from "./policies";

export { listTransactions, type TxRow } from "./transactions";
