//! Cedar context-field names produced by lowering.
//!
//! Centralizing these keys catches typos in lowering at compile time
//! (`use context_keys::ADDRESS;` instead of `"address"` strewn across files).
//! Cedar policies still author the string literals — they parse keys at
//! policy-load time, so the contract is "the string literal in the .cedar
//! file matches the string value of the constant here".
//!
//! When adding a new context field: declare the constant here, use it from
//! lowering, and reference the new constant by name from any integration
//! test that asserts on the field shape.

// ── Shared sub-record fields (AssetRef / AssetRefWithAmount / PoolRef) ──
/// Token contract address field.
pub const ADDRESS: &str = "address";
/// Token symbol field.
pub const SYMBOL: &str = "symbol";
/// Token decimal precision field.
pub const DECIMALS: &str = "decimals";
/// Non-fungible token id field.
pub const TOKEN_ID: &str = "tokenId";
/// `AssetRefWithAmount.asset` sub-record field.
pub const ASSET: &str = "asset";
/// Amount sub-record field.
pub const AMOUNT: &str = "amount";
/// Pool / market identifier (bytes32) field.
pub const ID: &str = "id";
/// Human-readable pool / strategy / contract label field.
pub const LABEL: &str = "label";

// ── DEX action context fields ──
/// Assets deposited or spent by a DEX action.
pub const INPUTS: &str = "inputs";
/// Assets received or withdrawn by a DEX action.
pub const OUTPUTS: &str = "outputs";
/// LP token minted or burned by a liquidity action.
pub const LP_TOKEN: &str = "lpToken";
/// LP amount constraint.
pub const LP_AMOUNT: &str = "lpAmount";
/// LP burn amount constraint.
pub const LP_BURN_AMOUNT: &str = "lpBurnAmount";
/// Fungible liquidity exit mode discriminator.
pub const EXIT_MODE: &str = "exitMode";
/// Concentrated-liquidity NFT burn kind discriminator.
pub const BURN_KIND: &str = "burnKind";
/// Internal liquidity delta for NFT decrease actions.
pub const LIQUIDITY_DELTA: &str = "liquidityDelta";
/// Concentrated-liquidity pool fee tier in basis points.
pub const FEE_TIER_BPS: &str = "feeTierBps";
/// Per-swap pool fee in basis points.
pub const FEE_BPS: &str = "feeBps";
/// Concentrated-liquidity tick range record.
pub const TICK_RANGE: &str = "tickRange";
/// Lower tick bound.
pub const LOWER: &str = "lower";
/// Upper tick bound.
pub const UPPER: &str = "upper";
/// Pool reference record.
pub const POOL: &str = "pool";
/// NFT collection reference for liquidity-position actions.
pub const NFT: &str = "nft";
/// Recipient address field.
pub const RECIPIENT: &str = "recipient";

// ── Host-derived enrichment fields ──
/// Total USD value of input requirements.
pub const TOTAL_INPUT_USD: &str = "totalInputUsd";
/// Total USD value of minimum output requirements.
pub const TOTAL_MIN_OUTPUT_USD: &str = "totalMinOutputUsd";
/// Input value as basis points of the actor's portfolio.
pub const TOTAL_INPUT_FRACTION_OF_PORTFOLIO_BPS: &str = "totalInputFractionOfPortfolioBps";
/// Validity-window delta from `block_timestamp` in seconds.
pub const VALIDITY_DELTA_SEC: &str = "validityDeltaSec";

// ── UsdValuation sub-record fields ──
/// Decimal value field.
pub const VALUE: &str = "value";
/// Oracle valuation timestamp field.
pub const AS_OF_TS: &str = "asOfTs";
/// Oracle staleness field in seconds.
pub const STALE_SEC: &str = "staleSec";
/// Oracle source list field.
pub const SOURCES: &str = "sources";

// ── Cedar `__extn` extension call shape used to embed Decimal values ──
/// Cedar extension object key.
pub const EXTN_KEY: &str = "__extn";
/// Cedar extension function name key.
pub const EXTN_FN: &str = "fn";
/// Cedar extension argument key.
pub const EXTN_ARG: &str = "arg";
/// Cedar Decimal extension function name.
pub const EXTN_DECIMAL: &str = "decimal";

// ── Stat-window keys consumed by host stat plumbing ──
/// Rolling 24-hour swap volume in USD.
pub const SWAP_VOLUME_USD_24H: &str = "swapVolumeUsd24h";
/// Rolling 24-hour swap count.
pub const SWAP_COUNT_24H: &str = "swapCount24h";
