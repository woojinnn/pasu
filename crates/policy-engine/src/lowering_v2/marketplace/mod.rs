//! Marketplace-domain lowering: per-action dispatch + shared venue / item
//! lowering. Actions carry no live inputs, so contexts are
//! `{ meta, venue, …action fields }` only.

use serde_json::{Map, Value};

use policy_transition::action::marketplace::{
    MarketItem, MarketItemKind, MarketplaceAction, MarketplaceVenue,
};

use super::common::cedar::{addr, u256_hex};
use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod cancel_order;
mod fulfill_order;
mod sign_order;

/// Dispatch a [`MarketplaceAction`] to its per-action lowering.
///
/// # Errors
///
/// Per-action lowerings are infallible today, but the `Result` matches the
/// shared per-action `lower` contract so the dispatch stays uniform.
pub(crate) fn lower(
    action: &MarketplaceAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    match action {
        MarketplaceAction::SignOrder(a) => sign_order::lower(a, ctx),
        MarketplaceAction::FulfillOrder(a) => fulfill_order::lower(a, ctx),
        MarketplaceAction::CancelOrder(a) => cancel_order::lower(a, ctx),
    }
}

/// Lower a [`MarketplaceVenue`] → `{ name, chain, settlement }`.
pub(crate) fn lower_marketplace_venue(venue: &MarketplaceVenue) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name().into()));
    match venue {
        MarketplaceVenue::Seaport { chain, settlement } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("settlement".into(), Value::String(addr(settlement)));
        }
    }
    Value::Object(m)
}

/// Whether a `0x`-hex string is all-zero (`0x000…0`). Used to derive
/// `anyToken` (criteria root) and `usesConduit` (conduit key).
pub(crate) fn is_zero_bytes(hex: &str) -> bool {
    let body = hex.strip_prefix("0x").unwrap_or(hex);
    !body.is_empty() && body.bytes().all(|b| b == b'0')
}

/// Lower a `&[MarketItem]` → a Cedar `Set<MarketItem>` (JSON array of records).
pub(crate) fn lower_market_items(items: &[MarketItem]) -> Value {
    Value::Array(items.iter().map(lower_market_item).collect())
}

fn lower_market_item(item: &MarketItem) -> Value {
    let mut m = Map::new();
    m.insert("idx".into(), Value::from(item.idx));
    m.insert(
        "kind".into(),
        Value::String(market_item_kind(item.kind).into()),
    );
    if let Some(token) = &item.token {
        m.insert("token".into(), Value::String(addr(token)));
    }
    if let Some(token_id) = item.token_id {
        m.insert("tokenId".into(), Value::String(u256_hex(token_id)));
    }
    if let Some(root) = &item.criteria_root {
        m.insert("criteriaRoot".into(), Value::String(root.clone()));
    }
    // `anyToken` (the load-bearing "any NFT in collection" signal) is derived:
    // a criteria-kind item whose criteria root is all-zero.
    let any_token = item.criteria_root.as_deref().is_some_and(|root| {
        matches!(
            item.kind,
            MarketItemKind::Erc721Criteria | MarketItemKind::Erc1155Criteria
        ) && is_zero_bytes(root)
    });
    m.insert("anyToken".into(), Value::Bool(any_token));
    m.insert(
        "startAmount".into(),
        Value::String(u256_hex(item.start_amount)),
    );
    m.insert("endAmount".into(), Value::String(u256_hex(item.end_amount)));
    if let Some(recipient) = &item.recipient {
        m.insert("recipient".into(), Value::String(addr(recipient)));
    }
    Value::Object(m)
}

const fn market_item_kind(kind: MarketItemKind) -> &'static str {
    match kind {
        MarketItemKind::Native => "native",
        MarketItemKind::Erc20 => "erc20",
        MarketItemKind::Erc721 => "erc721",
        MarketItemKind::Erc1155 => "erc1155",
        MarketItemKind::Erc721Criteria => "erc721_criteria",
        MarketItemKind::Erc1155Criteria => "erc1155_criteria",
    }
}

// ---------------------------------------------------------------------------
// Shared test support: sample builders + the conformance-gate helper (mirrors
// `staking::test_support`). Leaf tests build a representative `(body, meta)`
// and pass it to `assert_conforms`, which composes the per-policy schema and
// STRICTLY checks the lowered context against it.
// ---------------------------------------------------------------------------
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod test_support {
    use std::str::FromStr;

    use policy_state::live_field::{DataSource, OracleProvider};
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::LiveField;
    use policy_transition::action::marketplace::{MarketItem, MarketItemKind, MarketplaceVenue};
    use policy_transition::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};

    use crate::lowering_v2::TxMeta;

    pub(crate) const FROM: &str = "0x1111111111111111111111111111111111111111";
    pub(crate) const TO: &str = "0x0000000000000068f116a894984e2db1123eb395";

    pub(crate) fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn oracle_src() -> DataSource {
        DataSource::OracleFeed {
            provider: OracleProvider::Chainlink,
            feed_id: "ETH/USD".into(),
        }
    }

    /// Seaport venue on Ethereum mainnet.
    pub(crate) fn seaport_venue() -> MarketplaceVenue {
        MarketplaceVenue::Seaport {
            chain: ChainId::ethereum_mainnet(),
            settlement: Address::from_str("0x0000000000000068f116a894984e2db1123eb395").unwrap(),
        }
    }

    fn addr_of(hex: &str) -> Address {
        Address::from_str(hex).unwrap()
    }

    /// A concrete ERC-721 offer item (NFT being given).
    pub(crate) fn nft_offer_item() -> MarketItem {
        MarketItem {
            idx: 0,
            kind: MarketItemKind::Erc721,
            token: Some(addr_of("0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d")),
            token_id: Some(U256::from(1234u64)),
            criteria_root: None,
            start_amount: U256::from(1u64),
            end_amount: U256::from(1u64),
            recipient: None,
        }
    }

    /// A criteria ERC-721 offer item (any token in collection).
    pub(crate) fn criteria_offer_item() -> MarketItem {
        MarketItem {
            idx: 0,
            kind: MarketItemKind::Erc721Criteria,
            token: Some(addr_of("0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d")),
            token_id: None,
            criteria_root: Some(
                "0x0000000000000000000000000000000000000000000000000000000000000000".into(),
            ),
            start_amount: U256::from(1u64),
            end_amount: U256::from(1u64),
            recipient: None,
        }
    }

    /// A native-ETH consideration item paid to a recipient.
    pub(crate) fn native_consideration_item(idx: u32, recipient: &str) -> MarketItem {
        MarketItem {
            idx,
            kind: MarketItemKind::Native,
            token: None,
            token_id: None,
            criteria_root: None,
            start_amount: U256::from(1_000_000_000_000_000_000u64),
            end_amount: U256::from(1_000_000_000_000_000_000u64),
            recipient: Some(addr_of(recipient)),
        }
    }

    pub(crate) fn submitter_addr() -> Address {
        addr_of("0x1111111111111111111111111111111111111111")
    }

    /// An on-chain-transaction `ActionMeta` (Ethereum mainnet).
    pub(crate) fn onchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: submitter_addr(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 3,
                gas_limit: U256::from(200_000u64),
                gas_price: LiveField::new(U256::from(20_000_000_000u64), oracle_src(), now()),
                value: U256::ZERO,
            },
        }
    }

    /// An off-chain-signature `ActionMeta` (Seaport order).
    pub(crate) fn offchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: submitter_addr(),
            nature: ActionNature::OffchainSig {
                domain: Eip712Domain {
                    name: "Seaport".into(),
                    version: Some("1.6".into()),
                    chain_id: Some(1),
                    verifying_contract: Some(
                        Address::from_str("0x0000000000000068f116a894984e2db1123eb395").unwrap(),
                    ),
                    salt: None,
                },
                deadline: Time::from_unix(1_738_100_000),
                nonce_key: None,
            },
        }
    }

    /// THE GATE: compose the per-policy schema for `tag`, lower `body`/`meta`,
    /// and STRICTLY construct the Cedar context against the schema. A wrong
    /// rename / missing required field / wrong type ERRORS here.
    pub(crate) fn assert_conforms(tag: &str, body: &ActionBody, meta: &ActionMeta) {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": format!("{}-schema", tag),
            "schema_version": 2,
            "trigger": { "where": { "action.tag": { "eq": tag } } }
        }))
        .unwrap();
        let schema_text = crate::schema::compose_per_policy(&manifest).unwrap();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let lowered =
            crate::lowering_v2::lower_action(body, meta, &TxMeta { from: FROM, to: TO }).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();
        cedar_policy::Context::from_json_value(lowered.context, Some((&schema, &uid)))
            .unwrap_or_else(|e| panic!("{tag} context must conform: {e:?}"));
    }
}
