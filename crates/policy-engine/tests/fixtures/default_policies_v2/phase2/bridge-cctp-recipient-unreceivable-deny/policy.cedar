// scopeball default v2 policy — explorer id: BRG-11 (CCTP mintRecipient /
// destinationCaller cannot receive at the destination — self-lock).
//
// CCTP burns USDC at the source and mints to `mintRecipient` at the
// destination. A wrong mintRecipient (malformed address, exchange deposit
// address, or a contract that is not CCTP-aware) means the burn succeeds but the
// mint cannot — funds lock. With no dedicated CCTP decode the `depositForBurn`
// lands as `Core::Unknown`: `mintRecipient` (bytes32), `destinationDomain`, and
// `destinationCaller` are all buried in the `calldata` hex String, and judging
// destination receivability (EOA / CCTP-aware) needs an external check. None of
// this is expressible in Cedar. This is a METHOD:
// `bridge.cctp_recipient_unreceivable` decodes the burn arguments and resolves
// destination receivability, emitting the Bool `cctpRecipientUnreceivable`.
//
// Distinct from BRG-01 (theft redirect: recipient ≠ self): this is a
// self-inflicted lock where the address itself is not malicious but is
// unreceivable. `custom` / field are optional; the call is `optional` and
// fail-opens until the CCTP-decode + receivability surface ships. Deny before
// signing the burn when the recipient is unreceivable.
@id("bridge-cctp-recipient-unreceivable-deny")
@severity("deny")
@reason("CCTP mint recipient cannot receive at the destination — burned USDC would be locked")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has cctpRecipientUnreceivable
  && context.custom.cctpRecipientUnreceivable
};
