// blocks: granting an ERC-20 approval that authorizes more than $10k of USD value.
// why:    approval-allowance drain — a bounded-but-large allowance still lets a
//         compromised/malicious spender pull a meaningful balance (EIP-20 approve).
// bucket: wallet/usd-cap  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// Catalog A2 — warn on a high-USD-value ERC-20 approval.
//
// Even a non-unlimited approval can be large enough to matter. `oracle.usd_value`
// enriches the approved amount into `context.custom.approvalUsd` (a Decimal); the
// policy warns above $10k. ENRICHMENT — dormant until a `/v1/rpc` dispatcher
// serves `oracle.usd_value` (the `has` guard keeps it inert meanwhile).
@id("large-erc20-approve")
@severity("warn")
@reason("This approval authorizes a large USD value — confirm the spender and amount")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has approvalUsd
  && context.custom.approvalUsd.greaterThan(decimal("10000.0000"))
};
