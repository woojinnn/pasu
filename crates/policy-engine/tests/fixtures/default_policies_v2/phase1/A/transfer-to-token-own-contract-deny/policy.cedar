// scopeball default v2 policy — axiomatic-irreversibility (rule 2c) + extends a
// shipped sibling (rule 2a).
//
// AXIOM: an ERC20 sent to its OWN token contract address is definitionally,
// permanently lost. A plain ERC20 has no recovery path and no receive handler
// for tokens that land at the contract's own address — there is no key holder
// and no admin sweep. Etherscan and major token pages persistently warn "tokens
// sent to the contract address are lost", and "I sent USDC to the USDC
// contract, can I get it back" is a recurring (always-no) support thread.
//
// GAP CLOSED: the shipped DENY `send-first-time-or-burn-recipient-warn` matches
// ONLY the two literal burn strings 0x000…000 / 0x…dead — verified it never
// compares `recipient` against the token's own contract. No other fixture makes
// this self-reference comparison either.
//
// Both sides are base, non-optional/optional `String` fields of
// `Token::Erc20TransferContext` (same enrichment-free shape as the burn sibling
// and swap-recipient-not-self-deny): `recipient` is base `String`, and
// `token.key.address` is the ERC20 contract address — present iff
// standard == "erc20" (core.cedarschema TokenKey), so it is read behind a
// `has address` guard. Addresses are lowercase hex on both sides
// (host-normalized), so exact String equality holds. No policy_rpc, no custom
// context, no new method. A transfer here is permanent loss, hence `deny`.
@id("transfer-to-token-own-contract-deny")
@severity("deny")
@reason("This send goes to the token's own contract address — ERC20 sent there is permanently lost")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context.token.key has address &&
  context.recipient == context.token.key.address
};
