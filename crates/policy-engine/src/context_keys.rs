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

// ── Lending action context fields ──
/// Lending market reference.
pub const MARKET: &str = "market";
/// Authorization or revoke target contract reference.
pub const TARGET: &str = "target";
/// Authorization scope discriminator.
pub const AUTHORIZATION_SCOPE: &str = "authorizationScope";
/// Authorizer account address.
pub const AUTHORIZER: &str = "authorizer";
/// Authorized account address.
pub const AUTHORIZED: &str = "authorized";
/// Whether an authorization grants authority.
pub const IS_AUTHORIZED: &str = "isAuthorized";
/// Whether the action originator is the operator on behalf of `onBehalf`.
pub const ON_BEHALF: &str = "onBehalf";
/// Account that funds or supplies the action.
pub const FROM: &str = "from";
/// Liquidation borrower address.
pub const BORROWER: &str = "borrower";
/// Collateral asset record for liquidation.
pub const COLLATERAL_ASSET: &str = "collateralAsset";
/// Debt asset record for liquidation.
pub const DEBT_ASSET: &str = "debtAsset";
/// Debt amount to cover during liquidation.
pub const DEBT_TO_COVER: &str = "debtToCover";
/// Collateral amount to seize during liquidation.
pub const SEIZED_COLLATERAL_AMOUNT: &str = "seizedCollateralAmount";
/// Liquidation mechanism discriminator.
pub const LIQUIDATION_KIND: &str = "liquidationKind";
/// Liquidation input dimension discriminator.
pub const LIQUIDATE_MODE: &str = "liquidateMode";
/// Whether Aave collateral is received as an aToken.
pub const RECEIVE_A_TOKEN: &str = "receiveAToken";
/// Flash loan callback receiver contract.
pub const RECEIVER: &str = "receiver";
/// Flash loan variant discriminator.
pub const FLASH_LOAN_KIND: &str = "flashLoanKind";
/// Flash loan fee amount.
pub const FEE: &str = "fee";
/// Repayment funding source discriminator.
pub const REPAY_KIND: &str = "repayKind";
/// Authorization signature nonce.
pub const NONCE: &str = "nonce";
/// Amount denomination discriminator (assets / shares).
pub const AMOUNT_MODE: &str = "amountMode";
/// Revoke caller address.
pub const CALLER: &str = "caller";
/// Revoke subject address.
pub const SUBJECT: &str = "subject";
/// Revoke variant discriminator.
pub const REVOKE_KIND: &str = "revokeKind";
/// Set of assets borrowed in a flash loan.
pub const ASSETS: &str = "assets";

// ── Staking action context fields ──
/// Asset being staked or restaked.
pub const TOKEN_IN: &str = "tokenIn";
/// Asset withdrawn from staking or restaking.
pub const TOKEN_OUT: &str = "tokenOut";
/// Input amount staked, locked, or burned.
pub const AMOUNT_IN: &str = "amountIn";
/// Expected output amount from staking, unstaking, or restaking.
pub const AMOUNT_OUT: &str = "amountOut";
/// Receipt or share token associated with a staking or restaking action.
pub const RECEIPT_TOKEN: &str = "receiptToken";

// ── Derived action context fields ──
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
