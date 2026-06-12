// dambi default v2 policy — explorer id: BRG-04 (bridging to a destination
// chain the user does not actually use).
//
// Bridging to an unintended chain effectively strands funds (assets on an
// unsupported chain are hard or impossible to pull back). The destination chain
// id (LayerZero EID / CCTP domain / etc.) is encoded INSIDE the bridge
// calldata; with no dedicated bridge decode the move lands as `Core::Unknown`,
// where `dstChainId` is buried in the `calldata` hex String — NOT a parsed,
// comparable field, so Cedar cannot read it. The "supported chains" reference
// set is durable wallet state: `WalletId.chains` (the set of chains the wallet
// tracks / uses). Neither side is Cedar-comparable from the base context, so
// this is a METHOD: `bridge.dest_chain_supported` extracts `dstChainId` from
// the calldata and tests membership in `WalletId.chains`, emitting the Bool
// `destChainSupported`.
//
// `custom` and the field are optional; the call is `optional` and fail-opens
// until the sim-server fact host ships. Warn (not deny) when the destination
// chain is NOT in the wallet's chain set.
@id("bridge-dest-chain-unsupported-warn")
@severity("warn")
@reason("Bridging to a chain you do not use — funds may be stranded on the destination chain")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has destChainSupported
  && context.custom.destChainSupported == false
};
