//! `setApprovalForAll(operator, approved)` → `Action::SetApprovalForAll`.

use std::sync::Arc;

use abi_resolver::ids::SET_APPROVAL_FOR_ALL_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::common::{AssetKind, AssetRef};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::misc::SetApprovalForAllAction;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::{find_address, find_bool};

pub const SET_APPROVAL_FOR_ALL_MAPPER_ID: &str = "erc/setApprovalForAll";

/// `setApprovalForAll(operator, approved)` → `Action::SetApprovalForAll`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SetApprovalForAllMapper;

impl SetApprovalForAllMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for SetApprovalForAllMapper {
    fn id(&self) -> MapperId {
        MapperId::new(SET_APPROVAL_FOR_ALL_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == SET_APPROVAL_FOR_ALL_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let operator = find_address(decoded, "operator")?;
        let approved = find_bool(decoded, "approved")?;
        let collection = AssetRef {
            kind: AssetKind::Erc721,
            chain_id: ctx.chain_id,
            address: Some(ctx.to.clone()),
            symbol: None,
            decimals: None,
        };

        let action = SetApprovalForAllAction {
            collection,
            operator,
            operator_label: None,
            approved,
            previously_approved: None,
        };

        Ok(vec![ActionEnvelope {
            category: Category::Misc,
            action: Action::SetApprovalForAll(action),
        }])
    }
}

/// Convenience: the `MapperMatchKey` this Mapper should be registered under.
#[must_use]
pub fn set_approval_for_all_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(SET_APPROVAL_FOR_ALL_DECODER_ID),
    }
}

/// Convenience: build an `Arc<dyn Mapper>` for registration.
#[must_use]
pub fn set_approval_for_all_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(SetApprovalForAllMapper::new())
}
