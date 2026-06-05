// blocks: signing an off-chain marketplace order whose proceeds (recipient) go to an address other than the signer.
// why:    proceeds redirection — a malicious listing UI sets the order recipient to an attacker so the sale proceeds never reach the seller; the signed intent is a solver/filler mandate, so the redirect is locked in at sign time. Source: plan §2A nft-order / Seaport order model.
// bucket: action/nft
// tags: bucket:action cat:nft static action-family:nft action:sign_intent_order
// Catalog (action/nft) — warn when an intent-order's recipient is not the signer.
// SignIntentOrder models a marketplace listing/intent with `recipient:String`;
// `meta.submitter` is the signer. Both lowercase 0x. Pure-static (applies to the
// offchain_sig listing form; the recipient redirect is the threat regardless).
@id("nft-listing-proceeds-not-self")
@severity("warn")
@reason("Order proceeds are routed to an address other than yours — confirm the recipient before signing")
forbid(principal, action == Amm::Action::"SignIntentOrder", resource)
when {
  context.recipient != context.meta.submitter
};
