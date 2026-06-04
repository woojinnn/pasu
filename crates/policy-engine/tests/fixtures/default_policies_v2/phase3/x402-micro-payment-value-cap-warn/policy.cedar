// BLOCKED-BY-ACTION: erc3009_transfer_with_auth surface not yet in scopeball schema (see x402 gap-spec)
//
// scopeball policy — x402 (EIP-3009 TransferWithAuthorization / ReceiveWithAuthorization,
// the exact x402 "exact" payment scheme) whose signed `value` is far above any plausible
// micro-payment fee. x402 is sold as a micro-payment rail, but the EIP-3009 typed-data the
// user signs carries an arbitrary uint256 `value`; a "sign-to-verify" phishing page can ask
// for a `value` near the wallet's entire USDC balance, and a single off-chain signature then
// drains it wholesale — identical outcome to permit/permit2 single-signature drains
// (PEPE Permit2 $1.39M 2024-10; spWETH ~$32M 2024-09 — decrypt, cointelegraph). SlowMist
// observed an EIP-3009 authorization for 116.239404 USDC matching the stolen amount; KuCoin's
// advisory treats any x402 payment exceeding ~1 USDC as a payment-scam red flag.
//
// This is a cedar-only policy on a BASE field — no enrichment, no policy_rpc, no custom_context.
// `valueUsd` is the host-populated OPTIONAL USD estimate on the action surface (mirrors
// Token::Erc20Permit `amountUsd?`), so it is `has`-guarded and compared as a Cedar `decimal`
// (.greaterThan, 4dp dotted). If the host has not projected `valueUsd`, the comparison is
// skipped (fail-open) and the equivalent USD can later be sourced via oracle.usd_value as an
// additive enrichment increment. The per-payment ceiling here is an inlined policy constant
// (10.0000 USD — generously above KuCoin's ~1 USDC micro-fee expectation); people do sometimes
// make larger legitimate x402 payments, hence warn, not deny.
@id("x402-micro-payment-value-cap-warn")
@severity("warn")
@reason("This x402 payment signature authorizes a USD value far above a normal micro-payment fee — a single signature this large can drain most of your balance, just like permit phishing")
forbid(principal, action == Token::Action::"Erc3009TransferWithAuth", resource)
when {
  context has valueUsd
  && context.valueUsd.greaterThan(decimal("10.0000"))
};
