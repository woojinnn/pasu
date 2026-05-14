//! Cedar policy schema composition.

const CORE_SCHEMA: &str = include_str!("../../../policy-schema/core.cedarschema");
const DEX_ADD_LIQUIDITY_SCHEMA: &str =
    include_str!("../../../policy-schema/actions/DEX/add_liquidity.cedarschema");
const DEX_BURN_LIQUIDITY_NFT_SCHEMA: &str =
    include_str!("../../../policy-schema/actions/DEX/burn_liquidity_nft.cedarschema");
const DEX_DECREASE_LIQUIDITY_SCHEMA: &str =
    include_str!("../../../policy-schema/actions/DEX/decrease_liquidity.cedarschema");
const DEX_INCREASE_LIQUIDITY_SCHEMA: &str =
    include_str!("../../../policy-schema/actions/DEX/increase_liquidity.cedarschema");
const DEX_MINT_LIQUIDITY_NFT_SCHEMA: &str =
    include_str!("../../../policy-schema/actions/DEX/mint_liquidity_nft.cedarschema");
const DEX_REMOVE_LIQUIDITY_SCHEMA: &str =
    include_str!("../../../policy-schema/actions/DEX/remove_liquidity.cedarschema");
const DEX_SWAP_SCHEMA: &str = include_str!("../../../policy-schema/actions/DEX/swap.cedarschema");

/// Composes the shipped core and action Cedar schemas.
#[derive(Debug, Default, Clone)]
pub struct PolicySchemaComposer;

impl PolicySchemaComposer {
    /// Construct a schema composer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Return the concatenated Cedar schema text.
    #[must_use]
    pub fn compose(&self) -> String {
        [
            CORE_SCHEMA,
            DEX_ADD_LIQUIDITY_SCHEMA,
            DEX_BURN_LIQUIDITY_NFT_SCHEMA,
            DEX_DECREASE_LIQUIDITY_SCHEMA,
            DEX_INCREASE_LIQUIDITY_SCHEMA,
            DEX_MINT_LIQUIDITY_NFT_SCHEMA,
            DEX_REMOVE_LIQUIDITY_SCHEMA,
            DEX_SWAP_SCHEMA,
        ]
        .join("\n")
    }
}
