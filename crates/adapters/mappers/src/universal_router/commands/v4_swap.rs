//! UR command 0x10 V4_SWAP — input `abi.encode(bytes actions, bytes[] params)`.
//! Each byte in `actions` is a V4 Actions opcode (see uniswap_v4 mappers).

use alloy_primitives::Bytes;
use alloy_sol_types::SolValue;

use crate::context::{BuildContext, RawTx};
use crate::error::MapError;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v4;

pub fn map_command(
    ctx: &BuildContext,
    tx: &RawTx,
    input: &[u8],
) -> Result<Vec<ActionEnvelope>, MapError> {
    let (actions, params): (Bytes, Vec<Bytes>) =
        <(Bytes, Vec<Bytes>)>::abi_decode_sequence(input, true)
            .map_err(|e| MapError::AbiDecode(e.to_string()))?;
    let mut out: Vec<ActionEnvelope> = Vec::new();
    for (i, &a) in actions.iter().enumerate() {
        let p = params.get(i).map(|b| b.as_ref()).unwrap_or(&[]);
        let res = match a {
            0x06 => uniswap_v4::swap_exact_in_single::map_action(ctx, tx, p),
            0x07 => uniswap_v4::swap_exact_in::map_action(ctx, tx, p),
            0x08 => uniswap_v4::swap_exact_out_single::map_action(ctx, tx, p),
            0x09 => uniswap_v4::swap_exact_out::map_action(ctx, tx, p),
            _ => Ok(vec![]), // non-swap V4 actions (SETTLE/TAKE/WRAP/UNWRAP/etc.) — skip
        };
        if let Ok(envs) = res {
            out.extend(envs);
        }
    }
    Ok(out)
}
