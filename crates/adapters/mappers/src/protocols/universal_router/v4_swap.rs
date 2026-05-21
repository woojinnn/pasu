//! UR opcode `V4_SWAP` → one or more `Action::Swap` envelopes.
//!
//! V4_SWAP is itself an opcode stream: the per-opcode `inputs[i]` decodes as
//! `(bytes actions, bytes[] params)`, and the V4Router contract dispatches
//! `actions` byte-by-byte against [`V4_ROUTER_TABLE`]. The splitter
//! pre-decodes only the outer wrapper (actions + params live as bytes args
//! on the SubCall.decoded); this mapper does the inner dispatch and emits
//! one envelope per V4 *swap* action.
//!
//! The two-pass swap-envelope walk (build a `SwapAction` per swap action,
//! then patch its recipient from the trailing `TAKE`) lives in the shared
//! [`v4_swap_builder`](super::v4_swap_builder) module — Phase 7B (TB-2)
//! extracted it so the declarative `opcode_stream_dispatch` path can reuse
//! the identical builder. This module is the *imperative* entrypoint: it
//! pulls `(actions, params)` off a pre-decoded `DecodedCall`, dispatches
//! against `V4_ROUTER_TABLE`, and hands the step list to the builder.

use std::sync::Arc;

use abi_resolver::ids::UR_V4_SWAP_DECODER_ID;
use abi_resolver::subdecode::opcode_stream::dispatch as dispatch_opcodes;
use abi_resolver::subdecode::protocols::v4_router::V4_ROUTER_TABLE;
use abi_resolver::{DecodedCall, DecodedValue, DecoderId};
use policy_engine::action::envelope::ActionEnvelope;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::find_bytes;
use super::v4_swap_builder::build_v4_swap_envelopes;

pub const UR_V4_SWAP_MAPPER_ID: &str = "uniswap-ur/V4_SWAP";

#[derive(Debug, Clone, Copy, Default)]
pub struct UrV4SwapMapper;

impl UrV4SwapMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UrV4SwapMapper {
    fn id(&self) -> MapperId {
        MapperId::new(UR_V4_SWAP_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == UR_V4_SWAP_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let actions = find_bytes(decoded, "actions")?;
        let params = find_bytes_array(decoded, "params")?;
        let steps = dispatch_opcodes(&actions, &params, &V4_ROUTER_TABLE);
        // Shared two-pass swap builder — also used by the declarative
        // `opcode_stream::execute_v4_swap_step`.
        build_v4_swap_envelopes(ctx, &steps)
    }
}

#[must_use]
pub fn v4_swap_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(UR_V4_SWAP_DECODER_ID),
    }
}

#[must_use]
pub fn v4_swap_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(UrV4SwapMapper::new())
}

/// Look up a `bytes[]` arg by name and convert each element to `Vec<u8>`.
/// UR V4_SWAP carries `params: bytes[]` as the second arg.
fn find_bytes_array(decoded: &DecodedCall, name: &str) -> Result<Vec<Vec<u8>>, MapperError> {
    let arg = decoded
        .args
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| MapperError::MissingArgument(name.into()))?;
    let items = match &arg.value {
        DecodedValue::Array(items) => items,
        _ => {
            return Err(MapperError::ArgumentMismatch {
                name: name.into(),
                message: "expected bytes[] array".into(),
            })
        }
    };
    items
        .iter()
        .map(|v| match v {
            DecodedValue::Bytes(b) => Ok(b.clone()),
            _ => Err(MapperError::ArgumentMismatch {
                name: name.into(),
                message: "array entry must be bytes".into(),
            }),
        })
        .collect()
}
