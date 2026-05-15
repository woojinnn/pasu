//! `approve(spender, amount)` → `Action::Approve`.

use std::str::FromStr as _;
use std::sync::Arc;

use abi_resolver::ids::ERC20_APPROVE_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::common::{AmountConstraint, AmountKind, AssetKind, AssetRef, DecimalString};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::misc::{ApprovalKind, ApproveAction};

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::{find_address, find_uint};

pub const APPROVE_MAPPER_ID: &str = "erc20/approve";

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

/// Convenience: the `MapperMatchKey` this Mapper should be registered under.
#[must_use]
pub fn approve_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(ERC20_APPROVE_DECODER_ID),
    }
}

/// Convenience: build an `Arc<dyn Mapper>` for registration.
#[must_use]
pub fn approve_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(Erc20ApproveMapper::new())
}
