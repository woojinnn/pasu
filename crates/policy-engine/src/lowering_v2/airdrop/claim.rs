//! `Airdrop::Claim` lowering → `Airdrop::ClaimContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::airdrop::{ClaimAirdropAction, ClaimTarget};
use simulation_state::primitives::ProtocolRef;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower an `Airdrop::Claim` action into the `Airdrop::ClaimContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &ClaimAirdropAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("source".into(), lower_protocol_ref(&action.source));
    m.insert("claimTarget".into(), lower_claim_target(&action.claim_target));
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));

    // `proof` (Merkle inclusion proof) → Set<String> of sibling hashes;
    // omitted entirely when absent.
    if let Some(proof) = &action.proof {
        let siblings = proof
            .siblings
            .iter()
            .map(|h| Value::String(h.clone()))
            .collect();
        m.insert("proof".into(), Value::Array(siblings));
    }
    // `sig` (EIP-712 signature, 0x-hex bytes) → String; omitted when absent.
    if let Some(sig) = &action.sig {
        m.insert("sig".into(), Value::String(sig.clone()));
    }

    // ----- Live inputs (LiveField<T> inlined to T) -----
    m.insert(
        "isStillClaimable".into(),
        Value::Bool(action.live_inputs.is_still_claimable.value),
    );
    m.insert(
        "actualAmount".into(),
        Value::String(u256_hex(action.live_inputs.actual_amount.value)),
    );
    // `actualAmountNano` / `actualAmountUsd` are host-populated 3-layer
    // siblings — always omitted here.
    m.insert(
        "claimToken".into(),
        lower_token_ref(&action.live_inputs.claim_token.value),
    );
    // `claim_window` is LiveField<Option<(Time, Time)>>; flatten the inner
    // tuple to two parallel optional Long fields (both present or both absent).
    if let Some((start, end)) = &action.live_inputs.claim_window.value {
        m.insert("claimWindowStart".into(), Value::from(start.as_unix()));
        m.insert("claimWindowEnd".into(), Value::from(end.as_unix()));
    }
    // `custom` is OMITTED — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Airdrop::Action::"Claim""#, Value::Object(m)))
}

/// Lower a [`ProtocolRef`] → `{ name, version?, chain?, market? }`
/// (`Core::ProtocolRef`). Absent optionals are omitted.
fn lower_protocol_ref(source: &ProtocolRef) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(source.name.clone()));
    if let Some(version) = &source.version {
        m.insert("version".into(), Value::String(version.clone()));
    }
    if let Some(chain) = &source.chain {
        m.insert("chain".into(), Value::String(chain.to_string()));
    }
    if let Some(market) = &source.market {
        m.insert("market".into(), Value::String(market.clone()));
    }
    Value::Object(m)
}

/// Lower a [`ClaimTarget`] → discriminated `{ kind, chain, contract, index? }`
/// (`Airdrop::ClaimTarget`). Only `MerkleDistributor` carries `index`.
fn lower_claim_target(target: &ClaimTarget) -> Value {
    let mut m = Map::new();
    match target {
        ClaimTarget::MerkleDistributor {
            chain,
            contract,
            index,
        } => {
            m.insert("kind".into(), Value::String("merkle_distributor".into()));
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("contract".into(), Value::String(addr(contract)));
            m.insert("index".into(), Value::from(*index));
        }
        ClaimTarget::SignatureDistributor { chain, contract } => {
            m.insert("kind".into(), Value::String("signature_distributor".into()));
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("contract".into(), Value::String(addr(contract)));
        }
        ClaimTarget::StakingClaim { chain, contract } => {
            m.insert("kind".into(), Value::String("staking_claim".into()));
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("contract".into(), Value::String(addr(contract)));
        }
    }
    Value::Object(m)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use std::str::FromStr;

    use simulation_reducer::action::airdrop::{
        AirdropAction, ClaimAirdropAction, ClaimAirdropLiveInputs, ClaimTarget,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::position::MerkleProof;
    use simulation_state::primitives::{Address, ChainId, ProtocolRef, Time, U256};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::LiveField;

    use super::super::test_support::{assert_conforms, onchain_source, now, sample_token_ref};

    /// A Merkle-distributor claim with proof, claim window, on-chain meta.
    fn sample_claim() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let claim = AirdropAction::Claim(ClaimAirdropAction {
            source: ProtocolRef::new("optimism"),
            claim_target: ClaimTarget::MerkleDistributor {
                chain: chain.clone(),
                contract: Address::from_str("0xfeedfeedfeedfeedfeedfeedfeedfeedfeedfeed")
                    .unwrap(),
                index: 1234,
            },
            recipient: Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            proof: Some(MerkleProof {
                leaf_index: 1234,
                siblings: vec![
                    "0xaaa0000000000000000000000000000000000000000000000000000000000000".into(),
                    "0xbbb0000000000000000000000000000000000000000000000000000000000000".into(),
                ],
            }),
            sig: None,
            live_inputs: ClaimAirdropLiveInputs {
                is_still_claimable: LiveField::new(true, onchain_source(), now()),
                actual_amount: LiveField::new(U256::from(5_000_000u64), onchain_source(), now()),
                claim_token: LiveField::new(sample_token_ref(&chain), onchain_source(), now()),
                claim_window: LiveField::new(
                    Some((Time::from_unix(1_738_000_000), Time::from_unix(1_739_000_000))),
                    onchain_source(),
                    now(),
                ),
            },
        });

        (ActionBody::Airdrop(claim), super::super::test_support::onchain_meta())
    }

    #[test]
    fn claim_lowering_conforms_to_schema() {
        let (body, meta) = sample_claim();
        assert_conforms("claim", &body, &meta);
    }

    /// A signature-distributor claim with `sig` set, no proof / no window —
    /// widens the gate over the optional branches.
    #[test]
    fn claim_signature_no_window_conforms_to_schema() {
        let chain = ChainId::arbitrum();
        let claim = AirdropAction::Claim(ClaimAirdropAction {
            source: ProtocolRef::new("arbitrum_dao").with_version("v2"),
            claim_target: ClaimTarget::SignatureDistributor {
                chain: chain.clone(),
                contract: Address::from_str("0xfeedfeedfeedfeedfeedfeedfeedfeedfeedfeed")
                    .unwrap(),
            },
            recipient: Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            proof: None,
            sig: Some("0xdeadbeef".into()),
            live_inputs: ClaimAirdropLiveInputs {
                is_still_claimable: LiveField::new(false, onchain_source(), now()),
                actual_amount: LiveField::new(U256::ZERO, onchain_source(), now()),
                claim_token: LiveField::new(
                    TokenRef {
                        key: TokenKey::Native { chain },
                    },
                    onchain_source(),
                    now(),
                ),
                claim_window: LiveField::new(None, onchain_source(), now()),
            },
        });

        let body = ActionBody::Airdrop(claim);
        assert_conforms("claim", &body, &super::super::test_support::onchain_meta());
    }

    /// A staking-reward claim (the third `ClaimTarget` variant, untested by the
    /// other samples) whose `source` ProtocolRef sets every optional field
    /// (`version` + `chain` + `market`) — exercising the `Some` branch of each
    /// optional in `lower_protocol_ref`, which the merkle/signature samples
    /// leave `None`. Neither `proof` nor `sig` is supplied (a staking claim
    /// needs neither), so the omitted-both combination is covered here too.
    #[test]
    fn claim_staking_with_full_source_conforms_to_schema() {
        let chain = ChainId::base();
        let claim = AirdropAction::Claim(ClaimAirdropAction {
            source: ProtocolRef {
                name: "lido".into(),
                version: Some("v2".into()),
                chain: Some(ChainId::ethereum_mainnet()),
                market: Some("steth".into()),
            },
            claim_target: ClaimTarget::StakingClaim {
                chain: chain.clone(),
                contract: Address::from_str("0xfeedfeedfeedfeedfeedfeedfeedfeedfeedfeed")
                    .unwrap(),
            },
            recipient: Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            proof: None,
            sig: None,
            live_inputs: ClaimAirdropLiveInputs {
                is_still_claimable: LiveField::new(true, onchain_source(), now()),
                actual_amount: LiveField::new(U256::from(42u64), onchain_source(), now()),
                claim_token: LiveField::new(sample_token_ref(&chain), onchain_source(), now()),
                claim_window: LiveField::new(
                    Some((Time::from_unix(1_738_000_000), Time::from_unix(1_739_000_000))),
                    onchain_source(),
                    now(),
                ),
            },
        });

        let body = ActionBody::Airdrop(claim);
        assert_conforms("claim", &body, &super::super::test_support::onchain_meta());
    }

    /// A merkle claim whose `source` ProtocolRef sets `chain` but leaves
    /// `version`/`market` `None`, paired with `proof = Some` and `sig = None`.
    /// The other merkle sample (`sample_claim`) leaves the source's `chain`
    /// `None`, so this isolates the `chain = Some` / `market = None` mix.
    #[test]
    fn claim_merkle_source_chain_only_conforms_to_schema() {
        let chain = ChainId::arbitrum();
        let mut source = ProtocolRef::new("optimism");
        source.chain = Some(ChainId::ethereum_mainnet());
        let claim = AirdropAction::Claim(ClaimAirdropAction {
            source,
            claim_target: ClaimTarget::MerkleDistributor {
                chain: chain.clone(),
                contract: Address::from_str("0xfeedfeedfeedfeedfeedfeedfeedfeedfeedfeed")
                    .unwrap(),
                index: 0,
            },
            recipient: Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            proof: Some(MerkleProof {
                leaf_index: 0,
                siblings: vec![],
            }),
            sig: None,
            live_inputs: ClaimAirdropLiveInputs {
                is_still_claimable: LiveField::new(true, onchain_source(), now()),
                actual_amount: LiveField::new(U256::from(1u64), onchain_source(), now()),
                claim_token: LiveField::new(sample_token_ref(&chain), onchain_source(), now()),
                claim_window: LiveField::new(None, onchain_source(), now()),
            },
        });

        let body = ActionBody::Airdrop(claim);
        assert_conforms("claim", &body, &super::super::test_support::onchain_meta());
    }
}
