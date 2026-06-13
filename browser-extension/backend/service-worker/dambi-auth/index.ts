/**
 * Dambi (Rust server) auth surface for the service worker.
 *
 * Tight, message-friendly API the popup can call via
 * `Browser.runtime.sendMessage`. The actual handlers are wired in
 * `service-worker/index.ts`.
 */

export {
  SERVER_BASE_URL,
  ServerError,
  setOnSessionExpired,
  resetSessionExpiredGuard,
  request,
  fetchMe,
  listWallets,
  listWalletSummaries,
  addWallet,
  updateWallet,
  deleteWallet,
  evaluate,
  type Me,
  type WalletId,
  type WalletSummary,
  type AddWalletBody,
  type AddWalletResp,
  type RequestOptions,
  type EvaluateRequestDto,
  type EvaluateResponseDto,
} from "./client";

export { startGoogleLogin, parseTokensFromUrl } from "./oauthFlow";

export {
  getAccessToken,
  getRefreshToken,
  setTokens,
  clearTokens,
} from "./tokenStore";
