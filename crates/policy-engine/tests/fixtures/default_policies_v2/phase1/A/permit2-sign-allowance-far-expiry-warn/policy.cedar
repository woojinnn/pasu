// scopeball policy — Uniswap Permit2 PermitSingle ("sign to verify / connect")
// whose allowance expiry is years out (an open-ended standing allowance).
//
// Permit2 off-chain-signature phishing is the dominant high-value drain vector:
// a single PermitSingle signature whose `spender` is the attacker grants a
// standing allowance the wallet screen hides behind a "verify / connect" prompt
// (PEPE $1.39M; spWETH $32.43M single-signature loss — cited in-repo in
// permit2-unknown-spender-full-balance-warn/policy.cedar). Ordinary users click
// "verify/connect" and never read the EIP-712 JSON, so a far-future `expiresAt`
// — meaning the granted allowance stays live for years — goes unnoticed.
//
// This is a pure action-field policy (no enrichment): the real Permit2 path
// lowers to `Token::Action::"Permit2SignAllowance"` and carries `expiresAt` as a
// NON-OPTIONAL base context Long (unix seconds; schema permit2_sign_allowance
// .cedarschema:38, populated unconditionally by lowering_v2/token/
// permit2_sign_allowance.rs:29). No has-guard is needed because the field is
// non-optional; the manifest declares no policy_rpc and no custom_context.
//
// The cutoff is an absolute unix-seconds horizon (2 000 000 000 ≈ 2033-05),
// well beyond any reasonable standing-allowance window — exact precedent
// nft-far-expiry-order-warn (context.meta.nature.deadline > 2000000000). People
// do sometimes grant long allowances deliberately, hence warn, not deny.
// Personalizing the cutoff to `submittedAt + user.allowance_max_horizon` needs
// Cedar Long arithmetic (a later enrichment increment); this slice uses the
// absolute far-future cutoff.
@id("permit2-sign-allowance-far-expiry-warn")
@severity("warn")
@reason("This Permit2 signature grants a standing allowance that does not expire for years — open-ended allowances stay drainable long after you forget them")
forbid(principal, action == Token::Action::"Permit2SignAllowance", resource)
when {
  context.expiresAt > 2000000000
};
