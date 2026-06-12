//! Defines where a `LiveField` comes from.
//!
//! `DataSource` records how the sync layer refreshes the value stored inside a
//! `LiveField`.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Address, ChainId};

/// Oracle price-feed provider.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum OracleProvider {
    /// Pyth Network oracle.
    Pyth,
    /// Chainlink oracle.
    Chainlink,
    /// `RedStone` oracle.
    RedStone,
    /// Any other provider, preserved by name only.
    Other(String),
}

/// Authentication scheme used when calling an external API.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthSpec {
    /// No authentication required.
    None,
    /// Bearer-token auth; the token is read from this environment variable.
    Bearer {
        /// Name of the env var holding the bearer token.
        token_env: String,
    },
    /// HMAC-signature auth; the signing key is read from this environment variable.
    HmacSig {
        /// Name of the env var holding the HMAC signing key.
        key_env: String,
    },
    /// Custom auth scheme, identified by an opaque string.
    Custom(String),
}

/// A data source the sync orchestrator uses to populate a `LiveField`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DataSource {
    /// An on-chain view (read-only) function call, e.g. via `eth_call`.
    OnchainView {
        /// Chain the contract lives on.
        chain: ChainId,
        /// Address of the contract to call.
        #[tsify(type = "string")]
        contract: Address,
        /// Name of the view function to invoke.
        function: String,
        /// Identifier of the decoder (in an external registry) used to decode the result.
        decoder_id: String,
    },

    /// A standard oracle price feed.
    OracleFeed {
        /// Oracle provider serving the feed.
        provider: OracleProvider,
        /// Provider-specific feed identifier.
        feed_id: String,
    },

    /// A REST/WebSocket venue API (e.g. Hyperliquid, GMX subgraph, dYdX indexer).
    VenueApi {
        /// Endpoint URL of the venue API.
        endpoint: String,
        /// Identifier of the parser (in an external registry) used to interpret the response.
        parser_id: String,
        /// Optional authentication scheme for the endpoint.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[tsify(optional)]
        auth: Option<AuthSpec>,
    },

    /// A value computed from other `LiveField`s; a reducer updates it in place.
    DerivedFrom {
        /// References to the `LiveField`s this value is computed from.
        inputs: Vec<FieldRef>,
        /// Identifier of the calculation (in an external registry) to run over the inputs.
        calc_id: String,
    },

    /// The dambi registry server: a provider of static metadata such as
    /// token classification, protocol mapping, and decoders. Unlike an oracle,
    /// it tells you "what this is" rather than a price, and its cache policy is
    /// very long (24h+).
    RegistryApi {
        /// Endpoint URL of the registry server.
        endpoint: String,
        /// Specific registry resource being requested.
        resource: RegistryResource,
        /// Optional version to pin against, used when the registry schema changes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<String>,
    },

    /// A value supplied directly by the user (e.g. a manual override).
    UserSupplied,
}

/// Kind of resource to request from the registry server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RegistryResource {
    /// Token classification: fetches kind / symbol / decimals.
    TokenMeta {
        /// Chain the token lives on.
        chain: ChainId,
        /// Token contract address.
        address: Address,
    },
    /// Which protocol and which component of it the contract belongs to.
    ProtocolMap {
        /// Chain the contract lives on.
        chain: ChainId,
        /// Contract address to map.
        address: Address,
    },
    /// Pool metadata: fee tier, underlyings, and so on.
    PoolMeta {
        /// Chain the pool lives on.
        chain: ChainId,
        /// Pool contract address.
        pool_addr: Address,
    },
    /// Mapping from a 4-byte selector to its ABI / function decoder.
    DecoderRegistry,
}

/// A reference to another `LiveField`, used in `DataSource::DerivedFrom` inputs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum FieldRef {
    /// A field on a specific token.
    TokenField {
        /// The `TokenKey` serialized to a JSON string. Carried as a string to
        /// avoid a circular dependency: since the `LiveField` is embedded in the
        /// token, importing `TokenKey` directly would risk a module cycle.
        token_key_json: String,
        /// Which token field is referenced.
        field: TokenFieldName,
    },
    /// A field on a specific position.
    PositionField {
        /// Identifier of the position.
        position_id: String,
        /// Which position field is referenced.
        field: PositionFieldName,
    },
    /// A field on a specific pending action.
    PendingField {
        /// Identifier of the pending action.
        pending_id: String,
        /// Which pending field is referenced.
        field: PendingFieldName,
    },
    /// A global value independent of any wallet/position, e.g. `gas_price`, `eth_usd`.
    Global {
        /// Name of the global value.
        name: String,
    },
}

/// Referenceable live fields on a token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum TokenFieldName {
    /// Token price in USD.
    PriceUsd,
}

/// Referenceable live fields on a position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PositionFieldName {
    /// Position health factor.
    HealthFactor,
    /// Loan-to-value ratio.
    Ltv,
    /// Liquidation threshold.
    LiquidationThreshold,
    /// Current mark price.
    MarkPrice,
    /// Price at which the position would be liquidated.
    LiqPrice,
    /// Unrealized profit and loss.
    UnrealizedPnl,
    /// Funding currently owed on the position.
    FundingOwed,
    /// Position leverage.
    Leverage,
}

/// Referenceable live fields on a pending action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PendingFieldName {
    /// Status of the pending action.
    Status,
    /// Fill progress, e.g. partial-fill ratio.
    FillRatio,
}
