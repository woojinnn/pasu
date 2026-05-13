//! ERC-20 mappers. Currently covers `approve(address,uint256)` → `Action::Approve`
//! and `transfer(address,uint256)` → `Action::Transfer`.

use std::str::FromStr as _;
use std::sync::Arc;

use abi_resolver::{DecodedCall, DecodedValue, DecoderId};
use policy_engine::action::common::{
    Address, AmountConstraint, AmountKind, AssetKind, AssetRef, DecimalString,
};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::misc::{ApprovalKind, ApproveAction, TransferAction};

use crate::mapper::{MapContext, Mapper, MapperError, MapperId};

const ERC20_APPROVE_DECODER_ID: &str = "erc20/approve";
const ERC20_TRANSFER_DECODER_ID: &str = "erc20/transfer";
const APPROVE_MAPPER_ID: &str = "erc20/approve";
const TRANSFER_MAPPER_ID: &str = "erc20/transfer";

/// `approve(spender, amount)` → `Action::Approve` envelope under `Category::Misc`.
/// `amount == U256::MAX` is normalised to `AmountKind::Unlimited` per schema
/// guidance; any other value becomes `AmountKind::Exact`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Erc20ApproveMapper;

impl Erc20ApproveMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for Erc20ApproveMapper {
    fn id(&self) -> MapperId {
        MapperId::new(APPROVE_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == ERC20_APPROVE_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let spender = find_address(decoded, "spender")?;
        let amount_u256 = find_uint(decoded, "amount")?;
        let amount = if amount_u256 == alloy_primitives::U256::MAX {
            AmountConstraint {
                kind: AmountKind::Unlimited,
                value: None,
            }
        } else {
            AmountConstraint {
                kind: AmountKind::Exact,
                value: Some(
                    DecimalString::from_str(&amount_u256.to_string())
                        .expect("U256 decimal string is always valid"),
                ),
            }
        };

        let (symbol, decimals) = ctx
            .token_registry
            .lookup(ctx.chain_id, ctx.to)
            .map(|m| (Some(m.symbol), Some(m.decimals)))
            .unwrap_or((None, None));

        let token = AssetRef {
            kind: AssetKind::Erc20,
            chain_id: ctx.chain_id,
            address: Some(ctx.to.clone()),
            symbol,
            decimals,
        };

        let action = ApproveAction {
            token,
            spender,
            spender_label: None,
            amount,
            approval_kind: ApprovalKind::Erc20,
            current_allowance: None,
            validity: None,
        };

        Ok(vec![ActionEnvelope {
            category: Category::Misc,
            action: Action::Approve(action),
        }])
    }
}

/// `transfer(to, amount)` → `Action::Transfer` envelope under `Category::Misc`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Erc20TransferMapper;

impl Erc20TransferMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for Erc20TransferMapper {
    fn id(&self) -> MapperId {
        MapperId::new(TRANSFER_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == ERC20_TRANSFER_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let recipient = find_address(decoded, "to")?;
        let amount_u256 = find_uint(decoded, "amount")?;
        let amount = AmountConstraint {
            kind: AmountKind::Exact,
            value: Some(
                DecimalString::from_str(&amount_u256.to_string())
                    .expect("U256 decimal string is always valid"),
            ),
        };

        let (symbol, decimals) = ctx
            .token_registry
            .lookup(ctx.chain_id, ctx.to)
            .map(|m| (Some(m.symbol), Some(m.decimals)))
            .unwrap_or((None, None));

        let token = AssetRef {
            kind: AssetKind::Erc20,
            chain_id: ctx.chain_id,
            address: Some(ctx.to.clone()),
            symbol,
            decimals,
        };

        let action = TransferAction {
            token,
            from: ctx.from.clone(),
            recipient,
            amount: Some(amount),
            token_id: None,
        };

        Ok(vec![ActionEnvelope {
            category: Category::Misc,
            action: Action::Transfer(action),
        }])
    }
}

fn find_address(decoded: &DecodedCall, name: &str) -> Result<Address, MapperError> {
    decoded
        .args
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| match &a.value {
            DecodedValue::Address(addr) => Some(addr.clone()),
            _ => None,
        })
        .ok_or_else(|| MapperError::MissingArgument(name.into()))
}

fn find_uint(decoded: &DecodedCall, name: &str) -> Result<alloy_primitives::U256, MapperError> {
    decoded
        .args
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| match &a.value {
            DecodedValue::Uint(u) => Some(*u),
            _ => None,
        })
        .ok_or_else(|| MapperError::MissingArgument(name.into()))
}

/// Convenience: the `MapperMatchKey` this Mapper should be registered under.
#[must_use]
pub fn approve_mapper_key() -> crate::mapper::MapperMatchKey {
    crate::mapper::MapperMatchKey {
        decoder_id: DecoderId::new(ERC20_APPROVE_DECODER_ID),
    }
}

/// Convenience: build an `Arc<dyn Mapper>` for registration.
#[must_use]
pub fn approve_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(Erc20ApproveMapper::new())
}

/// Convenience: the `MapperMatchKey` this Mapper should be registered under.
#[must_use]
pub fn transfer_mapper_key() -> crate::mapper::MapperMatchKey {
    crate::mapper::MapperMatchKey {
        decoder_id: DecoderId::new(ERC20_TRANSFER_DECODER_ID),
    }
}

/// Convenience: build an `Arc<dyn Mapper>` for registration.
#[must_use]
pub fn transfer_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(Erc20TransferMapper::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_registry::EmptyTokenRegistry;
    use abi_resolver::DecodedArg;

    fn build_approve_decoded(spender: Address, amount: alloy_primitives::U256) -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new(ERC20_APPROVE_DECODER_ID),
            function_signature: "approve(address,uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "spender".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(spender),
                },
                DecodedArg {
                    name: "amount".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(amount),
                },
            ],
            nested: vec![],
        }
    }

    fn build_transfer_decoded(to: Address, amount: alloy_primitives::U256) -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new(ERC20_TRANSFER_DECODER_ID),
            function_signature: "transfer(address,uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "to".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(to),
                },
                DecodedArg {
                    name: "amount".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(amount),
                },
            ],
            nested: vec![],
        }
    }

    #[test]
    fn unlimited_approve_maps_to_unlimited_kind() {
        let token_registry = EmptyTokenRegistry;
        let from = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        let to = Address::from_str("0xdac17f958d2ee523a2206206994597c13d831ec7").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = MapContext {
            chain_id: 1,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp: None,
            token_registry: &token_registry,
        };
        let spender = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let decoded = build_approve_decoded(spender.clone(), alloy_primitives::U256::MAX);

        let envelopes = Erc20ApproveMapper::new().map(&ctx, &decoded).unwrap();
        assert_eq!(envelopes.len(), 1);
        let Action::Approve(a) = &envelopes[0].action else {
            panic!("expected Approve, got kind={}", envelopes[0].action.kind());
        };
        assert_eq!(envelopes[0].category, Category::Misc);
        assert_eq!(a.amount.kind, AmountKind::Unlimited);
        assert!(a.amount.value.is_none());
        assert_eq!(a.approval_kind, ApprovalKind::Erc20);
        assert_eq!(a.spender, spender);
        assert_eq!(a.token.address.as_ref(), Some(&to));
    }

    #[test]
    fn exact_approve_maps_to_exact_amount() {
        let token_registry = EmptyTokenRegistry;
        let from = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        let to = Address::from_str("0xdac17f958d2ee523a2206206994597c13d831ec7").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = MapContext {
            chain_id: 1,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp: None,
            token_registry: &token_registry,
        };
        let spender = Address::from_str("0x2222222222222222222222222222222222222222").unwrap();
        let decoded = build_approve_decoded(spender, alloy_primitives::U256::from(123_456_789_u64));

        let envelopes = Erc20ApproveMapper::new().map(&ctx, &decoded).unwrap();
        let Action::Approve(a) = &envelopes[0].action else {
            panic!("expected Approve");
        };
        assert_eq!(a.amount.kind, AmountKind::Exact);
        assert_eq!(
            a.amount.value.as_ref().map(ToString::to_string),
            Some("123456789".to_string()),
        );
    }

    #[test]
    fn transfer_maps_to_exact_transfer_action() {
        let token_registry = EmptyTokenRegistry;
        let from = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        let to = Address::from_str("0xdac17f958d2ee523a2206206994597c13d831ec7").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = MapContext {
            chain_id: 1,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp: None,
            token_registry: &token_registry,
        };
        let recipient = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let decoded = build_transfer_decoded(
            recipient.clone(),
            alloy_primitives::U256::from(1_000_000_000_000_u64),
        );

        let envelopes = Erc20TransferMapper::new().map(&ctx, &decoded).unwrap();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].category, Category::Misc);
        let Action::Transfer(a) = &envelopes[0].action else {
            panic!("expected Transfer, got kind={}", envelopes[0].action.kind());
        };
        assert_eq!(a.token.kind, AssetKind::Erc20);
        assert_eq!(a.token.address.as_ref(), Some(&to));
        assert_eq!(a.from, from);
        assert_eq!(a.recipient, recipient);
        assert!(a.token_id.is_none());
        let amount = a.amount.as_ref().expect("ERC-20 transfer amount");
        assert_eq!(amount.kind, AmountKind::Exact);
        assert_eq!(
            amount.value.as_ref().map(ToString::to_string),
            Some("1000000000000".to_string()),
        );
    }
}
