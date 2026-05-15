//! Per-action lowering for lending actions.
//!
//! Each submodule provides an `impl Lower for <Action>` so the dispatcher in
//! [`crate::lowering::dispatch`] can call `action.build(&ctx)` uniformly.

pub(crate) mod borrow;
pub(crate) mod common;
pub(crate) mod supply;
pub(crate) mod withdraw;

#[cfg(test)]
pub(crate) mod test_support {
    use std::str::FromStr as _;

    use crate::action::lending::{ContractRef, MarketRef};
    use crate::action::{
        Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
        Category, DecimalString, Hex, Validity, ValiditySource,
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

    pub(crate) fn market() -> MarketRef {
        MarketRef {
            address: Some(address("0x1111111111111111111111111111111111111111")),
            id: Some(hex(
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )),
            label: Some("Aave V3 USDC".to_owned()),
        }
    }

    pub(crate) fn contract_ref() -> ContractRef {
        ContractRef {
            address: Some(address("0x4444444444444444444444444444444444444444")),
            label: Some("Pool".to_owned()),
        }
    }

    pub(crate) fn validity(expires_at: u64) -> Validity {
        Validity {
            expires_at: decimal(&expires_at.to_string()),
            source: ValiditySource::SignatureDeadline,
        }
    }

    pub(crate) fn envelope(action: Action) -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Lending,
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
        .expect("Lending envelope lowers to policy request")
    }
}
