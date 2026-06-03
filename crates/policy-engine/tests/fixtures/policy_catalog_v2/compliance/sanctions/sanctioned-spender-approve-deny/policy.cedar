// blocks: an ERC-20 approve granting allowance to a spender on an OFAC SDN / EU / UN sanctions list.
// why:    sanctions screening — handing spend authority to a legally-sanctioned spender is a compliance violation and a standing drain grant; screen the spender like a VASP screens a counterparty (OFAC SLS / Treasury SDN list).
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:approval
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the approve spender against OFAC/EU/UN lists.
// ENRICHMENT — dormant until the dispatcher serves the method.
@id("sanctioned-spender-approve-deny")
@severity("deny")
@reason("Spender is on a sanctions list (OFAC/EU/UN) — approval blocked")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
