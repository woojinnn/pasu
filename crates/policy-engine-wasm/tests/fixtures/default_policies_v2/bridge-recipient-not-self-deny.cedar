// dambi default v2 policy — explorer id: BRG-01 (bridge sends to a
// recipient that is not the user's own wallet).
//
// Address-poisoning is the single highest-loss path in bridging ($68M WBTC in
// one case): the destination/recipient sits as a separate calldata argument the
// user rarely scrutinises, so a look-alike address is easy to miss. A bridge
// move that has no dedicated decode lands as `Token::Erc20Transfer`, where the
// destination is the parsed, COMPARABLE String field `context.recipient` and
// the actor's own address is `principal.address` (Core::Wallet entity attr).
//
// `dest_is_self` is therefore a pure action-field comparison — `recipient`
// equals the wallet address — needing NO enrichment / policy_rpc call: Cedar
// compares the two Strings for equality directly. The manifest stays pure
// ({id, schema_version, trigger}). Deny when the recipient is NOT the wallet.
//
// Addresses are lowercase hex on both sides (lowering normalizes), so `==`
// is a sound identity test here.
@id("bridge-recipient-not-self-deny")
@severity("deny")
@reason("Bridge recipient is not your own wallet address — likely address poisoning")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context.recipient != principal.address
};
