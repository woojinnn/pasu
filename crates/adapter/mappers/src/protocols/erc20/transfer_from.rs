//! `transferFrom(from, to, amount)` → `Action::Transfer`.

use std::str::FromStr as _;
use std::sync::Arc;

use abi_resolver::ids::ERC20_TRANSFER_FROM_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::common::{AmountConstraint, AmountKind, AssetKind, AssetRef, DecimalString};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::misc::TransferAction;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::{find_address, find_uint};

pub const TRANSFER_FROM_MAPPER_ID: &str = "erc20/transferFrom";

/// `transferFrom(from, to, amount)` → `Action::Transfer` envelope under `Category::Misc`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Erc20TransferFromMapper;

impl Erc20TransferFromMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for Erc20TransferFromMapper {
    fn id(&self) -> MapperId {
        MapperId::new(TRANSFER_FROM_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == ERC20_TRANSFER_FROM_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let from = find_address(decoded, "from")?;
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
            from,
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

/// Convenience: the `MapperMatchKey` this Mapper should be registered under.
#[must_use]
pub fn transfer_from_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(ERC20_TRANSFER_FROM_DECODER_ID),
    }
}

/// Convenience: build an `Arc<dyn Mapper>` for registration.
#[must_use]
pub fn transfer_from_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(Erc20TransferFromMapper::new())
}
