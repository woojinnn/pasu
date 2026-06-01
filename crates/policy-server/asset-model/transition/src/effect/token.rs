//! `TokenAction` reducers — `ERC20` / `ERC721` / `ERC1155` / `Permit2` ops.
//! ## Time semantics
//! Approval-side helpers (`set_erc20_allowance`, `set_for_all`, `set_nft_approve`)
//! consume a `Time` for `AllowanceSpec::last_set_at`. We use `ctx.now` —
//! "evaluation now" — as the timestamp, NOT `Action.meta.submitted_at`. The two
//! diverge for off-chain signatures whose `deadline` lies in the future, but
//! `last_set_at` is a *recorded-at* slot, not a *valid-until* slot. `ctx.now`
//! matches the convention already used by `helpers::approval` tests
//! (`now() = Time::from_unix(1_738_000_000)`).
//! ## Off-chain sig handling (`Erc20Permit`, `Permit2SignAllowance`)
//! Off-chain signatures do not mutate on-chain state at the moment of signing
//! — only when a relayer presents them. We model the signing event as a
//! `PendingTx` with `commitment: AssetCommitment::PermitCap` so that the
//! wallet's `committed` accounting (PDF §6.1) reflects the spend cap that
//! the signature has authorised, even though no on-chain allowance has been
//! set yet. Lifecycle starts in `Active`. The corresponding lifecycle row
//! `valid_until = expires_at` is the on-chain expiration; PDF §6 keeps
//! `sig_deadline` separately and we record it via the `signed_at` slot for
//! audit (the broader "two deadlines" representation is a follow-up).

use policy_state::pending::{
    AssetCommitment, NonceKey, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
};
use policy_state::primitives::Spender;
use policy_state::{DataSource, EvalContext, PendingChange, StateDelta, TokenKey, WalletState};

use crate::action::token::{
    Erc20ApproveAction, Erc20PermitAction, Erc20TransferAction, NftApproveAction,
    NftSetForAllAction, NftTransferAction, Permit2ApproveAction, Permit2SignAction,
    RevokeApprovalAction, RevokeScope, TokenAction,
};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

// ---------------------------------------------------------------------------
// Small helpers shared by `Erc20Permit` and `Permit2SignAllowance`.
// ---------------------------------------------------------------------------

/// Synthesize a `DataSource` for a freshly-emitted `PendingTx`. The sync
/// orchestrator owns later polling; reducer-side we mark the entry as
/// `UserSupplied` so it is not auto-refreshed from chain until a downstream
/// step swaps in a real `OnchainView` / `VenueApi` source.
const fn pending_user_source() -> DataSource {
    DataSource::UserSupplied
}

/// Stable pending id derived from the constituent fields. Same shape used by
/// rpc-server / db crate later; reducer-side we only need determinism so that
/// re-evaluating the same `Action` produces the same `PendingTx.id`.
fn pending_id_for_eip2612(token_addr_hex: &str, spender_hex: &str, nonce: &str) -> String {
    format!("eip2612:{token_addr_hex}:{spender_hex}:{nonce}")
}

fn pending_id_for_permit2(token_addr_hex: &str, spender_hex: &str, word: &str, bit: u8) -> String {
    format!("permit2:{token_addr_hex}:{spender_hex}:{word}:{bit}")
}

// ---------------------------------------------------------------------------
// Dispatcher.
// ---------------------------------------------------------------------------

impl Reducer for TokenAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::Erc20Approve(a) => a.apply(state, ctx),
            Self::Erc20Permit(a) => a.apply(state, ctx),
            Self::Permit2Approve(a) => a.apply(state, ctx),
            Self::Permit2SignAllowance(a) => a.apply(state, ctx),
            Self::Erc20Transfer(a) => a.apply(state, ctx),
            Self::NftApprove(a) => a.apply(state, ctx),
            Self::NftSetApprovalForAll(a) => a.apply(state, ctx),
            Self::NftTransfer(a) => a.apply(state, ctx),
            Self::RevokeApproval(a) => a.apply(state, ctx),
        }
    }
}

// ---------------------------------------------------------------------------
// ERC20 approve — fully deterministic, no `LiveField`.
// ---------------------------------------------------------------------------

impl Reducer for Erc20ApproveAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();
        // `approve(spender, 0)` is dispatched through the same helper —
        // `set_erc20_allowance` accepts zero. Distinct revoke semantics are
        // reachable via `RevokeApprovalAction { scope: Erc20 }`.
        helpers::approval::set_erc20_allowance(
            state,
            &mut delta,
            ctx.now,
            &self.token,
            self.spender,
            self.amount,
        )?;
        Ok(delta)
    }
}

// ---------------------------------------------------------------------------
// ERC20 permit (EIP-2612) — off-chain sig; emits a pending entry.
// ---------------------------------------------------------------------------

impl Reducer for Erc20PermitAction {
    fn apply(&self, _state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();

        let token_addr = match &self.token.key {
            TokenKey::Erc20 { address, .. } => *address,
            _ => {
                return Err(ReducerError::Invariant(
                    "Erc20PermitAction.token must be Erc20".into(),
                ));
            }
        };

        let nonce_value = self.nonce.value;
        let spender = self.spender;
        let id = pending_id_for_eip2612(
            &format!("{token_addr:#x}"),
            &format!("{spender:#x}"),
            &format!("{nonce_value}"),
        );

        let pending = PendingTx {
            id,
            kind: PendingKind::SignedEIP2612 {
                token: self.token.clone(),
                spender: self.spender,
                amount: self.amount,
                expires_at: self.deadline,
                nonce: nonce_value,
            },
            commitment: AssetCommitment::PermitCap {
                token: self.token.clone(),
                spender: self.spender,
                max_out: self.amount,
            },
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status: PendingStatus::Active,
                valid_until: Some(self.deadline),
                nonce: Some(NonceKey::Eip2612 {
                    token: token_addr,
                    nonce: nonce_value,
                }),
                on_chain_tx: None,
            },
            sync: pending_user_source(),
            signed_at: ctx.now,
            signature_payload: Vec::new(),
        };

        delta.pending_changes.push(PendingChange::Add {
            pending: Box::new(pending),
        });

        Ok(delta)
    }
}

// ---------------------------------------------------------------------------
// Permit2 on-chain `approve`.
// ---------------------------------------------------------------------------

impl Reducer for Permit2ApproveAction {
    fn apply(&self, state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();
        helpers::approval::upsert_permit2_allowance(
            state,
            &mut delta,
            &self.token,
            Spender::from(self.spender),
            self.amount,
            self.expires_at,
        )?;
        Ok(delta)
    }
}

// ---------------------------------------------------------------------------
// Permit2 off-chain sig (single allowance).
// ---------------------------------------------------------------------------

impl Reducer for Permit2SignAction {
    fn apply(&self, _state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();

        let token_addr = match &self.token.key {
            TokenKey::Erc20 { address, .. } => *address,
            _ => {
                return Err(ReducerError::Invariant(
                    "Permit2SignAction.token must be Erc20".into(),
                ));
            }
        };
        let _ = token_addr;

        let (word, bit) = self.nonce.value;
        let spender = self.spender;
        let id = pending_id_for_permit2(
            &format!("{token_addr:#x}"),
            &format!("{spender:#x}"),
            &format!("{word}"),
            bit,
        );

        let pending = PendingTx {
            id,
            kind: PendingKind::SignedPermit2 {
                token: self.token.clone(),
                spender: self.spender,
                amount: self.amount,
                expires_at: self.expires_at,
                nonce: (word, bit),
            },
            commitment: AssetCommitment::PermitCap {
                token: self.token.clone(),
                spender: self.spender,
                max_out: self.amount,
            },
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status: PendingStatus::Active,
                // Use the on-chain `expires_at` as the lifecycle valid-until
                // window. PDF §6 keeps `sig_deadline` as a separate field;
                // until the lifecycle struct grows a second deadline we record
                // only the on-chain expiration here.
                valid_until: Some(self.expires_at),
                nonce: Some(NonceKey::Permit2 { word, bit }),
                on_chain_tx: None,
            },
            sync: pending_user_source(),
            signed_at: ctx.now,
            signature_payload: Vec::new(),
        };

        delta.pending_changes.push(PendingChange::Add {
            pending: Box::new(pending),
        });

        Ok(delta)
    }
}

// ---------------------------------------------------------------------------
// ERC20 transfer.
// ---------------------------------------------------------------------------

impl Reducer for Erc20TransferAction {
    fn apply(&self, state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();
        helpers::balance::transfer(
            state,
            &mut delta,
            &self.token.key,
            self.recipient,
            self.amount,
        )?;
        Ok(delta)
    }
}

// ---------------------------------------------------------------------------
// NFT per-token approve.
// ---------------------------------------------------------------------------

impl Reducer for NftApproveAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();
        helpers::approval::set_nft_approve(
            state,
            &mut delta,
            ctx.now,
            &self.nft_key,
            self.spender,
        )?;
        Ok(delta)
    }
}

// ---------------------------------------------------------------------------
// NFT setApprovalForAll.
// ---------------------------------------------------------------------------

impl Reducer for NftSetForAllAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();
        helpers::approval::set_for_all(
            state,
            &mut delta,
            ctx.now,
            &self.chain,
            self.contract,
            self.spender,
            self.approved,
        )?;
        Ok(delta)
    }
}

// ---------------------------------------------------------------------------
// NFT transfer (ERC721 single, ERC1155 quantity).
// ---------------------------------------------------------------------------

impl Reducer for NftTransferAction {
    fn apply(&self, state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();
        helpers::balance::transfer_nft(
            state,
            &mut delta,
            &self.nft_key,
            self.recipient,
            self.amount,
        )?;
        Ok(delta)
    }
}

// ---------------------------------------------------------------------------
// Revoke approval (scope-typed).
// ---------------------------------------------------------------------------

impl Reducer for RevokeApprovalAction {
    fn apply(&self, state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();
        match &self.scope {
            RevokeScope::Erc20 { token, spender } => {
                helpers::approval::revoke_erc20_allowance(state, &mut delta, token, *spender)?;
            }
            RevokeScope::NftSingleToken { nft_key } => {
                // Map "revoke per-token NFT approval" to
                // `set_nft_approve(spender = Address::ZERO)` — the helper
                // normalises a zero spender to `Erc721ApprovedTo { spender:
                // None }`, matching `ERC721.approve(address(0), tokenId)`.
                helpers::approval::set_nft_approve(
                    state,
                    &mut delta,
                    policy_state::primitives::Time::from_unix(0),
                    nft_key,
                    policy_state::primitives::Address::ZERO,
                )?;
            }
            RevokeScope::NftSetForAll {
                chain,
                contract,
                spender,
            } => {
                helpers::approval::set_for_all(
                    state,
                    &mut delta,
                    policy_state::primitives::Time::from_unix(0),
                    chain,
                    *contract,
                    *spender,
                    false,
                )?;
            }
            RevokeScope::Permit2Lockdown { token, spender } => {
                helpers::approval::revoke_permit2_allowance(state, &mut delta, token, *spender)?;
            }
            RevokeScope::Permit2UnorderedNonce { .. } => {
                // Permit2 unordered nonce bitmaps are not tracked in WalletState
                // yet. The ActionBody still exposes the bitmap coordinates for
                // policy/UI; simulation has no local approval row to mutate.
            }
        }
        Ok(delta)
    }
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::approval::AllowanceSpec;
    use policy_state::delta::token_change::ApprovalScope as TcApprovalScope;
    use policy_state::delta::TokenChange;
    use policy_state::eval_context::RequestKind;
    use policy_state::live_field::DataSource;
    use policy_state::pending::{AssetCommitment, NonceKey, PendingKind};
    use policy_state::primitives::{Address, ChainId, Duration, Time, U256};
    use policy_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
    };
    use policy_state::wallet::{WalletId, WalletState};
    use policy_state::LiveField;
    use std::str::FromStr;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn later() -> Time {
        Time::from_unix(1_738_086_400)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn spender_addr() -> Address {
        Address::from_str("0x00000000000000000000000000000000DeaDBeef").unwrap()
    }

    fn usdc_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    fn nft_collection() -> Address {
        Address::from_str("0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d").unwrap()
    }

    fn nft_key(token_id: u64) -> TokenKey {
        TokenKey::Erc721 {
            chain: ChainId::ethereum_mainnet(),
            contract: nft_collection(),
            token_id: U256::from(token_id),
        }
    }

    fn erc1155_key(token_id: u64) -> TokenKey {
        TokenKey::Erc1155 {
            chain: ChainId::ethereum_mainnet(),
            contract: nft_collection(),
            token_id: U256::from(token_id),
        }
    }

    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]))
    }

    fn ctx() -> EvalContext {
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn make_usdc_holding(amount: u128) -> TokenHolding {
        let key = usdc_ref().key;
        let contract = key
            .contract()
            .copied()
            .unwrap_or_else(|| Address::from([0u8; 20]));
        TokenHolding {
            key,
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
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

    fn make_nft_holding(key: TokenKey, balance: Balance) -> TokenHolding {
        let contract = key
            .contract()
            .copied()
            .unwrap_or_else(|| Address::from([0u8; 20]));
        TokenHolding {
            key,
            kind: TokenKind::Unknown,
            symbol: "NFT".into(),
            decimals: 0,
            balance,
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract,
                function: "ownerOf(uint256)".into(),
                decoder_id: "erc721_owner".into(),
            },
        }
    }

    fn live_nonce_u256(value: U256) -> LiveField<U256> {
        LiveField::new(value, DataSource::UserSupplied, now()).with_ttl(Duration::from_secs(60))
    }

    fn live_nonce_pair(word: U256, bit: u8) -> LiveField<(U256, u8)> {
        LiveField::new((word, bit), DataSource::UserSupplied, now())
            .with_ttl(Duration::from_secs(60))
    }

    // ---------- Erc20Approve ----------

    #[test]
    fn erc20_approve_emits_approval_set_at_ctx_now() {
        let state = empty_state();
        let action = Erc20ApproveAction {
            token: usdc_ref(),
            spender: spender_addr(),
            amount: U256::from(2_500_000_000u64),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        let TokenChange::ApprovalSet {
            key,
            spender: s,
            allowance,
        } = &delta.token_changes[0]
        else {
            panic!("expected ApprovalSet");
        };
        assert_eq!(*key, usdc_ref().key);
        assert_eq!(*s, spender_addr());
        assert_eq!(allowance.amount, U256::from(2_500_000_000u64));
        assert!(!allowance.is_unlimited);
        assert_eq!(allowance.last_set_at, now());
    }

    #[test]
    fn erc20_approve_unlimited() {
        let state = empty_state();
        let action = Erc20ApproveAction {
            token: usdc_ref(),
            spender: spender_addr(),
            amount: U256::MAX,
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        let TokenChange::ApprovalSet { allowance, .. } = &delta.token_changes[0] else {
            panic!("expected ApprovalSet");
        };
        assert!(allowance.is_unlimited);
    }

    // ---------- Erc20Permit (off-chain sig) ----------

    #[test]
    fn erc20_permit_emits_pending_add_with_permit_cap_commitment() {
        let state = empty_state();
        let nonce = U256::from(7u64);
        let action = Erc20PermitAction {
            token: usdc_ref(),
            spender: spender_addr(),
            amount: U256::from(1_000_000_000u64),
            deadline: later(),
            nonce: live_nonce_u256(nonce),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert!(delta.token_changes.is_empty());
        assert_eq!(delta.pending_changes.len(), 1);
        let PendingChange::Add { pending } = &delta.pending_changes[0] else {
            panic!("expected PendingChange::Add");
        };
        match &pending.commitment {
            AssetCommitment::PermitCap {
                token,
                spender,
                max_out,
            } => {
                assert_eq!(token, &usdc_ref());
                assert_eq!(*spender, spender_addr());
                assert_eq!(*max_out, U256::from(1_000_000_000u64));
            }
            other => panic!("expected PermitCap, got {other:?}"),
        }
        match &pending.kind {
            PendingKind::SignedEIP2612 {
                token,
                spender,
                amount,
                expires_at,
                nonce: n,
            } => {
                assert_eq!(token, &usdc_ref());
                assert_eq!(*spender, spender_addr());
                assert_eq!(*amount, U256::from(1_000_000_000u64));
                assert_eq!(*expires_at, later());
                assert_eq!(*n, nonce);
            }
            other => panic!("expected SignedEIP2612, got {other:?}"),
        }
        assert!(matches!(
            pending.lifecycle.nonce,
            Some(NonceKey::Eip2612 { .. })
        ));
        assert_eq!(pending.lifecycle.valid_until, Some(later()));
        assert_eq!(pending.signed_at, now());
    }

    #[test]
    fn erc20_permit_rejects_non_erc20_key() {
        let state = empty_state();
        let action = Erc20PermitAction {
            token: TokenRef {
                key: TokenKey::Native {
                    chain: ChainId::ethereum_mainnet(),
                },
            },
            spender: spender_addr(),
            amount: U256::from(1u64),
            deadline: later(),
            nonce: live_nonce_u256(U256::ZERO),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    // ---------- Permit2 on-chain approve ----------

    #[test]
    fn permit2_approve_packs_expiration_into_allowance_last_set_at() {
        let state = empty_state();
        let action = Permit2ApproveAction {
            token: usdc_ref(),
            spender: spender_addr(),
            amount: U256::from(5_000_000u64),
            expires_at: later(),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        let TokenChange::ApprovalSet {
            key,
            spender: s,
            allowance,
        } = &delta.token_changes[0]
        else {
            panic!("expected ApprovalSet");
        };
        assert_eq!(*key, usdc_ref().key);
        assert_eq!(*s, spender_addr());
        assert_eq!(allowance.amount, U256::from(5_000_000u64));
        assert_eq!(allowance.last_set_at, later());
    }

    // ---------- Permit2 sign (off-chain) ----------

    #[test]
    fn permit2_sign_emits_pending_with_permit2_nonce_key() {
        let state = empty_state();
        let action = Permit2SignAction {
            token: usdc_ref(),
            spender: spender_addr(),
            amount: U256::from(2_000_000u64),
            expires_at: later(),
            sig_deadline: later(),
            nonce: live_nonce_pair(U256::from(3u64), 7),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.pending_changes.len(), 1);
        let PendingChange::Add { pending } = &delta.pending_changes[0] else {
            panic!("expected PendingChange::Add");
        };
        match &pending.kind {
            PendingKind::SignedPermit2 {
                token,
                spender,
                amount,
                expires_at,
                nonce: (w, b),
            } => {
                assert_eq!(token, &usdc_ref());
                assert_eq!(*spender, spender_addr());
                assert_eq!(*amount, U256::from(2_000_000u64));
                assert_eq!(*expires_at, later());
                assert_eq!(*w, U256::from(3u64));
                assert_eq!(*b, 7);
            }
            other => panic!("expected SignedPermit2, got {other:?}"),
        }
        match pending.lifecycle.nonce.as_ref() {
            Some(NonceKey::Permit2 { word, bit }) => {
                assert_eq!(*word, U256::from(3u64));
                assert_eq!(*bit, 7);
            }
            other => panic!("expected NonceKey::Permit2, got {other:?}"),
        }
    }

    // ---------- Erc20 transfer ----------

    #[test]
    fn erc20_transfer_emits_negative_balance_delta() {
        let mut state = empty_state();
        let holding = make_usdc_holding(1_000_000_000);
        state.tokens.insert(holding.key.clone(), holding);

        let recipient = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
        let action = Erc20TransferAction {
            token: usdc_ref(),
            recipient,
            amount: U256::from(250_000_000u64),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        let TokenChange::BalanceDelta { key, delta: d } = &delta.token_changes[0] else {
            panic!("expected BalanceDelta");
        };
        assert_eq!(*key, usdc_ref().key);
        assert!(d.is_negative());
    }

    #[test]
    fn erc20_transfer_underflow_is_invariant() {
        let mut state = empty_state();
        let holding = make_usdc_holding(100);
        state.tokens.insert(holding.key.clone(), holding);
        let action = Erc20TransferAction {
            token: usdc_ref(),
            recipient: spender_addr(),
            amount: U256::from(101u64),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    // ---------- NFT approve ----------

    #[test]
    fn nft_approve_emits_erc721_approved_to() {
        let state = empty_state();
        let action = NftApproveAction {
            nft_key: nft_key(42),
            spender: spender_addr(),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        let TokenChange::Erc721ApprovedTo { key, spender: s } = &delta.token_changes[0] else {
            panic!("expected Erc721ApprovedTo");
        };
        assert_eq!(*key, nft_key(42));
        assert_eq!(*s, Some(spender_addr()));
    }

    // ---------- NFT setApprovalForAll ----------

    #[test]
    fn nft_set_for_all_true_emits_approval_set_unlimited() {
        let state = empty_state();
        let action = NftSetForAllAction {
            chain: ChainId::ethereum_mainnet(),
            contract: nft_collection(),
            spender: spender_addr(),
            approved: true,
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        let TokenChange::ApprovalSet { allowance, .. } = &delta.token_changes[0] else {
            panic!("expected ApprovalSet");
        };
        assert!(allowance.is_unlimited);
    }

    #[test]
    fn nft_set_for_all_false_emits_revoke_set_for_all_scope() {
        let state = empty_state();
        let action = NftSetForAllAction {
            chain: ChainId::ethereum_mainnet(),
            contract: nft_collection(),
            spender: spender_addr(),
            approved: false,
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        let TokenChange::ApprovalRevoke { scope, .. } = &delta.token_changes[0] else {
            panic!("expected ApprovalRevoke");
        };
        assert_eq!(*scope, TcApprovalScope::SetForAll);
    }

    // ---------- NFT transfer ----------

    #[test]
    fn nft_transfer_erc721_emits_minus_one_balance_delta() {
        let mut state = empty_state();
        let key = nft_key(7);
        state
            .tokens
            .insert(key.clone(), make_nft_holding(key.clone(), Balance::Owned));

        let action = NftTransferAction {
            nft_key: key.clone(),
            amount: None,
            recipient: spender_addr(),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        let TokenChange::BalanceDelta { key: k, delta: d } = &delta.token_changes[0] else {
            panic!("expected BalanceDelta");
        };
        assert_eq!(*k, key);
        assert!(d.is_negative());
    }

    #[test]
    fn nft_transfer_erc1155_emits_balance_delta_fungible() {
        let mut state = empty_state();
        let key = erc1155_key(11);
        state.tokens.insert(
            key.clone(),
            make_nft_holding(key.clone(), Balance::fungible(U256::from(10u64))),
        );

        let action = NftTransferAction {
            nft_key: key.clone(),
            amount: Some(U256::from(3u64)),
            recipient: spender_addr(),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        let TokenChange::BalanceDelta { key: k, delta: d } = &delta.token_changes[0] else {
            panic!("expected BalanceDelta");
        };
        assert_eq!(*k, key);
        assert!(d.is_negative());
    }

    #[test]
    fn nft_transfer_erc1155_without_amount_is_invariant() {
        let mut state = empty_state();
        let key = erc1155_key(11);
        state.tokens.insert(
            key.clone(),
            make_nft_holding(key.clone(), Balance::fungible(U256::from(10u64))),
        );
        let action = NftTransferAction {
            nft_key: key,
            amount: None,
            recipient: spender_addr(),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    // ---------- RevokeApproval ----------

    #[test]
    fn revoke_approval_erc20_routes_to_revoke_helper() {
        let state = empty_state();
        let action = RevokeApprovalAction {
            scope: RevokeScope::Erc20 {
                token: usdc_ref(),
                spender: spender_addr(),
            },
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        let TokenChange::ApprovalRevoke { scope, .. } = &delta.token_changes[0] else {
            panic!("expected ApprovalRevoke");
        };
        assert_eq!(*scope, TcApprovalScope::Erc20);
    }

    #[test]
    fn revoke_approval_nft_single_token_maps_to_zero_spender() {
        let state = empty_state();
        let action = RevokeApprovalAction {
            scope: RevokeScope::NftSingleToken {
                nft_key: nft_key(99),
            },
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        let TokenChange::Erc721ApprovedTo { spender, .. } = &delta.token_changes[0] else {
            panic!("expected Erc721ApprovedTo");
        };
        assert_eq!(*spender, None);
    }

    #[test]
    fn revoke_approval_set_for_all_emits_revoke() {
        let state = empty_state();
        let action = RevokeApprovalAction {
            scope: RevokeScope::NftSetForAll {
                chain: ChainId::ethereum_mainnet(),
                contract: nft_collection(),
                spender: spender_addr(),
            },
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        let TokenChange::ApprovalRevoke { scope, .. } = &delta.token_changes[0] else {
            panic!("expected ApprovalRevoke");
        };
        assert_eq!(*scope, TcApprovalScope::SetForAll);
    }

    #[test]
    fn revoke_approval_permit2_emits_permit2_scoped_revoke() {
        let state = empty_state();
        let action = RevokeApprovalAction {
            scope: RevokeScope::Permit2Lockdown {
                token: usdc_ref(),
                spender: spender_addr(),
            },
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        let TokenChange::ApprovalRevoke { key, scope, .. } = &delta.token_changes[0] else {
            panic!("expected ApprovalRevoke");
        };
        assert_eq!(*key, usdc_ref().key);
        assert_eq!(*scope, TcApprovalScope::Permit2);
    }

    #[test]
    fn revoke_approval_permit2_unordered_nonce_is_metadata_only_today() {
        let state = empty_state();
        let action = RevokeApprovalAction {
            scope: RevokeScope::Permit2UnorderedNonce {
                chain: ChainId::ethereum_mainnet(),
                word_pos: U256::from(42u64),
                mask: U256::from(0xffu64),
            },
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert!(delta.is_empty());
    }

    // ---------- Dispatcher ----------

    #[test]
    fn token_action_dispatcher_routes_to_erc20_approve() {
        let state = empty_state();
        let action = TokenAction::Erc20Approve(Erc20ApproveAction {
            token: usdc_ref(),
            spender: spender_addr(),
            amount: U256::from(1u64),
        });
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        assert!(matches!(
            &delta.token_changes[0],
            TokenChange::ApprovalSet { .. }
        ));
    }

    // ---------- helper smoke: ensure non-erc20 sender for sign actions errs ----------

    #[test]
    fn permit2_sign_rejects_non_erc20_token() {
        let state = empty_state();
        let action = Permit2SignAction {
            token: TokenRef {
                key: TokenKey::Native {
                    chain: ChainId::ethereum_mainnet(),
                },
            },
            spender: spender_addr(),
            amount: U256::from(1u64),
            expires_at: later(),
            sig_deadline: later(),
            nonce: live_nonce_pair(U256::ZERO, 0),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    // Suppress unused — `AllowanceSpec` re-exported only to ensure test
    // compilation is referencing a stable path.
    #[test]
    fn _allowance_spec_path_is_stable() {
        let _ = AllowanceSpec::new(U256::ZERO, Time::from_unix(0));
    }
}
