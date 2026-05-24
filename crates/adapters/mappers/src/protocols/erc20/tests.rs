use std::str::FromStr as _;

use abi_resolver::ids::{
    ERC20_APPROVE_DECODER_ID, ERC20_TRANSFER_DECODER_ID, ERC20_TRANSFER_FROM_DECODER_ID,
    SET_APPROVAL_FOR_ALL_DECODER_ID,
};
use abi_resolver::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
use policy_engine::action::common::{Address, AmountKind, AssetKind, DecimalString};
use policy_engine::action::envelope::{Action, Category};
use policy_engine::action::misc::ApprovalKind;

use crate::token_registry::EmptyTokenRegistry;
use crate::{MapContext, Mapper as _};

use super::approve::Erc20ApproveMapper;
use super::set_approval_for_all::SetApprovalForAllMapper;
use super::transfer::Erc20TransferMapper;
use super::transfer_from::Erc20TransferFromMapper;

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

fn build_transfer_from_decoded(
    from: Address,
    to: Address,
    amount: alloy_primitives::U256,
) -> DecodedCall {
    DecodedCall {
        decoder_id: DecoderId::new(ERC20_TRANSFER_FROM_DECODER_ID),
        function_signature: "transferFrom(address,address,uint256)".into(),
        args: vec![
            DecodedArg {
                name: "from".into(),
                abi_type: "address".into(),
                value: DecodedValue::Address(from),
            },
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

fn build_set_approval_for_all_decoded(operator: Address, approved: bool) -> DecodedCall {
    DecodedCall {
        decoder_id: DecoderId::new(SET_APPROVAL_FOR_ALL_DECODER_ID),
        function_signature: "setApprovalForAll(address,bool)".into(),
        args: vec![
            DecodedArg {
                name: "operator".into(),
                abi_type: "address".into(),
                value: DecodedValue::Address(operator),
            },
            DecodedArg {
                name: "approved".into(),
                abi_type: "bool".into(),
                value: DecodedValue::Bool(approved),
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
        parent_calldata: None,
        depth: 0,
        resolver: None,
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
        parent_calldata: None,
        depth: 0,
        resolver: None,
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
        parent_calldata: None,
        depth: 0,
        resolver: None,
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
    assert_eq!(a.token.asset.kind, AssetKind::Erc20);
    assert_eq!(a.token.asset.address.as_ref(), Some(&to));
    assert_eq!(a.from, from);
    assert_eq!(a.recipient, recipient);
    assert!(a.token.asset.token_id.is_none());
    let amount = &a.token.amount;
    assert_eq!(amount.kind, AmountKind::Exact);
    assert_eq!(
        amount.value.as_ref().map(ToString::to_string),
        Some("1000000000000".to_string()),
    );
}

#[test]
fn transfer_from_maps_to_transfer_envelope() {
    let token_registry = EmptyTokenRegistry;
    let tx_sender = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
    let token = Address::from_str("0xdac17f958d2ee523a2206206994597c13d831ec7").unwrap();
    let value = DecimalString::from_str("0").unwrap();
    let ctx = MapContext {
        chain_id: 1,
        from: &tx_sender,
        to: &token,
        value_wei: &value,
        block_timestamp: None,
        token_registry: &token_registry,
        parent_calldata: None,
        depth: 0,
        resolver: None,
    };
    let owner = Address::from_str("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
    let recipient = Address::from_str("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
    let decoded = build_transfer_from_decoded(
        owner.clone(),
        recipient.clone(),
        alloy_primitives::U256::from(1_000_000_u64),
    );

    let envelopes = Erc20TransferFromMapper::new().map(&ctx, &decoded).unwrap();
    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].category, Category::Misc);
    assert_eq!(envelopes[0].action.kind(), "transfer");
    let Action::Transfer(a) = &envelopes[0].action else {
        panic!("expected Transfer, got kind={}", envelopes[0].action.kind());
    };
    assert_eq!(a.token.asset.kind, AssetKind::Erc20);
    assert_eq!(a.token.asset.address.as_ref(), Some(&token));
    assert_eq!(a.from, owner);
    assert_ne!(a.from, tx_sender);
    assert_eq!(a.recipient, recipient);
    assert!(a.token.asset.token_id.is_none());
    let amount = &a.token.amount;
    assert_eq!(amount.kind, AmountKind::Exact);
    assert_eq!(
        amount.value.as_ref().map(ToString::to_string),
        Some("1000000".to_string()),
    );
}

#[test]
fn set_approval_for_all_maps_to_set_approval_for_all_envelope() {
    let token_registry = EmptyTokenRegistry;
    let from = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
    let collection = Address::from_str("0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d").unwrap();
    let value = DecimalString::from_str("0").unwrap();
    let ctx = MapContext {
        chain_id: 1,
        from: &from,
        to: &collection,
        value_wei: &value,
        block_timestamp: None,
        token_registry: &token_registry,
        parent_calldata: None,
        depth: 0,
        resolver: None,
    };
    let operator = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
    let decoded = build_set_approval_for_all_decoded(operator.clone(), true);

    let envelopes = SetApprovalForAllMapper::new().map(&ctx, &decoded).unwrap();
    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].category, Category::Misc);
    assert_eq!(envelopes[0].action.kind(), "set_approval_for_all");
    let Action::SetApprovalForAll(action) = &envelopes[0].action else {
        panic!(
            "expected SetApprovalForAll, got kind={}",
            envelopes[0].action.kind()
        );
    };
    assert_eq!(action.collection.kind, AssetKind::Erc721);
    assert_eq!(action.collection.address.as_ref(), Some(&collection));
    assert_eq!(action.operator, operator);
    assert!(action.approved);
    assert!(action.previously_approved.is_none());
}
