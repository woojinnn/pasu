// dambi default v2 policy — deny any swap that SELLS USDC (USDC as the input
// token), across the major chains.
//
// Unlike the USD/token-amount caps, this guards a field the lowering ALWAYS
// populates: `tokenIn.key.address` is the input token's contract address,
// emitted as lowercase 0x-hex (`format!("{a:#x}")`) for every erc20 token. So
// the comparison is pure String equality with NO enrichment — it fires at
// runtime today.
//
// `address` is optional on `Core::TokenKey` (present iff standard == "erc20",
// absent for native gas tokens), so the guard `has`-checks the path first. The
// allowlisted addresses below are the canonical USDC deployments; add more chains
// by extending the set. Addresses are lowercase on both sides (lowering uses
// LowerHex), so exact equality holds.
@id("swap-usdc-input-deny")
@severity("deny")
@reason("Swapping out of USDC is blocked by your policy")
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
};
