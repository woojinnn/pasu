// blocks: an ERC-20 transfer to the zero address (0x000…0), which most tokens treat as an irreversible burn.
// why:    irretrievable burn — sending to the null address destroys the tokens with no recovery path; it is almost always a mistake or a malicious default recipient (irretrievable-burn threat class). Source: plan §2A transfer / EIP-20.
// bucket: action/transfer
// tags: bucket:action cat:transfer static action-family:transfer
// Catalog (action/transfer) — warn when the recipient is the all-zero address.
// `recipient` is a lowercase 0x-hex string; the zero address renders canonically
// as 40 hex zeros. Pure-static.
@id("transfer-to-zero-address")
@severity("warn")
@reason("Recipient is the zero address — this will burn the tokens irreversibly; confirm before sending")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context.recipient == "0x0000000000000000000000000000000000000000"
};
