// scopeball default v2 policy — explorer id: LP-06 (paying a sale with a token
// it does not accept).
//
// Mixing up stablecoins and committing with a token the sale does not accept
// can strand funds irreversibly. The defence compares the pay token against the
// token the sale declared it accepts, denying on a mismatch.
//
// sv = COV: no durable state — but the sale's accepted token is EXTERNAL info
// (registry / Aggregate), and Cedar cannot reach it. `context.payToken` is a
// nested `Core::TokenRef` whose identity lives at `payToken.key.address`
// (optional, hex String) — not numerically comparable, and the sale token is
// off-context anyway. So this is an EXTERNAL method
// (`launchpad.pay_token_match`) that resolves the sale's accepted token from
// the registry and projects the verdict as `context.custom.payTokenMatchesSale`
// (Bool). The call is `optional`: until the registry provider ships, its
// absence fail-opens (`context.custom has payTokenMatchesSale` is false ->
// no-op). Deny when the pay token does NOT match the sale's declared token.
@id("lp-commit-pay-token-mismatch-deny")
@severity("deny")
@reason("This pay token is not the token the sale accepts")
forbid(principal, action == Launchpad::Action::"Commit", resource)
when {
  context has custom
  && context.custom has payTokenMatchesSale
  && context.custom.payTokenMatchesSale == false
};
