// blocks: an ERC-20 transfer whose recipient is a contract when the user's policy expects plain-wallet (EOA) payees only.
// why:    crypto.com-help-style "send to a wallet, not a contract" guardrail — sending tokens to an unexpected contract address can lock funds or feed a malicious receiver; restricting payouts to EOAs catches mis-pasted contract addresses (wrong-recipient-type threat class).
// bucket: wallet/cooldown
// tags: bucket:wallet cat:cooldown enrichment method:address.activity action-family:cooldown
// methods: address.activity -> ../../../_methods/address.activity.md
// dormant: until address.activity is registered in method-catalog.json
//
// Cooldown — warn when the transfer recipient is a deployed contract (not an EOA).
// `address.activity` enriches the contract-ness flag. ENRICHMENT — dormant.
@id("eoa-only-recipient")
@severity("warn")
@reason("Recipient is a contract, not a regular wallet — confirm you meant to send tokens to a contract")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientIsContract
  && context.custom.recipientIsContract == true
};
