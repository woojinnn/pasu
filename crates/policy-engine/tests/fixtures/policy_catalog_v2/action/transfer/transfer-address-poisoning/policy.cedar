// blocks: an ERC-20 transfer to a lookalike address that collides (prefix/suffix) with a real counterparty the wallet has interacted with — the address-poisoning trap.
// why:    address-poisoning — attackers spam zero-value / dust transfers from vanity addresses that match the first/last hex chars of a known counterparty, hoping the user copies the poisoned entry from their history; funds sent are unrecoverable. Source: plan §2A transfer / etherscan address-poisoning advisories.
// bucket: action/transfer
// tags: bucket:action cat:transfer enrichment method:address.similarity action-family:transfer
// methods: address.similarity -> ../../../_methods/address.similarity.md
// Catalog (action/transfer) — warn when the recipient is a poisoning lookalike of
// a known counterparty. `address.similarity` enriches `poisonCollision` (the
// candidate recipient resembles a prior counterparty but is not it).
// ENRICHMENT — dormant until the dispatcher serves `address.similarity`.
// dormant: until address.similarity is registered in method-catalog.json
@id("transfer-address-poisoning")
@severity("warn")
@reason("Recipient closely resembles an address you've used before but is not the same — possible address-poisoning; verify carefully")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has poisonCollision
  && context.custom.poisonCollision == true
};
