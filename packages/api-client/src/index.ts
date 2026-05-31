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

// Phase 1 catalog endpoints (policy schema, templates, examples,
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

// Phase 2 verdict / audit / history / findings.
export {
  listAuditVerdicts,
  getAuditCounts,
  auditExportUrl,
  listHistoryVerdicts,
  listFindings,
  createVerdict,
  setVerdictDecision,
  type VerdictDto,
  type VerdictListOpts,
  type VerdictRangeAlias,
  type CreateVerdictBody,
  type ContractRef,
  type SelectorRef,
  type PolicyRef,
} from "./verdicts";
