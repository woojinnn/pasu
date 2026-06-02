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
  addWallet,
  syncWallet,
  getWalletState,
  getWalletHoldings,
  getWalletApprovals,
  getWalletApprovalsWithRisk,
  getWalletBlockHeights,
  patchWallet,
  deleteWallet,
  type WalletId,
  type AddWalletBody,
  type AddWalletResp,
  type BlockHeight,
  type WalletStateView,
  type ApprovalRisk,
  type ClassifiedApprovals,
  type ClassifiedErc20Approval,
  type ClassifiedSetForAllApproval,
  type ClassifiedPermit2Approval,
} from "./wallets";

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

export { listTokens, type TokenCatalogRow } from "./tokens";

// Catalog endpoints (policy schema, templates, examples,
// spenders, single policy fetch).
export {
  getPolicySchema,
  getPolicyTemplates,
  getExampleTransactions,
  getPolicy,
  getSpender,
  type PolicySchema,
  type PolicyPredicate,
  type PolicyActionMeta,
  type PolicyTemplate,
  type ExampleTransaction,
  type SpenderMeta,
  type SpenderRep,
} from "./catalog";

// Dashboard summary.
export {
  getDashboardSummary,
  type DashboardSummary,
  type DashboardWalletSummary,
  type ChainShare,
} from "./dashboard";

// Verdict / audit / history / findings — backed by
// chrome.storage.local via the extension bridge.
export {
  listAuditVerdicts,
  getAuditCounts,
  exportAuditCsv,
  listHistoryVerdicts,
  listFindings,
  setVerdictDecision,
  type VerdictDto,
  type VerdictListOpts,
  type VerdictRangeAlias,
  type CreateVerdictBody,
  type ContractRef,
  type SelectorRef,
  type PolicyRef,
} from "./verdicts";

export {
  sendToExtension,
  ExtensionBridgeError,
  ExtensionBridgeTimeout,
} from "./extension-bridge";

// Dashboard ↔ extension SW bridge for managed policies. Replaces the
// retired server-side `user_policies` CRUD (see policies.ts stubs).
export {
  putPolicy,
  deletePolicy as deleteManagedPolicy,
  listManagedPolicies,
  getEnabledPolicyIds,
  setEnabledPolicyIds,
  ENABLED_IDS_STORAGE_KEY,
  dashboardId,
  stripDashboardId,
  type ManagedPolicy,
} from "./extension-sync";

export { subscribeToBroadcast } from "./extension-bridge";

// Shared primitive types — kept in one file (./types) to mirror the
// Rust DTOs. Re-exported here so consumer pages can
// `import type { ... } from "../server-api"`.
export type {
  Address,
  ChainId,
  Decimal,
  UnixSeconds,
  AuthUser,
  TokenHolding,
  TokenMetadata,
  Balance,
  LiveFieldPrice,
  PolicySeverity,
  Verdict,
  VerdictRow,
  WalletState,
  I18nString,
} from "./types";
