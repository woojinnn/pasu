// scopeball policy — Approval Hygiene Set (signal: approval-phishing-ecosystem-
// scale-1bn). Approval phishing is the largest single drainer vector:
// Chainalysis estimates ~$1.0B cumulative; 2024 wallet-drainer losses hit $494M
// (+67% YoY), much of it routed through fake airdrop / claim / mint sites that
// coax an `approve` (often unlimited or Permit2). drainer-as-a-service makes the
// pattern commodity. (chainalysis, infosecurity, therecord, securityweek)
//
// External Fact Lookup (Aggregator type 9; sv = COV). The "is this dApp origin a
// freshly-registered / low-reputation domain presenting an approval-phishing
// claim/mint context?" judgement lives OUTSIDE wallet state and outside any
// action base field — domain age / reputation is a registry/feed concern. The
// `external.fresh_domain_approval_phishing` policy_rpc call folds that verdict
// into `context.custom.freshDomainApprovalPhishing` (Bool). It is `optional`, so
// until the feed ships the call fail-opens: `context.custom has
// freshDomainApprovalPhishing` is false and the policy no-ops rather than
// SystemFail. It activates automatically once the provider lands.
@id("approve-fresh-domain-airdrop-context-deny")
@severity("deny")
@reason("This approval comes from a newly seen, low-reputation site posing as an airdrop or mint — a common approval-phishing setup")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has freshDomainApprovalPhishing
  && context.custom.freshDomainApprovalPhishing
};
