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
  type WalletState,
  type ApprovalRisk,
  type ClassifiedApprovals,
  type ClassifiedErc20Approval,
  type ClassifiedSetForAllApproval,
  type ClassifiedPermit2Approval,
} from "./wallets";

// Phase 3 dashboard summary.
export {
  getDashboardSummary,
  type DashboardSummary,
  type DashboardWalletSummary,
  type ChainShare,
} from "./dashboard";

// Cedar validate/test/simulate moved off the server — call
// `@scopeball/cedar-wasm` (via `apps/web/src/cedar/`) directly.
// Selector decode + revoke calldata builder moved to
// `apps/web/src/tools/` (pure TS, no roundtrip).

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
