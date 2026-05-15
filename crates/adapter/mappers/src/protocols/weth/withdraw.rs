//! `WETH.withdraw(uint256)` → `Action::Unwrap`.

use std::str::FromStr as _;
use std::sync::Arc;

use abi_resolver::ids::WETH_WITHDRAW_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::common::{AmountConstraint, AmountKind, DecimalString};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::misc::UnwrapAction;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::{find_uint, native_eth, wrapped_weth};

pub const WETH_WITHDRAW_MAPPER_ID: &str = "weth/withdraw";

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
