// blocks: a Hyperliquid sendToEvmWithData that carries non-empty (attacker-controllable) calldata.
// why:    fund-exfiltration via arbitrary calldata — sendToEvmWithData bridges a token to an EVM recipient WITH caller-supplied calldata that can re-route or trap the funds at the destination; a plain bridge-out needs no calldata, so any non-"0x" data is the highest-risk pattern and is denied outright. (hl docs — sendToEvmWithData)
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-transfer
//
// Catalog — DENY a Hyperliquid sendToEvmWithData carrying non-empty calldata.
// `data` is a 0x-hex string; an empty payload is exactly "0x". Anything else is
// attacker-controllable calldata forwarded to the recipient — block. Pure-static.
@id("hl-send-to-evm-with-data-deny")
@severity("deny")
@reason("Bridging funds off Hyperliquid with arbitrary calldata — non-empty data is blocked")
forbid(principal, action == HyperliquidCore::Action::"HlSendToEvmWithData", resource)
when {
  context.data != "0x"
};
