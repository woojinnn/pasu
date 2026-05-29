//! Token-domain lowering: exhaustive per-action dispatch.
//!
//! Each [`TokenAction`] variant lowers to its `Token::*Context` shape via the
//! matching leaf module. Token actions never carry a venue and share no
//! sub-lowering across leaves, so this dispatch is a flat exhaustive match with
//! no domain-local helper (the `RevokeScope` lowering is single-use and lives
//! private in the `revoke_approval` leaf).

use simulation_reducer::action::token::TokenAction;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod erc20_approve;
mod erc20_permit;
mod erc20_transfer;
mod nft_approve;
mod nft_set_approval_for_all;
mod nft_transfer;
mod permit2_approve;
mod permit2_sign_allowance;
mod revoke_approval;

/// Dispatch a [`TokenAction`] to its per-action lowering.
///
/// # Errors
///
/// Each leaf lowering is infallible today, but the `Result` matches the shared
/// per-action contract so the fan-out stays uniform across domains.
pub(crate) fn lower(action: &TokenAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    match action {
        TokenAction::Erc20Approve(a) => erc20_approve::lower(a, ctx),
        TokenAction::Erc20Permit(a) => erc20_permit::lower(a, ctx),
        TokenAction::Permit2Approve(a) => permit2_approve::lower(a, ctx),
        TokenAction::Permit2SignAllowance(a) => permit2_sign_allowance::lower(a, ctx),
        TokenAction::Erc20Transfer(a) => erc20_transfer::lower(a, ctx),
        TokenAction::NftApprove(a) => nft_approve::lower(a, ctx),
        TokenAction::NftSetApprovalForAll(a) => nft_set_approval_for_all::lower(a, ctx),
        TokenAction::NftTransfer(a) => nft_transfer::lower(a, ctx),
        TokenAction::RevokeApproval(a) => revoke_approval::lower(a, ctx),
    }
}

/// Shared sample builders + the conformance-gate helper used by every token
/// leaf's `#[cfg(test)] mod tests`.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod test_support {
    use std::str::FromStr;

    use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};
    use simulation_state::live_field::{DataSource, OracleProvider};
    use simulation_state::primitives::{Address, ChainId, Time, U256};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::{LiveField, NonceKey};

    use crate::lowering_v2::{lower_action, TxMeta};

    pub(crate) const FROM: &str = "0x1111111111111111111111111111111111111111";
    pub(crate) const TO: &str = "0x2222222222222222222222222222222222222222";

    /// Submission / signing timestamp shared by every sample.
    pub(crate) fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    /// The acting wallet (matches the `submitter` in the meta samples).
    pub(crate) fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    /// A `spender` address (the party being granted/revoked an allowance).
    pub(crate) fn spender() -> Address {
        Address::from_str("0x000000000022d473030f116ddee9f6b43ac78ba3").unwrap()
    }

    /// A `recipient` address (transfer destination).
    pub(crate) fn recipient() -> Address {
        Address::from_str("0x00000000000000000000000000000000000ce110").unwrap()
    }

    /// An NFT / collection contract address.
    pub(crate) fn nft_contract() -> Address {
        Address::from_str("0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d").unwrap()
    }

    /// A representative ERC20 `TokenRef` (USDC on Ethereum mainnet).
    pub(crate) fn sample_erc20_token() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    /// A representative ERC721 `TokenKey` (a specific token id in a collection).
    pub(crate) fn sample_nft_key() -> TokenKey {
        TokenKey::Erc721 {
            chain: ChainId::ethereum_mainnet(),
            contract: nft_contract(),
            token_id: U256::from(7777u64),
        }
    }

    /// Wrap a `U256` in a `LiveField` (the on-chain-view source shape used by
    /// `Erc20Permit`'s `nonce`).
    pub(crate) fn live_u256(value: U256) -> LiveField<U256> {
        LiveField::new(value, sample_source(), now())
    }

    /// Wrap a `(word, bit)` `Permit2` nonce coordinate in a `LiveField`.
    pub(crate) fn live_nonce_pair(word: U256, bit: u8) -> LiveField<(U256, u8)> {
        LiveField::new((word, bit), sample_source(), now())
    }

    /// A shared `DataSource` for the `LiveField` samples.
    fn sample_source() -> DataSource {
        DataSource::OnchainView {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            function: "nonces(address)".into(),
            decoder_id: "erc2612_nonce".into(),
        }
    }

    /// An on-chain `ActionMeta` (the common case: `approve` / `transfer`).
    pub(crate) fn onchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 42,
                gas_limit: U256::from(120_000u64),
                gas_price: LiveField::new(
                    U256::from(100_000_000u64),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Pyth,
                        feed_id: "ETH/USD".into(),
                    },
                    now(),
                ),
                value: U256::ZERO,
            },
        }
    }

    /// An off-chain-signature `ActionMeta` (used by `Erc20Permit` and
    /// `Permit2SignAllowance`).
    pub(crate) fn offchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OffchainSig {
                domain: Eip712Domain {
                    name: "Permit2".into(),
                    version: Some("1".into()),
                    chain_id: Some(1),
                    verifying_contract: Some(
                        Address::from_str("0x000000000022d473030f116ddee9f6b43ac78ba3").unwrap(),
                    ),
                    salt: None,
                },
                deadline: Time::from_unix(1_738_001_800),
                nonce_key: Some(NonceKey::OrderHash {
                    hash: "0xabc0000000000000000000000000000000000000000000000000000000000000"
                        .into(),
                }),
            },
        }
    }

    /// THE GATE: synthesize the per-policy schema for `tag`, lower the sample,
    /// and strictly construct the Cedar context against it. A wrong rename, a
    /// missing required field, a wrong type, or an over-emitted host-only field
    /// makes `Context::from_json_value` ERROR here.
    pub(crate) fn assert_conforms(tag: &str, body: &ActionBody, meta: &ActionMeta) {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": format!("{}-schema", tag),
            "schema_version": 2,
            "trigger": { "where": { "action.tag": { "eq": tag } } }
        }))
        .unwrap();
        let schema_text = crate::schema::compose_per_policy(&manifest).unwrap();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let lowered = lower_action(body, meta, &TxMeta { from: FROM, to: TO }).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();
        cedar_policy::Context::from_json_value(lowered.context.clone(), Some((&schema, &uid)))
            .unwrap_or_else(|e| panic!("{tag} context must conform: {e:?}"));
    }
}
