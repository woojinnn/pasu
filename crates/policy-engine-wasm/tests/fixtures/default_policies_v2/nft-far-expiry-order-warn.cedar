// dambi default v2 policy — explorer id: NFT-07 (order signature with a very
// distant / effectively open-ended expiry).
//
// A Seaport/Blur listing or offer signature stays exploitable until its
// `deadline`. A signature that lives far past any reasonable listing window is a
// "free option" handed to a counterparty — it can still be filled long after the
// floor has dropped. Warn on such long-lived order signatures (people do use long
// listings deliberately, hence warn, not deny).
//
// This is a pure action-field policy (Aggregator type 8 — Decode Introspection):
// the marketplace order arrives undecoded as `Core::Action::"Unknown"` with an
// `OffchainSig` nature, and the signed expiry is the base context Long
// `context.meta.nature.deadline` (unix seconds, present only on offchain_sig
// natures). No enrichment is required — the manifest declares no policy_rpc and
// no custom_context. `deadline` is optional in the schema (omitted by onchain_tx
// natures), so the body `has`-checks the path before reading it.
//
// The cutoff is an absolute unix-seconds horizon (here 2 000 000 000 ≈ 2033-05),
// well beyond any normal listing window; personalizing it to
// `submittedAt + user.order_max_horizon` needs Cedar Long arithmetic (a later
// enrichment increment), so this slice uses the absolute far-future cutoff.
@id("nft-far-expiry-order-warn")
@severity("warn")
@reason("this order signature does not expire for a very long time — long-lived signatures stay exploitable after the floor drops")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context.meta.nature.kind == "offchain_sig"
  && context.meta.nature has deadline
  && context.meta.nature.deadline > 2000000000
};
