// blocks: claiming an airdrop into a recipient (other than the signer) that is on a sanctions list.
// why:    sanctions screening — directing a claim to a sanctioned third-party recipient routes value to a blocked party; warn (not deny) since the claim itself may be legitimate and the recipient redirect is the suspicious leg (OFAC SLS / Treasury SDN list).
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:airdrop
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the claim recipient against OFAC/EU/UN lists; the
// rule fires only when the recipient is redirected away from the signer.
// ENRICHMENT — dormant until the dispatcher serves the method.
@id("sanctioned-airdrop-claim-warn")
@severity("warn")
@reason("Airdrop claim recipient (not you) is on a sanctions list (OFAC/EU/UN) — review before claiming")
forbid(principal, action == Airdrop::Action::"Claim", resource)
when {
  context.recipient != context.meta.submitter
  && context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
