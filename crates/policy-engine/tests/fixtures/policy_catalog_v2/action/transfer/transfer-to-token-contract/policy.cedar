// blocks: an ERC-20 transfer whose recipient is the token's own contract address, where the tokens are effectively burned / stuck.
// why:    fat-finger / paste-the-contract — users frequently paste the ERC-20 contract address into the recipient field; most ERC-20s have no recovery, so the funds are permanently lost (irretrievable-burn threat class). Source: plan §2A transfer / EIP-20.
// bucket: action/transfer
// tags: bucket:action cat:transfer static action-family:transfer
// Catalog (action/transfer) — warn when the transfer recipient equals the token
// contract itself. Erc20Transfer carries `token:TokenRef` (NOT `asset`) and a
// `recipient:String`. `token.key.address` is present only for erc20 (guard with
// `has`); both addresses are lowercase 0x. Pure-static.
@id("transfer-to-token-contract")
@severity("warn")
@reason("Recipient is the token's own contract address — these tokens will likely be lost; confirm before sending")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context.token.key has address
  && context.recipient == context.token.key.address
};
