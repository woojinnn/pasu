// scopeball default v2 policy — NFT Seaport zero-consideration drainer order.
//
// A Seaport NFT sale is an off-chain EIP-712 *signature*, not a transaction.
// Phishing disguises the order as a "login / verify ownership / connect" prompt.
// The attacker crafts a valid Seaport order with the user's NFT(s) in the `offer`
// array and 0 ETH / 1 wei directed to the attacker in the `consideration` array,
// so a single signature gives the NFT away for nothing. The consideration total
// is invisible to a beginner unless the order is parsed (Scam Sniffer pattern).
//
// The order arrives undecoded as `Core::Action::"Unknown"` (OffchainSig); the
// consideration total is NOT a base context field. This reuses the SAME
// `external.seaport_order_shape` policy_rpc already declared by
// `nft-seaport-wildcard-zone-deny`, adding a third projected Bool
// `seaportConsiderationNearZero` (true when total consideration to the user is
// <= a dust threshold, e.g. 1 wei, while the offer gives away an NFT). The
// near-zero arithmetic / dust threshold lives inside the method.
//
// `custom` and the call are optional, so the body `has`-checks the path and
// no-ops (fail-open) until the Seaport decoder host ships.
@id("nft-seaport-zero-consideration-sign-deny")
@severity("deny")
@reason("this Seaport order gives away your NFT for zero or near-zero consideration to the counterparty")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has seaportConsiderationNearZero
  && context.custom.seaportConsiderationNearZero
};
