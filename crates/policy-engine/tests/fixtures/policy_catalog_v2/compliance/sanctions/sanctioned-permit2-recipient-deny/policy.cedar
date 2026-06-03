// blocks: signing a Permit2 SignatureTransfer that authorizes a sanctioned spender to pull the token.
// why:    sanctions screening — a one-time Permit2 transfer signature whose authorized spender is OFAC/EU/UN-listed hands value to a blocked party off-chain; screen the on-message spender (the recipient is not in the signed struct, so spender is the bind-site). (OFAC SLS / Treasury SDN list.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:approval
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the Permit2 authorized spender against OFAC/EU/UN
// lists. Off-chain signature (`meta.nature.kind == "offchain_sig"`). ENRICHMENT —
// dormant until the dispatcher serves the method.
@id("sanctioned-permit2-recipient-deny")
@severity("deny")
@reason("Permit2 transfer spender is on a sanctions list (OFAC/EU/UN) — signature blocked")
forbid(principal, action == Token::Action::"Permit2SignTransfer", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
