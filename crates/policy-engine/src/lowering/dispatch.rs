//! Dispatch from normalized action envelopes to per-action policy requests.

use crate::action::{Action, ActionEnvelope, Address, DecimalString};
use crate::policy::PolicyRequest;
use serde_json::Value;

use super::common::cedar::entities;
use super::error::LoweringError;

#[allow(dead_code)]
pub(crate) struct LoweringCtx<'a> {
    pub(crate) from: &'a Address,
    pub(crate) to: &'a Address,
    pub(crate) value_wei: &'a DecimalString,
    pub(crate) chain_id: u64,
    pub(crate) block_timestamp: u64,
}

impl LoweringCtx<'_> {
    /// Assemble the standard `Wallet`/`Action`/`Protocol` triple for an action.
    ///
    /// `action_kind` flows into `Action::"<kind>"`. The `Protocol` resource
    /// uid is the transaction target (`self.to`) so policies can match the
    /// contract being interacted with — e.g.
    /// `resource == Protocol::"0xUniswapV3Router"`.
    pub(crate) fn request(&self, action_kind: &str, context: Value) -> PolicyRequest {
        PolicyRequest::new(
            format!(r#"Wallet::"{}""#, self.from),
            format!(r#"Action::"{action_kind}""#),
            format!(r#"Protocol::"{}""#, self.to),
            entities(self.from, self.to),
            context,
        )
    }
}

/// Per-action contract: build a Cedar `PolicyRequest` from a single action
/// payload plus the lowering context. Implemented once per action variant in
/// the matching `lowering/<category>/<action>.rs` file. The dispatcher in this
/// module matches on [`Action`] and calls [`Lower::build`] on the wrapped
/// payload, which keeps every per-action implementation honest about its
/// signature.
pub(crate) trait Lower {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest;
}

/// Build a Cedar policy request from a normalized action envelope.
///
/// # Errors
///
/// Returns [`LoweringError::UnsupportedAction`] when the action variant has no
/// per-action lowering implementation yet. Callers must convert this into a
/// fails-closed engine error rather than silently letting the transaction
/// continue — that is the whole reason this returns `Result` instead of
/// `Option`.
pub fn policy_request_from_envelope(
    envelope: &ActionEnvelope,
    from: &Address,
    to: &Address,
    value_wei: &DecimalString,
    chain_id: u64,
    block_timestamp: u64,
) -> Result<PolicyRequest, LoweringError> {
    let ctx = LoweringCtx {
        from,
        to,
        value_wei,
        chain_id,
        block_timestamp,
    };

    // Every variant of [`Action`] is listed explicitly — no `_ =>` catch-all —
    // so adding a new variant to the enum forces an `unreachable_patterns` /
    // `non_exhaustive_patterns` compile-time decision on whether to wire a new
    // lowering or to extend the `UnsupportedAction` branch below.
    match &envelope.action {
        Action::Swap(action) => Ok(action.build(&ctx)),
        Action::AddLiquidity(action) => Ok(action.build(&ctx)),
        Action::RemoveLiquidity(action) => Ok(action.build(&ctx)),
        Action::MintLiquidityNft(action) => Ok(action.build(&ctx)),
        Action::BurnLiquidityNft(action) => Ok(action.build(&ctx)),
        Action::IncreaseLiquidity(action) => Ok(action.build(&ctx)),
        Action::DecreaseLiquidity(action) => Ok(action.build(&ctx)),
        Action::Supply(action) => Ok(action.build(&ctx)),
        Action::Withdraw(action) => Ok(action.build(&ctx)),
        Action::Borrow(action) => Ok(action.build(&ctx)),
        Action::Repay(action) => Ok(action.build(&ctx)),
        Action::Liquidate(action) => Ok(action.build(&ctx)),
        Action::FlashLoan(action) => Ok(action.build(&ctx)),
        Action::SetAuthorization(action) => Ok(action.build(&ctx)),
        Action::SignAuthorization(action) => Ok(action.build(&ctx)),
        Action::Revoke(action) => Ok(action.build(&ctx)),
        Action::Stake(action) => Ok(action.build(&ctx)),
        Action::RequestUnstake(action) => Ok(action.build(&ctx)),
        Action::ClaimUnstake(action) => Ok(action.build(&ctx)),
        Action::Restake(action) => Ok(action.build(&ctx)),
        Action::RequestRestakeWithdrawal(action) => Ok(action.build(&ctx)),
        Action::ClaimRestakeWithdrawal(action) => Ok(action.build(&ctx)),
        Action::Wrap(_)
        | Action::Unwrap(_)
        | Action::Approve(_)
        | Action::SetApprovalForAll(_)
        | Action::Transfer(_)
        | Action::Permit(_)
        | Action::ClaimRewards(_)
        | Action::SignMessage(_)
        | Action::Delegate(_)
        | Action::Vote(_) => Err(LoweringError::UnsupportedAction {
            kind: envelope.action.kind().to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{policy_request_from_envelope, LoweringError};
    use crate::action::lending::{
        AuthorizationScope, BorrowAction, FlashLoanAction, FlashLoanKind, LiquidateAction,
        LiquidationKind, RepayAction, RepayKind, RevokeAction, RevokeKind, SetAuthorizationAction,
        SignAuthorizationAction, SignAuthorizationScope, SupplyAction, WithdrawAction,
    };
    use crate::action::misc::ClaimRewardsAction;
    use crate::action::restaking::{
        ClaimRestakeWithdrawalAction, RequestRestakeWithdrawalAction, RestakeAction,
    };
    use crate::action::staking::{
        ClaimUnstakeAction, RequestUnstakeAction, StakeAction, TicketRef,
    };
    use crate::action::{
        Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
        Category, DecimalString, Validity, ValiditySource,
    };
    use std::str::FromStr as _;

    fn address(value: &str) -> Address {
        Address::from_str(value).unwrap()
    }

    fn decimal(value: &str) -> DecimalString {
        DecimalString::from_str(value).unwrap()
    }

    fn erc20() -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc20,
            address: Some(address("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")),
            token_id: None,
            symbol: Some("USDC".to_owned()),
            decimals: Some(6),
        }
    }

    fn exact(value: &str) -> AmountConstraint {
        AmountConstraint {
            kind: AmountKind::Exact,
            value: Some(decimal(value)),
        }
    }

    fn empty_ticket() -> TicketRef {
        TicketRef {
            nft: None,
            token_id: None,
            id: None,
        }
    }

    fn validity() -> Validity {
        Validity {
            expires_at: decimal("1700000600"),
            source: ValiditySource::SignatureDeadline,
        }
    }

    fn lower(envelope: &ActionEnvelope) -> Result<crate::policy::PolicyRequest, LoweringError> {
        policy_request_from_envelope(
            envelope,
            &address("0x1111111111111111111111111111111111111111"),
            &address("0x2222222222222222222222222222222222222222"),
            &decimal("0"),
            1,
            1_700_000_000,
        )
    }

    fn envelope(action: Action) -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Lending,
            action,
        }
    }

    fn staking_envelope(action: Action) -> ActionEnvelope {
        ActionEnvelope {
            category: Category::LiquidStaking,
            action,
        }
    }

    fn restaking_envelope(action: Action) -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Restaking,
            action,
        }
    }

    #[test]
    fn dispatch_routes_each_lending_variant_to_its_action_id() {
        let me = address("0x1111111111111111111111111111111111111111");
        let other = address("0x3333333333333333333333333333333333333333");
        let cases: Vec<(Action, &'static str)> = vec![
            (
                Action::Supply(SupplyAction {
                    market: None,
                    asset: erc20(),
                    amount: exact("1000"),
                    amount_mode: None,
                    recipient: me.clone(),
                    from: None,
                    validity: None,
                }),
                "supply",
            ),
            (
                Action::Withdraw(WithdrawAction {
                    market: None,
                    asset: erc20(),
                    amount: exact("1000"),
                    amount_mode: None,
                    recipient: me.clone(),
                    on_behalf: None,
                }),
                "withdraw",
            ),
            (
                Action::Borrow(BorrowAction {
                    market: None,
                    asset: erc20(),
                    amount: exact("1000"),
                    amount_mode: None,
                    recipient: me.clone(),
                    on_behalf: me.clone(),
                    validity: None,
                }),
                "borrow",
            ),
            (
                Action::Repay(RepayAction {
                    market: None,
                    asset: erc20(),
                    amount: exact("1000"),
                    amount_mode: None,
                    on_behalf: me.clone(),
                    repay_kind: RepayKind::DebtAsset,
                    validity: None,
                }),
                "repay",
            ),
            (
                Action::Liquidate(LiquidateAction {
                    market: None,
                    borrower: other.clone(),
                    collateral_asset: None,
                    debt_asset: erc20(),
                    debt_to_cover: None,
                    seized_collateral_amount: None,
                    liquidation_kind: LiquidationKind::PoolShare,
                    liquidate_mode: None,
                    recipient: None,
                    receive_a_token: None,
                }),
                "liquidate",
            ),
            (
                Action::FlashLoan(FlashLoanAction {
                    pool: None,
                    assets: vec![erc20()],
                    amounts: vec![exact("1000")],
                    receiver: me.clone(),
                    on_behalf: None,
                    flash_loan_kind: FlashLoanKind::Simple,
                    fee: None,
                }),
                "flash_loan",
            ),
            (
                Action::SetAuthorization(SetAuthorizationAction {
                    market: None,
                    authorizer: me.clone(),
                    authorized: other.clone(),
                    is_authorized: true,
                    authorization_scope: AuthorizationScope::All,
                    amount: None,
                }),
                "set_authorization",
            ),
            (
                Action::SignAuthorization(SignAuthorizationAction {
                    market: None,
                    authorizer: me.clone(),
                    authorized: other.clone(),
                    is_authorized: true,
                    authorization_scope: SignAuthorizationScope::All,
                    amount: None,
                    nonce: None,
                    validity: validity(),
                }),
                "sign_authorization",
            ),
            (
                Action::Revoke(RevokeAction {
                    target: None,
                    caller: me.clone(),
                    subject: other.clone(),
                    revoke_kind: RevokeKind::Erc20Allowance,
                }),
                "revoke",
            ),
        ];

        for (action, expected_action_id) in cases {
            let envelope = envelope(action);
            let request = lower(&envelope)
                .unwrap_or_else(|error| panic!("{expected_action_id} should lower: {error}"));
            assert_eq!(
                request.action,
                format!(r#"Action::"{expected_action_id}""#),
                "wrong action uid for {expected_action_id}",
            );
        }
    }

    #[test]
    fn dispatch_routes_each_staking_variant_to_its_action_id() {
        let me = address("0x1111111111111111111111111111111111111111");
        let cases: Vec<(Action, &'static str)> = vec![
            (
                Action::Stake(StakeAction {
                    token_in: erc20(),
                    receipt_token: erc20(),
                    amount_in: exact("1000"),
                    amount_out: None,
                    recipient: me.clone(),
                }),
                "stake",
            ),
            (
                Action::RequestUnstake(RequestUnstakeAction {
                    receipt_token: erc20(),
                    token_out: None,
                    amount_in: exact("1000"),
                    amount_out: None,
                    ticket: None,
                    recipient: me.clone(),
                }),
                "request_unstake",
            ),
            (
                Action::ClaimUnstake(ClaimUnstakeAction {
                    token_out: erc20(),
                    amount_out: None,
                    ticket: empty_ticket(),
                    recipient: me,
                }),
                "claim_unstake",
            ),
        ];

        for (action, expected_action_id) in cases {
            let envelope = staking_envelope(action);
            let request = lower(&envelope)
                .unwrap_or_else(|error| panic!("{expected_action_id} should lower: {error}"));
            assert_eq!(
                request.action,
                format!(r#"Action::"{expected_action_id}""#),
                "wrong action uid for {expected_action_id}",
            );
        }
    }

    #[test]
    fn dispatch_routes_each_restaking_variant_to_its_action_id() {
        let me = address("0x1111111111111111111111111111111111111111");
        let cases: Vec<(Action, &'static str)> = vec![
            (
                Action::Restake(RestakeAction {
                    token_in: erc20(),
                    receipt_token: None,
                    amount_in: exact("1000"),
                    amount_out: None,
                    strategy: None,
                    recipient: me.clone(),
                }),
                "restake",
            ),
            (
                Action::RequestRestakeWithdrawal(RequestRestakeWithdrawalAction {
                    token_out: None,
                    receipt_token: None,
                    amount_in: exact("1000"),
                    amount_out: None,
                    strategy: None,
                    ticket: None,
                    recipient: me.clone(),
                }),
                "request_restake_withdrawal",
            ),
            (
                Action::ClaimRestakeWithdrawal(ClaimRestakeWithdrawalAction {
                    token_out: erc20(),
                    amount_out: None,
                    ticket: empty_ticket(),
                    recipient: me,
                }),
                "claim_restake_withdrawal",
            ),
        ];

        for (action, expected_action_id) in cases {
            let envelope = restaking_envelope(action);
            let request = lower(&envelope)
                .unwrap_or_else(|error| panic!("{expected_action_id} should lower: {error}"));
            assert_eq!(
                request.action,
                format!(r#"Action::"{expected_action_id}""#),
                "wrong action uid for {expected_action_id}",
            );
        }
    }

    #[test]
    fn unsupported_action_returns_unsupported_action_error() {
        let envelope = ActionEnvelope {
            category: Category::Misc,
            action: Action::ClaimRewards(ClaimRewardsAction {
                source: None,
                nft: None,
                token_id: None,
                from: Address::from_str("0x1111111111111111111111111111111111111111").unwrap(),
                recipient: Address::from_str("0x2222222222222222222222222222222222222222").unwrap(),
                reward_tokens: None,
                max_amounts: None,
            }),
        };

        let result = policy_request_from_envelope(
            &envelope,
            &Address::from_str("0x1111111111111111111111111111111111111111").unwrap(),
            &Address::from_str("0x2222222222222222222222222222222222222222").unwrap(),
            &DecimalString::from_str("0").unwrap(),
            1,
            1_700_000_000,
        );

        assert_eq!(
            result.unwrap_err(),
            LoweringError::UnsupportedAction {
                kind: "claim_rewards".to_owned(),
            }
        );
    }
}
