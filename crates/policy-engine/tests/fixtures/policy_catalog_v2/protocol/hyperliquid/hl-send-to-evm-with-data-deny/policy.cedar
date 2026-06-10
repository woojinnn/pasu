// blocks: a Hyperliquid sendToEvmWithData that carries non-empty (attacker-controllable) calldata.
// why:    fund-exfiltration via arbitrary calldata — sendToEvmWithData bridges a token to an EVM recipient WITH caller-supplied calldata that can re-route or trap the funds at the destination; a plain bridge-out needs no calldata, so any non-"0x" data is the highest-risk pattern and is denied outright. (hl docs — sendToEvmWithData)
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-transfer
//
// Catalog — DENY a Hyperliquid sendToEvmWithData carrying non-empty calldata.
// sendToEvmWithData now lowers to the generic Core::Unknown, preserving the
// risk-bearing payload as `context.calldata` (= the wire `data`); an empty
// payload is exactly "0x". Anything else is attacker-controllable calldata
// forwarded to the recipient — block. The `hl_send_to_evm_with_data` manifest
// trigger keeps this scoped to HL bridge-outs. Pure-static.
@id("hl-send-to-evm-with-data-deny")
@severity("deny")
@reason("Bridging funds off Hyperliquid with arbitrary calldata — non-empty data is blocked")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context.calldata != "0x"
};
