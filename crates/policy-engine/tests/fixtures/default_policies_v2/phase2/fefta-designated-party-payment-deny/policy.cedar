// scopeball JP individual pack — DIRECT-INDIVIDUAL-DUTY.
//
// FEFTA (外国為替及び外国貿易法 / Foreign Exchange and Foreign Trade Act),
// Article 16(1) (法第十六条第1項): the competent minister MAY require a
// *resident* who makes a payment to a non-resident — or, via Cabinet
// sanctions order, to a designated party — to FIRST OBTAIN A LICENCE
// (許可). MOF implements UN/autonomous sanctions by designating individuals
// and entities for ASSET FREEZE (資産凍結) and making any 支払等 (payment)
// to them subject to that licence; an unlicensed payment is an offence.
// This duty binds the PAYER (the居住者), i.e. the self-custody individual
// signing the transfer — NOT a bank or VASP. There is no de-minimis: any
// value to a designated address requires a licence the retail user does
// not hold, so this is a hard BLOCK (a pre-sign licence the user lacks).
//
// On-chain we cannot read the MOF告示 list in Cedar, so the manifest calls
// the external sanctions/asset-freeze registry (external.address_blocklisted,
// list = jp_mof_fefta_asset_freeze) on the transfer `recipient` and projects
// the hit into context.custom.recipientFeftaDesignated. The call is
// `optional` (fail-open until the JP feed ships) and `custom` is optional,
// so the flag is `has`-guarded before reading — absence no-ops, never
// SystemFail.
@id("fefta-designated-party-payment-deny")
@severity("deny")
@reason("This recipient is on Japan's FEFTA/MOF asset-freeze list — paying them without a MOF licence is a sanctions offence")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientFeftaDesignated
  && context.custom.recipientFeftaDesignated
};
