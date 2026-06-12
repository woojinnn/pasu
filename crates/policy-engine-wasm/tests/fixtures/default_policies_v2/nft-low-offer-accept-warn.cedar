// dambi default v2 policy — explorer id: NFT-06 (accepting a collection offer
// well below the floor).
//
// Someone bids (a collection offer) on your NFT and you "accept" it — a flow
// where which token sells at which price is easy to misread, so it is a prime
// loss path. Warn when the amount you would *receive* is well below the floor.
//
// State-free: Action (the incoming offer's WETH amount) ÷ Oracle (floor). The
// offer-acceptance is also a signature/order that arrives undecoded as
// `Core::Action::"Unknown"` (OffchainSig), so the receivable amount is NOT a
// base context field, and amount ÷ floor is arithmetic Cedar cannot do — both
// are folded into the shared `external.nft_floor` policy_rpc, which projects the
// `offerVsFloor` ratio (decimal). (Aggregator type 2 — Valuation; reuses the
// floor method shared with NFT-01/NFT-02.)
//
// `custom` and the call are optional, so the body `has`-checks the path and
// no-ops until the floor host ships. The ratio is a Cedar `decimal`, so the
// threshold uses `.lessThan(...)` (75% of floor; personalized `accept_warn_pct`
// is a later increment).
@id("nft-low-offer-accept-warn")
@severity("warn")
@reason("accepting this offer pays you well below the collection floor")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has offerVsFloor
  && context.custom.offerVsFloor.lessThan(decimal("0.7500"))
};
