// blocks: signing a Permit2 SignatureTransfer whose authorized spender is frozen on the token's issuer blocklist — soft warn.
// why:    issuer-freeze screening — authorizing a Permit2 spender the stablecoin issuer has frozen (Circle/Centre blocklist, Tether isBlackListed) on the very token being delegated grants a pull-allowance to an address that cannot move the asset: the signed transfer would revert or strand value, and signing to a flagged spender is a counterparty-under-enforcement signal. The recipient is not in the signed struct, so the on-message `spender` is the bind-site. Warn (not deny) — a signature is a grant, not yet a movement. (Circle / Centre FiatTokenV2 isBlacklisted; Tether TetherToken isBlackListed.)
// bucket: compliance/issuer-freeze
// tags: bucket:compliance cat:issuer-freeze enrichment method:address.frozen could-be-compliance:true action-family:approval
// methods: address.frozen -> ../../../_methods/address.frozen.md
// dormant: until address.frozen is registered in method-catalog.json
//
// `address.frozen` views the issuer's on-chain blocklist for (asset=token, address=spender).
// Asset-scoped to the delegated token. Off-chain signature (`meta.nature.kind == "offchain_sig"`).
// ENRICHMENT — dormant until the dispatcher serves the method; the `context.custom has frozen`
// guard is inert until then.
@id("frozen-permit2-recipient-warn")
@severity("warn")
@reason("Permit2 transfer spender is frozen on this token's issuer blocklist — review before signing")
forbid(principal, action == Token::Action::"Permit2SignTransfer", resource)
when {
  context has custom
  && context.custom has frozen
  && context.custom.frozen == true
};
