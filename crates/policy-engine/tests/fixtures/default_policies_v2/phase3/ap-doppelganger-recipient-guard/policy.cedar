// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// scopeball GOLDEN SAMPLE — ap-doppelganger-recipient-guard
// (demand signal: "address-poisoning-lookalike-recipient" / doppelgänger send).
//
// THREAT (irreversible-mistake class). Address poisoning seeds a wallet's
// transaction history with a decoy whose FIRST and LAST hex chars match a real
// counterparty the user has actually paid before, but whose MIDDLE differs. The
// decoy is introduced by a $0 / dust "poison" transfer — it has never received
// real value from this wallet. Later the user opens their history, copies the
// look-alike (checking only head/tail, as every wallet UI trains them to), and
// sends. The calldata is a perfectly valid ERC20 transfer to a real address;
// nothing is malicious on-chain, so explorers / antivirus / revoke-tools all see
// nothing. The loss is a pure HUMAN copy-paste at the signing moment, and it is
// PERMANENT — the funds are gone. Ground truth: CMU arXiv:2501.16681 measures
// 270M attacks / 17M victims / 6,633 confirmed thefts / >= $83.8M; single
// incidents reach $68M (WBTC) and ~$50M (USDT).
//
// WHY THIS IS NOT a generic "new recipient" warn (which would be high
// false-positive). The DENY here fires ONLY on the deliberate doppelgänger
// pattern, which is the conjunction of THREE host-computed facts:
//   1. recipientLookalike — `recipient` matches a PRIOR recipient of this wallet
//      on prefix AND suffix but differs in the middle (cosmetic twin), AND
//   2. recipientNeverConfirmed — `recipient` has NEVER itself received confirmed
//      value from this wallet (it is a never-used address, not a real repeat), AND
//   3. recipientDustSeeded — `recipient` entered this wallet's history via an
//      inbound zero/dust "poison" transfer (the smoking gun of intent).
// All three together = a seeded imposter of an address you really use. That is
// not a vanity collision; it is an attack. Hence `deny` (hard block), not warn.
//
// The judgment is made HOST-SIDE: `order.recipient_doppelganger_history` reads
// the wallet's per-counterparty send-history / first-seen provenance ledger
// (sim-server / Aggregator state) and projects three scalar Bools into
// `context.custom`. NO send-history state crosses into the stateless policy
// layer — Cedar only reads Bools (precedent: order.recipient_lookalike_history,
// order.swap_count_24h). See manifest `infra_declaration`: the per-wallet
// counterparty ledger with dust-seed provenance is NEW STATE that must ship.
//
// The policy_rpc call is `optional`, so until the provider lands it fail-opens:
// the `has` guards are false and the policy no-ops rather than SystemFail. It
// activates automatically once the per-wallet ledger ships.
@id("ap-doppelganger-recipient-guard")
@severity("deny")
@reason("This recipient is a seeded doppelgänger: it matches the first/last characters of an address you have really paid before, but is itself a never-used address introduced by a zero/dust poison transfer — a permanent-loss address-poisoning trap")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientLookalike
  && context.custom.recipientLookalike
  && context.custom has recipientNeverConfirmed
  && context.custom.recipientNeverConfirmed
  && context.custom has recipientDustSeeded
  && context.custom.recipientDustSeeded
};
