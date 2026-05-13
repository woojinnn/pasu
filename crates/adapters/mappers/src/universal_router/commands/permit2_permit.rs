//! UR commands 0x02/0x03/0x0a/0x0d (Permit2 family).
//!
//! These are signature-side: they grant token allowance via Permit2 rather
//! than executing a swap. For the policy pipeline they typically emit an
//! `ApproveAction` — but the full Permit2 struct decoding is non-trivial
//! and lives in the dedicated `permit2` adapter. For now we emit no
//! envelope; the host-side signature pre-pass handles permit semantics.

use crate::context::{BuildContext, RawTx};
use crate::error::MapError;
use crate::types::envelope::ActionEnvelope;

pub fn map_command(
    _ctx: &BuildContext,
    _tx: &RawTx,
    _input: &[u8],
) -> Result<Vec<ActionEnvelope>, MapError> {
    Ok(vec![])
}
