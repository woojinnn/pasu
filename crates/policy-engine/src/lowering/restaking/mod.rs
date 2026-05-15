//! Per-action lowering for restaking actions.
//!
//! Each submodule provides an `impl Lower for <Action>` so the dispatcher in
//! [`crate::lowering::dispatch`] can call `action.build(&ctx)` uniformly.

pub(crate) mod claim_restake_withdrawal;
pub(crate) mod common;
pub(crate) mod request_restake_withdrawal;
pub(crate) mod restake;

#[cfg(test)]
pub(crate) mod test_support {
    use std::str::FromStr as _;

    use crate::action::restaking::StrategyRef;
    use crate::action::staking::TicketRef;
    use crate::action::{
        Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
        Category, DecimalString, Hex,
    };
    use crate::policy::PolicyRequest;

    pub(crate) const BLOCK_TIMESTAMP: u64 = 1_700_000_000;

    pub(crate) fn address(value: &str) -> Address {
        Address::from_str(value).unwrap()
    }

    pub(crate) fn decimal(value: &str) -> DecimalString {
        DecimalString::from_str(value).unwrap()
    }

    pub(crate) fn hex(value: &str) -> Hex {
        Hex::from_str(value).unwrap()
    }

    pub(crate) fn native(symbol: &str) -> AssetRef {
        AssetRef {
            kind: AssetKind::Native,
            address: None,
            token_id: None,
            symbol: Some(symbol.to_owned()),
            decimals: Some(18),
        }
    }

    pub(crate) fn erc20(address_value: &str, symbol: &str, decimals: u8) -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc20,
            address: Some(address(address_value)),
            token_id: None,
            symbol: Some(symbol.to_owned()),
            decimals: Some(decimals),
        }
    }

    pub(crate) fn amount(kind: AmountKind, value: &str) -> AmountConstraint {
        AmountConstraint {
            kind,
            value: Some(decimal(value)),
        }
    }

    pub(crate) fn strategy() -> StrategyRef {
        StrategyRef {
            address: Some(address("0x4444444444444444444444444444444444444444")),
            id: Some(hex(
                "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            )),
            label: Some("EigenLayer ezETH".to_owned()),
        }
    }

    pub(crate) fn empty_ticket() -> TicketRef {
        TicketRef {
            nft: None,
            token_id: None,
            id: None,
        }
    }

    pub(crate) fn envelope(action: Action) -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Restaking,
            action,
        }
    }

    pub(crate) fn policy_request(envelope: &ActionEnvelope, from: &Address) -> PolicyRequest {
        crate::lowering::policy_request_from_envelope(
            envelope,
            from,
            &address("0x2222222222222222222222222222222222222222"),
            &decimal("0"),
            1,
            BLOCK_TIMESTAMP,
        )
        .expect("Restaking envelope lowers to policy request")
    }
}
