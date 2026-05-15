//! `WETH.deposit()` → `Action::Wrap`.

use std::sync::Arc;

use abi_resolver::ids::WETH_DEPOSIT_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::common::{AmountConstraint, AmountKind};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::misc::WrapAction;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::{native_eth, wrapped_weth};

pub const WETH_DEPOSIT_MAPPER_ID: &str = "weth/deposit";

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
