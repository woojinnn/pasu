// Default v2 policy — deny selling 0.05 or more USDC in a swap (token-amount cap).
//
// WHY THIS SHAPE: the raw `direction.amountIn` is a U256 hex String (Cedar can't
// compare it numerically), and the schema's `direction.amountInNano` LiveField is
// NOT host-populated at runtime — so a direct amount guard is inert. Instead this
// bundle's manifest declares a `token.normalize_to_nano` policy-RPC call that the
// host serves IN-PROCESS (a pure function — no oracle, no network): it rescales
// `amountIn` to token-native nano (`raw × 10^(9 − decimals)`) and projects the
// result into `context.custom.amountInNano` (a `Long`).
//
// `decimals` is passed as the literal `6` in the manifest — correct ONLY for
// USDC. So this policy is gated to USDC input by address (the same field the
// working address-block uses, always populated). For the gated set, 0.05 USDC =
// 0.05 × 10^9 = 50_000_000 nano. `custom` is optional and the call is optional,
// so the `has` guards fail-open (pass) if enrichment is absent.
@id("swap-usdc-amount-cap-deny")
@severity("deny")
@reason("Selling 0.05 or more USDC in a swap is blocked by your policy")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context.tokenIn.key has address
  && [
    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // Ethereum mainnet
    "0xaf88d065e77c8cc2239327c5edb3a432268e5831", // Arbitrum One (native USDC)
    "0x2791bca1f2de4661ed88a30c99a7a9449aa84174", // Polygon (USDC.e)
    "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913", // Base
    "0x0b2c639c533813f4aa9d7837caf62653d097ff85", // Optimism (native USDC)
    "0x3c499c542cef5e3811e1192ce70d8cc03d5c3359"  // Polygon (native USDC)
  ].contains(context.tokenIn.key.address)
  && context has custom
  && context.custom has amountInNano
  && context.custom.amountInNano >= 50000000
};
