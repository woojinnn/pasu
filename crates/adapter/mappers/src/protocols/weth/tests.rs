use std::str::FromStr as _;

use abi_resolver::ids::{WETH_DEPOSIT_DECODER_ID, WETH_WITHDRAW_DECODER_ID};
use abi_resolver::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
use alloy_primitives::U256;
use policy_engine::action::common::{Address, AmountKind, AssetKind, DecimalString};
use policy_engine::action::envelope::{Action, Category};

use crate::token_registry::EmptyTokenRegistry;
use crate::Mapper as _;

use super::deposit::WethDepositMapper;
use super::withdraw::WethWithdrawMapper;

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
