// scopeball default v2 policy — explorer id: NFT-02 (public listing far below the
// collection floor).
//
// A fat-fingered or tricked public listing priced well under the collection
// floor is swept instantly by sweep bots — the loss is locked in before a human
// can cancel. Warn when the listing price is a low fraction of the floor.
//
// State-free: this is Action (the signed listing price) ÷ Oracle (collection
// floor) only. The listing arrives undecoded as `Core::Action::"Unknown"`
// (OffchainSig), so the price is NOT a base context field, and floor ÷ price is
// arithmetic Cedar cannot do — both are folded into the `external.nft_floor`
// policy_rpc, which decodes the order, fetches the floor, and projects the
// `listingPctOfFloor` ratio (decimal). (Aggregator type 2 — Valuation; reuses
// the floor method shared with NFT-01/NFT-06.)
//
// `custom` and the call are optional, so the body `has`-checks the path and
// no-ops until the floor host ships. The ratio is a Cedar `decimal`, so the
// threshold uses `.lessThan(...)` (70% of floor; personalized `floor_warn_pct`
// is a later increment).
@id("nft-low-floor-listing-warn")
@severity("warn")
@reason("this listing is priced well below the collection floor — sweep bots will take it instantly")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has listingPctOfFloor
  && context.custom.listingPctOfFloor.lessThan(decimal("0.7000"))
};
