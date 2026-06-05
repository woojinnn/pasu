// scopeball default v2 policy — explorer id: TOKENNFT-3 (Seaport criteria=any /
// untrusted-zone wildcard order).
//
// A Seaport collection-offer / criteria order can leave `offer`/`consideration`
// open to ANY token id in the collection (`identifierOrCriteria == 0`), so a
// single signature can drain *any* NFT the user holds — the user never sees this.
// A non-standard `zone` hands fill conditions to a third party. Deny when the
// signed order is criteria=any OR routes through an untrusted zone (Scam
// Sniffer / SlowMist pattern).
//
// The order arrives undecoded as `Core::Action::"Unknown"` (OffchainSig); the
// criteria/zone are NOT base context fields, so the new
// `external.seaport_order_shape` policy_rpc decodes the Seaport order and
// introspects it (Aggregator type 8 — Decode Introspection + type 9 — External
// zone allowlist), projecting `seaportWildcardCriteria` and `seaportUntrustedZone`
// (Bool). The trusted-zone allowlist lives inside the method.
//
// `custom` and the call are optional, so the body `has`-checks each path and
// no-ops until the Seaport decoder host ships.
@id("nft-seaport-wildcard-zone-deny")
@severity("deny")
@reason("this Seaport order is open to any token in the collection or routes through an untrusted zone")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && (
    (context.custom has seaportWildcardCriteria && context.custom.seaportWildcardCriteria)
    || (context.custom has seaportUntrustedZone && context.custom.seaportUntrustedZone)
  )
};
