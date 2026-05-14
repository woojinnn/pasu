//! Cedar context-field names produced by lowering.
//!
//! Centralizing these keys in one module catches typos in lowering
//! sites at compile time (`use context_keys::CHAIN_ID;` instead of
//! `"chainId"` strewn across files). Cedar policies still author the
//! string literals — they parse keys at policy-load time, so the
//! contract is "the string literal in the .cedar file matches the
//! string value of the constant here".
//!
//! When adding a new context field: declare the constant here, use
//! it from lowering, document it in the relevant policy authoring
//! reference, and reference the new constant by name from any
//! integration test that asserts on the field shape.

// Common transaction/action fields stamped by request lowering.
/// EVM chain id for the evaluated transaction.
pub const CHAIN_ID: &str = "chainId";
/// Sender wallet address.
pub const FROM: &str = "from";
/// Target contract address.
pub const TO: &str = "to";
/// Native value attached to the transaction, in wei.
pub const VALUE_WEI: &str = "valueWei";
/// Four-byte function selector as hex.
pub const SELECTOR: &str = "selector";
/// Full calldata as hex.
pub const RAW_CALLDATA: &str = "rawCalldata";
/// Semantic action target address.
pub const TARGET: &str = "target";

// Dex action context fields stamped by envelope lowering.
/// Protocol ids observed in the aggregate DEX action.
pub const PROTOCOL_IDS: &str = "protocolIds";
/// Assets deposited or spent by a DEX action.
pub const INPUTS: &str = "inputs";
/// Assets received or withdrawn by a DEX action.
pub const OUTPUTS: &str = "outputs";
/// Tokens the aggregate DEX action spends.
pub const INPUT_TOKENS: &str = "inputTokens";
/// Tokens the aggregate DEX action expects to receive.
pub const OUTPUT_TOKENS: &str = "outputTokens";
/// LP token minted or burned by a liquidity action.
pub const LP_TOKEN: &str = "lpToken";
/// LP amount constraint.
pub const LP_AMOUNT: &str = "lpAmount";
/// LP burn amount constraint.
pub const LP_BURN_AMOUNT: &str = "lpBurnAmount";
/// Fungible liquidity exit mode.
pub const EXIT_MODE: &str = "exitMode";
/// Concentrated liquidity NFT burn kind.
pub const BURN_KIND: &str = "burnKind";
/// Internal concentrated-liquidity amount delta.
pub const LIQUIDITY_DELTA: &str = "liquidityDelta";
/// Pool fee tier in basis points.
pub const FEE_TIER_BPS: &str = "feeTierBps";
/// Swap or pool fee in basis points.
pub const FEE_BPS: &str = "feeBps";
/// Concentrated liquidity tick range.
pub const TICK_RANGE: &str = "tickRange";
/// Lower tick bound.
pub const LOWER: &str = "lower";
/// Upper tick bound.
pub const UPPER: &str = "upper";
/// DEX pool reference.
pub const POOL: &str = "pool";
/// Position NFT reference.
pub const NFT: &str = "nft";
/// DEX action recipient.
pub const RECIPIENT: &str = "recipient";
/// Total USD value of input requirements.
pub const TOTAL_INPUT_USD: &str = "totalInputUsd";
/// Total USD value of minimum output requirements.
pub const TOTAL_MIN_OUTPUT_USD: &str = "totalMinOutputUsd";
/// Total USD value of output requirements.
pub const TOTAL_OUTPUT_USD: &str = "totalOutputUsd";
/// Input amount as basis points of portfolio value.
pub const TOTAL_INPUT_FRACTION_OF_PORTFOLIO_BPS: &str = "totalInputFractionOfPortfolioBps";
/// Highest pool or route fee in basis points.
pub const MAX_FEE_BPS: &str = "maxFeeBps";
/// Whether any swap leg has a zero minimum output.
pub const HAS_ZERO_MIN_OUTPUT: &str = "hasZeroMinOutput";
/// Whether any recipient differs from the actor.
pub const HAS_EXTERNAL_RECIPIENT: &str = "hasExternalRecipient";
/// Whether any allowance grants an unlimited spend.
pub const HAS_UNLIMITED_ALLOWANCE: &str = "hasUnlimitedAllowance";
/// Whether the recipient address is a contract.
pub const RECIPIENT_IS_CONTRACT: &str = "recipientIsContract";
/// Swap effective rate versus oracle price, in basis points.
pub const EFFECTIVE_RATE_VS_ORACLE_BPS: &str = "effectiveRateVsOracleBps";
/// Whether the current actor owns the position NFT.
pub const NFT_OWNER_IS_ACTOR: &str = "nftOwnerIsActor";
/// Rolling-window statistics for the action.
pub const WINDOW_STATS: &str = "windowStats";
/// Validity deadline delta in seconds.
pub const VALIDITY_DELTA_SEC: &str = "validityDeltaSec";

// Signature action context fields stamped by signature lowering.
/// Shared signature base context record.
pub const BASE: &str = "base";
/// Signature signer address.
pub const SIGNER: &str = "signer";
/// Chain id supplied by the wallet request.
pub const REQUEST_CHAIN_ID: &str = "requestChainId";
/// Chain id embedded in the EIP-712 domain.
pub const DOMAIN_CHAIN_ID: &str = "domainChainId";
/// EIP-712 verifying contract.
pub const VERIFYING_CONTRACT: &str = "verifyingContract";
/// EIP-712 primary type.
pub const PRIMARY_TYPE: &str = "primaryType";
/// Host clock timestamp used for deadline deltas.
pub const NOW_TS: &str = "nowTs";
/// Permit2 permit kind.
pub const PERMIT_KIND: &str = "permitKind";
/// Approval spender address.
pub const SPENDER: &str = "spender";
/// Signature token field.
pub const TOKEN: &str = "token";
/// Human Permit2 amount.
pub const AMOUNT_HUMAN: &str = "amountHuman";
/// Permit2 approval expiration.
pub const EXPIRATION: &str = "expiration";
/// Permit2 signature deadline.
pub const SIG_DEADLINE: &str = "sigDeadline";
/// Permit2 signature-deadline delta in seconds.
pub const SIG_DEADLINE_DELTA_SEC: &str = "sigDeadlineDeltaSec";
/// Signature nonce.
pub const NONCE: &str = "nonce";
/// Permit2 approval count.
pub const APPROVAL_COUNT: &str = "approvalCount";
/// Structural nonce sanity flag.
pub const NONCE_VALID: &str = "nonceValid";
/// Unlimited approval marker.
pub const IS_UNLIMITED: &str = "isUnlimited";
/// Permit2 witness payload marker.
pub const WITNESS_PRESENT: &str = "witnessPresent";
/// Whether Permit2 human amount was clamped at Cedar's decimal ceiling.
pub const AMOUNT_HUMAN_CLAMPED_AT_CEILING: &str = "amountHumanClampedAtCeiling";
/// Total approved USD valuation.
pub const TOTAL_APPROVED_USD: &str = "totalApprovedUsd";
/// EIP-2612 owner address.
pub const OWNER: &str = "owner";
/// Human EIP-2612 value.
pub const VALUE_HUMAN: &str = "valueHuman";
/// EIP-2612 deadline.
pub const DEADLINE: &str = "deadline";
/// EIP-2612 deadline delta in seconds.
pub const DEADLINE_DELTA_SEC: &str = "deadlineDeltaSec";
/// Whether EIP-2612 human value was clamped at Cedar's decimal ceiling.
pub const VALUE_HUMAN_CLAMPED_AT_CEILING: &str = "valueHumanClampedAtCeiling";
/// EIP-712 domain name.
pub const DOMAIN_NAME: &str = "domainName";
/// EIP-712 domain version.
pub const DOMAIN_VERSION: &str = "domainVersion";
/// EIP-712 domain salt.
pub const DOMAIN_SALT: &str = "domainSalt";
/// EIP-712 type map JSON.
pub const TYPES_JSON: &str = "typesJson";
/// EIP-712 message JSON.
pub const MESSAGE_JSON: &str = "messageJson";

// AmountSpec sub-record fields.
/// Token address field.
pub const ADDRESS: &str = "address";
/// Protocol-specific id field.
pub const ID: &str = "id";
/// Human-readable label field.
pub const LABEL: &str = "label";
/// Nested asset field.
pub const ASSET: &str = "asset";
/// Nested amount field.
pub const AMOUNT: &str = "amount";
/// Token id field.
pub const TOKEN_ID: &str = "tokenId";
/// Token symbol field.
pub const SYMBOL: &str = "symbol";
/// Token decimal precision field.
pub const DECIMALS: &str = "decimals";
/// Raw integer amount field.
pub const RAW: &str = "raw";
/// Human decimal amount field.
pub const HUMAN: &str = "human";
/// USD valuation sub-record field.
pub const USD: &str = "usd";
/// Decimal value field.
pub const VALUE: &str = "value";
/// Oracle valuation timestamp field.
pub const AS_OF_TS: &str = "asOfTs";
/// Oracle staleness field.
pub const STALE_SEC: &str = "staleSec";
/// Oracle source list field.
pub const SOURCES: &str = "sources";

// Cedar `__extn` extension call shape used to embed Decimal values.
/// Cedar extension object key.
pub const EXTN_KEY: &str = "__extn";
/// Cedar extension function name key.
pub const EXTN_FN: &str = "fn";
/// Cedar extension argument key.
pub const EXTN_ARG: &str = "arg";
/// Cedar Decimal extension function name.
pub const EXTN_DECIMAL: &str = "decimal";

// Stat-window keys consumed by lowering.
/// Rolling 24-hour swap volume in USD.
pub const SWAP_VOLUME_USD_24H: &str = "swapVolumeUsd24h";
/// Rolling 24-hour swap count.
pub const SWAP_COUNT_24H: &str = "swapCount24h";
