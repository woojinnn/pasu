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

// Dex action context fields stamped by `lowering::request_from_action`.
/// Protocol ids observed in the aggregate DEX action.
pub const PROTOCOL_IDS: &str = "protocolIds";
/// Tokens the aggregate DEX action spends.
pub const INPUT_TOKENS: &str = "inputTokens";
/// Tokens the aggregate DEX action expects to receive.
pub const OUTPUT_TOKENS: &str = "outputTokens";
/// Total USD value of input requirements.
pub const TOTAL_INPUT_USD: &str = "totalInputUsd";
/// Total USD value of minimum output requirements.
pub const TOTAL_MIN_OUTPUT_USD: &str = "totalMinOutputUsd";
/// Highest pool or route fee in basis points.
pub const MAX_FEE_BPS: &str = "maxFeeBps";
/// Whether any swap leg has a zero minimum output.
pub const HAS_ZERO_MIN_OUTPUT: &str = "hasZeroMinOutput";
/// Whether any recipient differs from the actor.
pub const HAS_EXTERNAL_RECIPIENT: &str = "hasExternalRecipient";
/// Input size as basis points of the actor portfolio.
pub const TOTAL_INPUT_FRACTION_OF_PORTFOLIO_BPS: &str = "totalInputFractionOfPortfolioBps";
/// Whether current allowances cover all ERC-20 inputs.
pub const ALLOWANCES_COVER_INPUTS: &str = "allowancesCoverInputs";
/// Aggregate stat-window context object.
pub const WINDOW_STATS: &str = "windowStats";

// AmountSpec sub-record fields.
/// Token address field.
pub const ADDRESS: &str = "address";
/// Token symbol field.
pub const SYMBOL: &str = "symbol";
/// Token decimal precision field.
pub const DECIMALS: &str = "decimals";
/// Native-asset marker field.
pub const IS_NATIVE: &str = "isNative";
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

// Stat-window keys consumed by lowering and stamped onto `windowStats`.
/// Rolling 24-hour swap volume in USD.
pub const SWAP_VOLUME_USD_24H: &str = "swapVolumeUsd24h";
/// Rolling 24-hour swap count.
pub const SWAP_COUNT_24H: &str = "swapCount24h";
