// dambi default v2 policy — explorer id: BRG-09 (intent-bridge output is
// haircut by an out-of-band relayer fee).
//
// On intent bridges (Across) the received amount is set not by slippage but by
// the relayer's fee — the input−output spread. When that fee exceeds the usual
// band (high pool utilization, repayment-chain, exclusivity) the user should be
// warned before signing; it also catches the variant where the UI-shown fee and
// the calldata `outputAmount` disagree. With no dedicated bridge decode the
// deposit lands as `Core::Unknown`: `inputAmount` / `outputAmount` are buried in
// the `calldata` hex String and the fee is `(input − output) / input` against a
// learned fee distribution — neither extractable nor computable in Cedar. This
// is a METHOD: `bridge.relayer_fee_bp` decodes the deposit amounts and returns
// the relayer fee in basis points (Long).
//
// Distinct from BRG-05 (AMM-style slippage floor): this compares a negotiated
// fee spread against the usual band. `custom` / field are optional; the call is
// `optional` and fail-opens until the fact host ships. The band (100 bp = 1%) is
// config inlined for this slice. Warn when the relayer fee exceeds the band.
@id("bridge-relayer-fee-band-warn")
@severity("warn")
@reason("Intent-bridge relayer fee is above the usual band — this transfer costs more than normal")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has relayerFeeBp
  && context.custom.relayerFeeBp > 100
};
