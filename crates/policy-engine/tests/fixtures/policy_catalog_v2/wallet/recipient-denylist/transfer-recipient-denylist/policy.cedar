// blocks: an ERC-20 transfer whose recipient is on the user/operator denylist of known-bad addresses (lowercase 0x).
// why:    sending funds to a known-malicious / drainer / scam address (address-poisoning, known-bad-recipient threat class); can also back a sanctions/OFAC-style screening feed.
// bucket: wallet/recipient-denylist  (precedence-won)
// tags: bucket:wallet cat:recipient-denylist static could-be-compliance:sanctions-feed
@id("transfer-recipient-denylist")
@severity("deny")
@reason("Recipient is on your denylist of known-malicious addresses — transfer blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  ["0x000000000000000000000000000000000000dead", "0x2222222222222222222222222222222222222222"].contains(context.recipient)
};
