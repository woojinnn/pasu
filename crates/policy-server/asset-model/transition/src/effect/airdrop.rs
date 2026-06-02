//! `AirdropAction` reducers — `Claim` / `Delegate`.
//!
//! ## Claim semantics (spec §7)
//! [`ClaimAirdropAction`] handles the three on-chain distributor variants
//! ([`ClaimTarget::MerkleDistributor`], [`ClaimTarget::SignatureDistributor`],
//! [`ClaimTarget::StakingClaim`]). All three resolve to the same observable
//! effect from the wallet's perspective: a one-shot credit of
//! `live_inputs.actual_amount` of `live_inputs.claim_token` to the recipient,
//! plus an [`AirdropClaim`]
//! `Position` opened with [`ClaimStatus::Claimed`] for audit. The variants
//! differ only in the on-chain payload (`proof`, `sig`, or pure
//! `staking-balance` lookup), all of which are recorded by the
//! [`ClaimTarget`] match arms below and are not part of the state delta —
//! the `Action.meta.calldata` carries the wire form. The signature variant
//! does NOT emit a `PendingTx`: spec §7 treats `Claim` as an atomic
//! on-chain claim+redeem (the wallet submits the sig as part of the
//! `redeem(...)` tx), unlike `Permit` where the sig is shared separately
//! and the spender redeems later.
//! ## Pre-conditions enforced
//! * `live_inputs.is_still_claimable.value` must be `true` — otherwise an
//!   `Invariant` error is returned (treating "already claimed / expired"
//!   as a programmer-side mistake, not a token-not-found situation).
//! * `live_inputs.claim_window`, if present, must contain `ctx.now` — out
//!   of window is also `Invariant`.
//! * The `MerkleDistributor` variant requires `self.proof.is_some()`.
//! * The `SignatureDistributor` variant requires `self.sig.is_some()`.
//! * `StakingClaim` requires no payload field on `self`.
//!
//! The wallet must already track a [`TokenHolding`](policy_state::token::TokenHolding)
//! for `claim_token` (so `credit` can append a `BalanceDelta`). The sync
//! orchestrator is expected to seed an empty holding before any first-time
//! claim — first-time receipt is a separate concern from this reducer.
//! ## Delegate semantics
//! Governance delegation (UNI / COMP / ENS / OP / ARB style `delegate(...)`)
//! does not change the delegator's balance, allowances, or positions. It is
//! a pure voting-power rotation on the token contract. The reducer therefore
//! returns an empty [`StateDelta`] with one structural guard: the token must
//! be an `Erc20` (a non-ERC20 token cannot host an `ERC20Votes` delegate).
//! Tracking the live delegate address is the sync orchestrator's job (it
//! re-reads `delegates(holder)` and refreshes
//! `DelegateLiveInputs.current_delegate`), so no `Position` is emitted —
//! the wallet's voting state lives on the live field, not in the delta.

use policy_state::position::{AirdropClaim, ClaimStatus, Position, PositionKind};
use policy_state::primitives::Time;
use policy_state::{DataSource, EvalContext, PositionChange, StateDelta, TokenKey, WalletState};

use crate::action::airdrop::{
    AirdropAction, ClaimAirdropAction, ClaimTarget, DelegateGovernanceAction,
};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

impl Reducer for AirdropAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::Claim(a) => a.apply(state, ctx),
            Self::Delegate(a) => a.apply(state, ctx),
        }
    }
}

impl Reducer for ClaimAirdropAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        // ---- 1. Validate `live_inputs.is_still_claimable` ---------------
        if !self.live_inputs.is_still_claimable.value {
            return Err(ReducerError::Invariant(format!(
                "airdrop claim from {:?} is no longer claimable",
                self.source.name
            )));
        }

        // ---- 2. Validate claim window (if present) ----------------------
        if let Some((start, end)) = self.live_inputs.claim_window.value {
            if ctx.now < start || ctx.now > end {
                return Err(ReducerError::Invariant(format!(
                    "airdrop claim from {} is outside its valid window ({}..={})",
                    self.source.name,
                    start.as_unix(),
                    end.as_unix(),
                )));
            }
        }

        // ---- 3. Validate ClaimTarget payload presence --------------------
        validate_claim_target(self)?;

        // ---- 4. Compute claim amount + emit credit ---------------------
        let claim_amount = self.live_inputs.actual_amount.value;
        let claim_token = self.live_inputs.claim_token.value.clone();

        let mut delta = StateDelta::new();
        helpers::balance::credit(state, &mut delta, &claim_token.key, claim_amount)?;

        // ---- 5. Open AirdropClaim position (status = Claimed) ----------
        let position_id = airdrop_position_id(self);
        let chain = Some(self.live_inputs.claim_token.value.key.chain().clone());
        let position = Position {
            id: position_id,
            protocol: self.source.clone(),
            chain,
            kind: PositionKind::AirdropClaim(AirdropClaim {
                source: self.source.clone(),
                claimable: claim_token,
                amount: claim_amount,
                proof: self.proof.clone(),
                claim_window: self.live_inputs.claim_window.value,
                status: ClaimStatus::Claimed,
            }),
            primitives_synced_at: ctx.now,
            primitives_source: DataSource::UserSupplied,
        };
        delta
            .position_changes
            .push(PositionChange::Open { position });

        Ok(delta)
    }
}

/// Validate that the payload fields on `action` match the discriminant on
/// its `ClaimTarget`. `MerkleDistributor` needs `proof`; `SignatureDistributor`
/// needs `sig`; `StakingClaim` requires neither.
const fn validate_claim_target(action: &ClaimAirdropAction) -> ReducerResult<()> {
    match &action.claim_target {
        ClaimTarget::MerkleDistributor { .. } => {
            if action.proof.is_none() {
                return Err(ReducerError::MissingField(
                    "ClaimAirdropAction.proof for MerkleDistributor",
                ));
            }
        }
        ClaimTarget::SignatureDistributor { .. } => {
            if action.sig.is_none() {
                return Err(ReducerError::MissingField(
                    "ClaimAirdropAction.sig for SignatureDistributor",
                ));
            }
        }
        ClaimTarget::StakingClaim { .. } => {
            // No on-self payload — staking balance is read live.
        }
    }
    Ok(())
}

/// Deterministic position id for a claimed airdrop. Shape:
/// `airdrop:{source.name}:{recipient_hex}` — combining the source
/// protocol with the receiving wallet address gives a stable identifier
/// across replays of the same claim.
fn airdrop_position_id(action: &ClaimAirdropAction) -> String {
    format!("airdrop:{}:{:#x}", action.source.name, action.recipient)
}

impl Reducer for DelegateGovernanceAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        // Structural guard: `delegate(...)` is an ERC20Votes call, so a
        // non-ERC20 token (Native / NFT) cannot be the subject. The actual
        // ERC20Votes interface check belongs to the manifest layer.
        match &self.token.key {
            TokenKey::Erc20 { .. } => {}
            other => {
                return Err(ReducerError::Invariant(format!(
                    "DelegateGovernanceAction.token must be Erc20, got {other:?}"
                )));
            }
        }

        // No state-level mutation: delegation rotates voting-power on the
        // token contract; the wallet's recorded `current_delegate` lives in
        // `DelegateLiveInputs.current_delegate` and is refreshed by the sync
        // orchestrator on the next read of `delegates(holder)`. The
        // delegator's balance / allowances / positions are unchanged.
        let _ = self.delegatee;
        // Touch `live_inputs` so it isn't flagged as unused while we keep
        // the symmetry with the orchestrator-owned refresh path.
        let _ = &self.live_inputs.current_delegate;
        let _ = &self.live_inputs.voting_power;

        Ok(StateDelta::new())
    }
}

// -- Helpers for tests below ---------------------------------------------

#[allow(dead_code)]
const fn _ensure_time_zero() -> Time {
    Time::from_unix(0)
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::airdrop::{ClaimAirdropLiveInputs, DelegateLiveInputs};
    use policy_state::delta::TokenChange;
    use policy_state::eval_context::RequestKind;
    use policy_state::live_field::DataSource;
    use policy_state::position::MerkleProof;
    use policy_state::primitives::{Address, ChainId, Duration, ProtocolRef, Time, U256};
    use policy_state::token::{Balance, BaseCategory, TokenHolding, TokenKey, TokenKind, TokenRef};
    use policy_state::wallet::{WalletId, WalletState};
    use policy_state::LiveField;
    use std::str::FromStr;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn distributor() -> Address {
        Address::from_str("0x00000000000000000000000000000000Dabb33ed").unwrap()
    }

    fn op_token_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0x4200000000000000000000000000000000000042").unwrap(),
            },
        }
    }

    fn make_token_holding(amount: u128) -> TokenHolding {
        let key = op_token_ref().key;
        let contract = key
            .contract()
            .copied()
            .unwrap_or_else(|| Address::from([0u8; 20]));
        TokenHolding {
            key,
            kind: TokenKind::Base {
                category: BaseCategory::Governance {
                    protocol: ProtocolRef::new("optimism"),
                },
                peg_to: None,
            },
            symbol: "OP".into(),
            decimals: 18,
            balance: Balance::fungible(U256::from(amount)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract,
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        }
    }

    fn state_with_op_holding() -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        let h = make_token_holding(0);
        s.tokens.insert(h.key.clone(), h);
        s
    }

    fn ctx() -> EvalContext {
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn live_bool(v: bool) -> LiveField<bool> {
        LiveField::new(v, DataSource::UserSupplied, now()).with_ttl(Duration::from_secs(60))
    }

    fn live_amount(v: u128) -> LiveField<U256> {
        LiveField::new(U256::from(v), DataSource::UserSupplied, now())
            .with_ttl(Duration::from_secs(60))
    }

    fn live_token(t: TokenRef) -> LiveField<TokenRef> {
        LiveField::new(t, DataSource::UserSupplied, now()).with_ttl(Duration::from_secs(60))
    }

    fn live_window(window: Option<(Time, Time)>) -> LiveField<Option<(Time, Time)>> {
        LiveField::new(window, DataSource::UserSupplied, now()).with_ttl(Duration::from_secs(60))
    }

    fn merkle_action_with_proof() -> ClaimAirdropAction {
        ClaimAirdropAction {
            source: ProtocolRef::new("optimism"),
            claim_target: ClaimTarget::MerkleDistributor {
                chain: ChainId::ethereum_mainnet(),
                contract: distributor(),
                index: 42,
            },
            recipient: user(),
            proof: Some(MerkleProof {
                leaf_index: 42,
                siblings: vec!["0x01".into(), "0x02".into()],
            }),
            sig: None,
            live_inputs: ClaimAirdropLiveInputs {
                is_still_claimable: live_bool(true),
                actual_amount: live_amount(1_000_000_000_000_000_000u128),
                claim_token: live_token(op_token_ref()),
                claim_window: live_window(None),
            },
        }
    }

    // ---------- ClaimAirdropAction (Merkle) ----------

    #[test]
    fn claim_merkle_happy_path_emits_credit_and_opens_position() {
        let state = state_with_op_holding();
        let action = merkle_action_with_proof();
        let delta = action.apply(&state, &ctx()).unwrap();

        // 1 credit + 1 position open.
        assert_eq!(delta.token_changes.len(), 1);
        let TokenChange::BalanceDelta { key, delta: d } = &delta.token_changes[0] else {
            panic!("expected BalanceDelta");
        };
        assert_eq!(*key, op_token_ref().key);
        assert!(!d.is_negative(), "credit must be positive");

        assert_eq!(delta.position_changes.len(), 1);
        let PositionChange::Open { position } = &delta.position_changes[0] else {
            panic!("expected Position::Open");
        };
        assert!(matches!(position.kind, PositionKind::AirdropClaim(_)));
        if let PositionKind::AirdropClaim(c) = &position.kind {
            assert!(matches!(c.status, ClaimStatus::Claimed));
            assert_eq!(c.amount, U256::from(1_000_000_000_000_000_000u128));
        }
    }

    #[test]
    fn claim_merkle_missing_proof_is_missing_field() {
        let state = state_with_op_holding();
        let mut action = merkle_action_with_proof();
        action.proof = None;
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::MissingField(_)));
    }

    #[test]
    fn claim_not_claimable_is_invariant() {
        let state = state_with_op_holding();
        let mut action = merkle_action_with_proof();
        action.live_inputs.is_still_claimable = live_bool(false);
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    #[test]
    fn claim_outside_window_is_invariant() {
        let state = state_with_op_holding();
        let mut action = merkle_action_with_proof();
        // Window ended one second before `now`.
        let end = Time::from_unix(now().as_unix() - 1);
        let start = Time::from_unix(end.as_unix() - 100);
        action.live_inputs.claim_window = live_window(Some((start, end)));
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    // ---------- ClaimAirdropAction (Signature) ----------

    #[test]
    fn claim_signature_happy_path_emits_credit_and_position() {
        let state = state_with_op_holding();
        let action = ClaimAirdropAction {
            source: ProtocolRef::new("optimism_v2"),
            claim_target: ClaimTarget::SignatureDistributor {
                chain: ChainId::ethereum_mainnet(),
                contract: distributor(),
            },
            recipient: user(),
            proof: None,
            sig: Some("0xdeadbeef".into()),
            live_inputs: ClaimAirdropLiveInputs {
                is_still_claimable: live_bool(true),
                actual_amount: live_amount(500),
                claim_token: live_token(op_token_ref()),
                claim_window: live_window(None),
            },
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        assert_eq!(delta.position_changes.len(), 1);
        // Signature variant does NOT emit a PendingTx — spec §7 treats
        // Claim as a single atomic on-chain claim+redeem.
        assert!(delta.pending_changes.is_empty());
    }

    #[test]
    fn claim_signature_missing_sig_is_missing_field() {
        let state = state_with_op_holding();
        let action = ClaimAirdropAction {
            source: ProtocolRef::new("optimism_v2"),
            claim_target: ClaimTarget::SignatureDistributor {
                chain: ChainId::ethereum_mainnet(),
                contract: distributor(),
            },
            recipient: user(),
            proof: None,
            sig: None,
            live_inputs: ClaimAirdropLiveInputs {
                is_still_claimable: live_bool(true),
                actual_amount: live_amount(500),
                claim_token: live_token(op_token_ref()),
                claim_window: live_window(None),
            },
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::MissingField(_)));
    }

    // ---------- ClaimAirdropAction (StakingClaim) ----------

    #[test]
    fn claim_staking_happy_path_credits_and_opens_position() {
        let state = state_with_op_holding();
        let action = ClaimAirdropAction {
            source: ProtocolRef::new("lido"),
            claim_target: ClaimTarget::StakingClaim {
                chain: ChainId::ethereum_mainnet(),
                contract: distributor(),
            },
            recipient: user(),
            proof: None,
            sig: None,
            live_inputs: ClaimAirdropLiveInputs {
                is_still_claimable: live_bool(true),
                actual_amount: live_amount(7_777),
                claim_token: live_token(op_token_ref()),
                claim_window: live_window(None),
            },
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        assert_eq!(delta.position_changes.len(), 1);
    }

    #[test]
    fn claim_unknown_token_returns_token_not_found() {
        let state = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        let action = merkle_action_with_proof();
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::TokenNotFound(_)));
    }

    // ---------- DelegateGovernanceAction ----------

    fn live_addr_opt() -> LiveField<Option<Address>> {
        LiveField::new(None, DataSource::UserSupplied, now()).with_ttl(Duration::from_secs(60))
    }

    fn live_voting_power(v: u128) -> LiveField<U256> {
        LiveField::new(U256::from(v), DataSource::UserSupplied, now())
            .with_ttl(Duration::from_secs(60))
    }

    #[test]
    fn delegate_governance_emits_empty_delta_on_erc20() {
        let state = state_with_op_holding();
        let action = DelegateGovernanceAction {
            token: op_token_ref(),
            delegatee: distributor(),
            live_inputs: DelegateLiveInputs {
                current_delegate: live_addr_opt(),
                voting_power: live_voting_power(100),
            },
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert!(delta.is_empty());
    }

    #[test]
    fn delegate_governance_rejects_native_token() {
        let state = state_with_op_holding();
        let action = DelegateGovernanceAction {
            token: TokenRef {
                key: TokenKey::Native {
                    chain: ChainId::ethereum_mainnet(),
                },
            },
            delegatee: distributor(),
            live_inputs: DelegateLiveInputs {
                current_delegate: live_addr_opt(),
                voting_power: live_voting_power(0),
            },
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    // ---------- AirdropAction dispatcher ----------

    #[test]
    fn airdrop_action_dispatcher_routes_to_claim() {
        let state = state_with_op_holding();
        let outer = AirdropAction::Claim(merkle_action_with_proof());
        let delta = outer.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        assert_eq!(delta.position_changes.len(), 1);
    }

    #[test]
    fn airdrop_action_dispatcher_routes_to_delegate() {
        let state = state_with_op_holding();
        let outer = AirdropAction::Delegate(DelegateGovernanceAction {
            token: op_token_ref(),
            delegatee: distributor(),
            live_inputs: DelegateLiveInputs {
                current_delegate: live_addr_opt(),
                voting_power: live_voting_power(0),
            },
        });
        let delta = outer.apply(&state, &ctx()).unwrap();
        assert!(delta.is_empty());
    }
}
