// scopeball policy — Send Safety Set: wrong-network / chain-mismatch send (EOA
// send-side, ERC20 transfer).
//
// Sending an ERC20 transfer whose (connected chain_id, recipient, token) combo
// is incompatible — the recipient cannot receive this asset on this network —
// is a near-certain PERMANENT LOSS. Demand evidence is strong: Binance,
// MetaMask, CoinJar, Blockchain.com and Ledger all run standing "sent to the
// wrong network" recovery pages and state recovery is impossible in almost all
// cases. Funds reach an address that cannot acknowledge them and are gone.
//
// The verdict is `is_network_mismatch(chain_id, recipient, token)`. That needs
// knowledge of the recipient's address-type / network compatibility and the
// token's deployment chain — none of which live in wallet state or on the
// action context. So the comparison is made off-Cedar (host-side context,
// External Fact Lookup): the manifest's `external.network_mismatch_loss`
// policy_rpc call projects the verdict into `context.custom.networkMismatch`.
//
// `custom` is optional, so the guard `has`-checks the path; the value is a
// `Bool`. The call is `optional` and the policy fail-opens until the external
// provider ships. Scoped to `Token::Action::"Erc20Transfer"` (action.tag =
// erc20_transfer) — the direct EOA send path.
@id("send-wrong-network-mismatch-deny")
@severity("deny")
@reason("This send targets a network that does not match the destination address or asset — funds sent this way are almost always permanently lost")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has networkMismatch
  && context.custom.networkMismatch
};
