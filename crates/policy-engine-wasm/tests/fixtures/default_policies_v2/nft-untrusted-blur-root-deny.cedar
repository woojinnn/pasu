// dambi default v2 policy — explorer id: NFT-03 (Blur "Root" bulk-listing
// signature whose EIP-712 domain is not the genuine Blur exchange).
//
// Blur lets a single "Root" signature list an entire collection at once; the
// bundled hash is opaque, so the wallet cannot unfold what is being signed
// (SlowMist-confirmed phishing primitive). The defense is provenance: an opaque
// order-signature whose declared EIP-712 domain is not a trusted marketplace
// must be denied — signing something you cannot read, from an unrecognized
// origin, is itself the danger signal (overlaps GEN-09's blind-sign warn, which
// this hardens to a deny for the Blur-Root shape).
//
// The Blur Root cannot be decoded, so it surfaces as `Core::Action::"Unknown"`
// with an `OffchainSig` nature. The signed EIP-712 domain name is the base
// context String `context.meta.nature.domain.name` (Aggregator type 5 — Set
// Membership against a literal allowlist). The trusted-marketplace domain
// allowlist is configuration; for this slice it is inlined as a Cedar set
// literal. `domain` and `domain.name` are optional in the schema (present only
// on offchain_sig natures), so the body `has`-checks the path before reading it.
@id("nft-untrusted-blur-root-deny")
@severity("deny")
@reason("this opaque order signature claims a marketplace domain that is not on the trusted allowlist")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context.meta.nature.kind == "offchain_sig"
  && context.meta.nature has domain
  && context.meta.nature.domain has name
  && !(["Blur Exchange", "Seaport"].contains(context.meta.nature.domain.name))
};
