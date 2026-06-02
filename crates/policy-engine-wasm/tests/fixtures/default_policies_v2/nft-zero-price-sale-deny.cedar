// scopeball default v2 policy — explorer id: NFT-01 (signing away an owned NFT
// at effectively zero / far-below-floor price).
//
// On Seaport/Blur an NFT sale is a *signature*, not a transaction. Phishing
// disguises a "free mint" as a listing that actually signs away an expensive NFT
// the user holds for ~0 — a single Blur 0-price listing cost one user a $240K
// NFT. Deny when the order lists an NFT the wallet actually owns and the total
// consideration is far below the collection floor.
//
// The marketplace order has no dedicated domain — it arrives undecoded as
// `Core::Action::"Unknown"` (OffchainSig). Two facts must be composed and they
// are NOT in the base context: (1) does an `offer` item match an
// `TokenKey::Erc721`/`Erc1155` the wallet holds as `Balance::Owned`
// (durable State read of `token_holdings`), and (2) the Σ-consideration ÷
// collection-floor ratio (external floor feed + arithmetic Cedar cannot do).
// The new `external.nft_floor` policy_rpc decodes the order, checks ownership,
// fetches the floor, and projects `ownedNftListed` (Bool) and
// `considerationVsFloor` (decimal). This is Aggregator types 1+2+9 fused into
// one call (precedent: `approval.unlimited_over_balance` emits two fields).
//
// `custom` is optional and the call is `optional`, so the guards `has`-check the
// path; the policy no-ops (fail-open) until the decoder/floor host ships. The
// ratio is a Cedar `decimal`, so the threshold uses `.lessThan(...)` (10% of
// floor; personalized `min_ratio` is a later increment).
@id("nft-zero-price-sale-deny")
@severity("deny")
@reason("this order sells an NFT you own for far below its collection floor — a classic zero-price listing drain")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has ownedNftListed
  && context.custom.ownedNftListed
  && context.custom has considerationVsFloor
  && context.custom.considerationVsFloor.lessThan(decimal("0.1000"))
};
