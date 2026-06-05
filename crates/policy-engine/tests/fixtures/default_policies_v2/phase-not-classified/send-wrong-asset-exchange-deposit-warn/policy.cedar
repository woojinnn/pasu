// scopeball default v2 policy — loss-avoidance: an ERC20 transfer to a CEX
// (centralized-exchange) DEPOSIT address that does NOT credit that asset — the
// right network/chain but the WRONG token for that deposit address. The
// exchange may permanently freeze (lose) the funds.
//
// THREAT: a user reuses or mis-copies a deposit address and sends a token the
// exchange does not list / credit at that address. Existing wrong-network
// checks look only at the CHAIN and pass here (chain is correct). The MISSING
// axis is asset-supported-AT-THIS-deposit-address. EVIDENCE: Binance.US /
// Coinbase / Bitget help centers all state that depositing an UNSUPPORTED asset
// is unrecoverable and the exchange bears no responsibility; they run paid
// best-effort "unsupported asset recovery" desks precisely because it is
// frequent. GAP: send-wrong-network-mismatch-deny fires on chain mismatch;
// transfer-recipient-is-contract-warn fires on any contract recipient; neither
// reads (recipient is a CEX deposit address) AND (this exact asset is not
// credited there).
//
// EXPRESSIBILITY: needs-external. external.recipient_exchange_deposit projects
// custom.recipientIsCexDeposit (Bool) + custom.assetSupportedAtDeposit (Bool)
// from an off-chain deposit-address labelling + per-(address,asset) support
// registry. `optional` (FAIL-OPEN): registry unreachable → both absent →
// no-op. WARN never deny: external best-effort data must not hard-block.
@id("send-wrong-asset-exchange-deposit-warn")
@severity("warn")
@reason("This recipient is a centralized-exchange deposit address that does not credit the asset you are sending — exchanges treat unsupported-asset deposits as permanently frozen")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientIsCexDeposit
  && context.custom.recipientIsCexDeposit == true
  && context.custom has assetSupportedAtDeposit
  && context.custom.assetSupportedAtDeposit == false
};
