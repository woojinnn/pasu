// scopeball default v2 policy — demand class: axiomatic-irreversibility (2c) +
// shipped-sibling extension onto a verified-dark tag (2a).
//
// AXIOM (2c): sending an NFT to the burn / zero address (0x000…000 / 0x…dead)
// is definitionally permanent loss — those addresses have no key holder, so the
// collectible (PFP, game item, ticket) can never be recovered. This is the NFT
// twin of the already-shipped ERC20 burn DENY (send-first-time-or-burn-recipient
// -warn, which forbids Token::Action::"Erc20Transfer" against the same literal
// set). The general-public "burn to claim / send to verify" scam weaponizes this
// irreversibility, instructing victims to send their NFT to the burn address.
//
// EXTENSION (2a): grep -rl 'nft_transfer' over the 122 default_policies_v2
// fixtures returns ZERO — the nft_transfer tag is currently unguarded. The
// schema (actions/token/nft_transfer.cedarschema) declares `recipient: String`
// as a non-optional base field of Token::NftTransferContext, so the identical
// Set.contains(context.recipient) burn-literal compare is pure Cedar (no
// enrichment, no policy_rpc, no custom_context) on Token::Action::"NftTransfer".
//
// Addresses are lowercase hex on both sides (host-normalized), so exact String
// Set membership holds. Permanent loss ⇒ deny (a hard block, not a one-beat warn),
// mirroring the severity of its shipped ERC20 sibling.
@id("nft-transfer-burn-recipient-deny")
@severity("deny")
@reason("This NFT is being sent to a burn address (0x000…000 / 0x…dead) — an NFT sent there is permanently lost")
forbid(principal, action == Token::Action::"NftTransfer", resource)
when {
  ["0x0000000000000000000000000000000000000000",
   "0x000000000000000000000000000000000000dead"].contains(context.recipient)
};
