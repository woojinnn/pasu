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
  getWalletApprovalsWithRisk,
  getWalletBlockHeights,
  patchWallet,
  deleteWallet,
  type WalletId,
  type BlockHeight,
  type WalletState,
  type ApprovalRisk,
  type ClassifiedApprovals,
  type ClassifiedErc20Approval,
  type ClassifiedSetForAllApproval,
  type ClassifiedPermit2Approval,
  type SpenderMetaInline,
} from "./wallets";

// Phase 3 dashboard summary.
export {
  getDashboardSummary,
  type DashboardSummary,
  type DashboardWalletSummary,
  type ChainShare,
} from "./dashboard";

// Phase 4 cedar editor support.
export {
  validatePolicy,
  testPolicy,
  type ValidateResp,
  type CedarRequestInput,
  type TestPolicyResp,
  type MatchedPolicyDto,
} from "./cedar";

// Re-export every shared type so consumers can `import { … } from "@scopeball/api-client"`
// without juggling two packages. (`DashboardSummary` is the live shape
// from ./dashboard above — the stub in @scopeball/types pre-dates Phase 3
// and will be removed in a follow-up.)
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
