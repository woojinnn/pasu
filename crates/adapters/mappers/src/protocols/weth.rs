use std::str::FromStr as _;
use std::sync::Arc;

use abi_resolver::decoders::weth::{WETH_DEPOSIT_DECODER_ID, WETH_WITHDRAW_DECODER_ID};
use abi_resolver::{DecodedCall, DecodedValue, DecoderId};
use policy_engine::action::common::{
    AmountConstraint, AmountKind, AssetKind, AssetRef, DecimalString,
};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::misc::{UnwrapAction, WrapAction};

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

pub const WETH_DEPOSIT_MAPPER_ID: &str = "weth/deposit";
pub const WETH_WITHDRAW_MAPPER_ID: &str = "weth/withdraw";

#[derive(Debug, Clone, Copy, Default)]
pub struct WethDepositMapper;

impl WethDepositMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for WethDepositMapper {
    fn id(&self) -> MapperId {
        MapperId::new(WETH_DEPOSIT_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == WETH_DEPOSIT_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        _decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let action = WrapAction {
            native_asset: native_eth(ctx.chain_id),
            wrapped_asset: wrapped_weth(ctx),
            amount: AmountConstraint {
                kind: AmountKind::Exact,
                value: Some(ctx.value_wei.clone()),
            },
            recipient: ctx.from.clone(),
        };

        Ok(vec![ActionEnvelope {
            category: Category::Misc,
            action: Action::Wrap(action),
        }])
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WethWithdrawMapper;

impl WethWithdrawMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for WethWithdrawMapper {
    fn id(&self) -> MapperId {
        MapperId::new(WETH_WITHDRAW_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == WETH_WITHDRAW_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let wad = find_uint(decoded, "wad")?;
        let action = UnwrapAction {
            wrapped_asset: wrapped_weth(ctx),
            native_asset: native_eth(ctx.chain_id),
            amount: AmountConstraint {
                kind: AmountKind::Exact,
                value: Some(
                    DecimalString::from_str(&wad.to_string())
                        .expect("U256 decimal string is always valid"),
                ),
            },
            recipient: ctx.from.clone(),
        };

        Ok(vec![ActionEnvelope {
            category: Category::Misc,
            action: Action::Unwrap(action),
        }])
    }
}

fn native_eth(chain_id: u64) -> AssetRef {
    AssetRef {
        kind: AssetKind::Native,
        chain_id,
        address: None,
        symbol: Some("ETH".to_owned()),
        decimals: Some(18),
    }
}

fn wrapped_weth(ctx: &MapContext<'_>) -> AssetRef {
    AssetRef {
        kind: AssetKind::Erc20,
        chain_id: ctx.chain_id,
        address: Some(ctx.to.clone()),
        symbol: Some("WETH".to_owned()),
        decimals: Some(18),
    }
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

#[must_use]
pub fn deposit_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(WETH_DEPOSIT_DECODER_ID),
    }
}

#[must_use]
pub fn deposit_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(WethDepositMapper::new())
}

#[must_use]
pub fn withdraw_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(WETH_WITHDRAW_DECODER_ID),
    }
}

#[must_use]
pub fn withdraw_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(WethWithdrawMapper::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_registry::EmptyTokenRegistry;
    use abi_resolver::decoders::weth::{WETH_DEPOSIT_DECODER_ID, WETH_WITHDRAW_DECODER_ID};
    use abi_resolver::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
    use alloy_primitives::U256;
    use policy_engine::action::common::{
        Address, AmountKind, AssetKind, DecimalString,
    };
    use policy_engine::action::envelope::{Action, Category};

    fn deposit_decoded() -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new(WETH_DEPOSIT_DECODER_ID),
            function_signature: "deposit()".to_owned(),
            args: vec![],
            nested: vec![],
        }
    }

    fn withdraw_decoded(wad: U256) -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new(WETH_WITHDRAW_DECODER_ID),
            function_signature: "withdraw(uint256)".to_owned(),
            args: vec![DecodedArg {
                name: "wad".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(wad),
            }],
            nested: vec![],
        }
    }

    #[test]
    fn wrap_action_built_from_deposit() {
        let token_registry = EmptyTokenRegistry;
        let from = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        let weth = Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap();
        let value = DecimalString::from_str("1000000000000000000").unwrap();
        let ctx = crate::MapContext {
            chain_id: 1,
            from: &from,
            to: &weth,
            value_wei: &value,
            block_timestamp: None,
            token_registry: &token_registry,
        };

        let envelopes = WethDepositMapper::new()
            .map(&ctx, &deposit_decoded())
            .unwrap();

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].category, Category::Misc);
        assert_eq!(envelopes[0].action.kind(), "wrap");
        let Action::Wrap(wrap) = &envelopes[0].action else {
            panic!("expected Wrap, got kind={}", envelopes[0].action.kind());
        };
        assert_eq!(wrap.native_asset.kind, AssetKind::Native);
        assert_eq!(wrap.native_asset.chain_id, 1);
        assert!(wrap.native_asset.address.is_none());
        assert_eq!(wrap.native_asset.symbol.as_deref(), Some("ETH"));
        assert_eq!(wrap.native_asset.decimals, Some(18));
        assert_eq!(wrap.wrapped_asset.kind, AssetKind::Erc20);
        assert_eq!(wrap.wrapped_asset.chain_id, 1);
        assert_eq!(wrap.wrapped_asset.address.as_ref(), Some(&weth));
        assert_eq!(wrap.wrapped_asset.symbol.as_deref(), Some("WETH"));
        assert_eq!(wrap.wrapped_asset.decimals, Some(18));
        assert_eq!(wrap.amount.kind, AmountKind::Exact);
        assert_eq!(wrap.amount.value.as_ref(), Some(&value));
        assert_eq!(wrap.recipient, from);
        assert_ne!(wrap.recipient, weth);
    }

    #[test]
    fn unwrap_action_built_from_withdraw() {
        let token_registry = EmptyTokenRegistry;
        let from = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        let weth = Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = crate::MapContext {
            chain_id: 1,
            from: &from,
            to: &weth,
            value_wei: &value,
            block_timestamp: None,
            token_registry: &token_registry,
        };
        let wad = U256::from(500_000_000_000_000_000_u64);

        let envelopes = WethWithdrawMapper::new()
            .map(&ctx, &withdraw_decoded(wad))
            .unwrap();

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].category, Category::Misc);
        assert_eq!(envelopes[0].action.kind(), "unwrap");
        let Action::Unwrap(unwrap) = &envelopes[0].action else {
            panic!("expected Unwrap, got kind={}", envelopes[0].action.kind());
        };
        assert_eq!(unwrap.wrapped_asset.kind, AssetKind::Erc20);
        assert_eq!(unwrap.wrapped_asset.chain_id, 1);
        assert_eq!(unwrap.wrapped_asset.address.as_ref(), Some(&weth));
        assert_eq!(unwrap.wrapped_asset.symbol.as_deref(), Some("WETH"));
        assert_eq!(unwrap.wrapped_asset.decimals, Some(18));
        assert_eq!(unwrap.native_asset.kind, AssetKind::Native);
        assert_eq!(unwrap.native_asset.chain_id, 1);
        assert!(unwrap.native_asset.address.is_none());
        assert_eq!(unwrap.native_asset.symbol.as_deref(), Some("ETH"));
        assert_eq!(unwrap.native_asset.decimals, Some(18));
        assert_eq!(unwrap.amount.kind, AmountKind::Exact);
        assert_eq!(
            unwrap.amount.value.as_ref().map(ToString::to_string),
            Some("500000000000000000".to_owned())
        );
        assert_eq!(unwrap.recipient, from);
        assert_ne!(unwrap.recipient, weth);
    }
}
