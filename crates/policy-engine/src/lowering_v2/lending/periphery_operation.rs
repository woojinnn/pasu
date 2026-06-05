//! `Lending::PeripheryOperation` lowering -> `Lending::PeripheryOperationContext`.

use serde_json::{Map, Value};

use policy_transition::action::lending::{LendingPeripheryKind, PeripheryOperationAction};

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_lending_venue;

const fn periphery_kind(kind: &LendingPeripheryKind) -> &'static str {
    match kind {
        LendingPeripheryKind::SwapCollateral => "swap_collateral",
        LendingPeripheryKind::RepayWithCollateral => "repay_with_collateral",
        LendingPeripheryKind::DebtSwap => "debt_swap",
        LendingPeripheryKind::Migration => "migration",
        LendingPeripheryKind::WithdrawSwap => "withdraw_swap",
        LendingPeripheryKind::Raw => "raw",
    }
}

/// Lower a high-risk Aave periphery operation. Adapter-side callback and route
/// effects are intentionally represented as audit context, not simulated state.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &PeripheryOperationAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_lending_venue(&action.venue));
    m.insert("adapter".into(), Value::String(addr(&action.adapter)));
    m.insert(
        "kind".into(),
        Value::String(periphery_kind(&action.kind).into()),
    );
    if let Some(asset_in) = &action.asset_in {
        m.insert("assetIn".into(), lower_token_ref(asset_in));
    }
    if let Some(asset_out) = &action.asset_out {
        m.insert("assetOut".into(), lower_token_ref(asset_out));
    }
    if let Some(amount) = action.amount {
        m.insert("amount".into(), Value::String(u256_hex(amount)));
    }
    if let Some(limit_amount) = action.limit_amount {
        m.insert("limitAmount".into(), Value::String(u256_hex(limit_amount)));
    }
    if let Some(user) = &action.user {
        m.insert("user".into(), Value::String(addr(user)));
    }
    if let Some(recipient) = &action.recipient {
        m.insert("recipient".into(), Value::String(addr(recipient)));
    }
    m.insert("calldata".into(), Value::String(action.calldata.clone()));

    Ok(ctx.lowered(r#"Lending::Action::"PeripheryOperation""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, ChainId, U256};
    use policy_transition::action::lending::{
        LendingAction, LendingPeripheryKind, LendingVenue, PeripheryOperationAction,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{assert_conforms, onchain_meta, other, usdc};

    #[test]
    fn periphery_operation_lowering_conforms_to_schema() {
        let adapter = Address::from_str("0xadc0a53095a0af87f3aa29fe0715b5c28016364e").unwrap();
        let body = ActionBody::Lending(LendingAction::PeripheryOperation(
            PeripheryOperationAction {
                venue: LendingVenue::AaveV3Periphery {
                    chain: ChainId::ethereum_mainnet(),
                    adapter,
                },
                adapter,
                kind: LendingPeripheryKind::SwapCollateral,
                asset_in: Some(usdc()),
                asset_out: Some(usdc()),
                amount: Some(U256::from(1_000_000u64)),
                limit_amount: Some(U256::from(999_000u64)),
                user: Some(other()),
                recipient: Some(other()),
                calldata: "0x1234".to_owned(),
            },
        ));
        assert_conforms("periphery_operation", &body, &onchain_meta());
    }
}
