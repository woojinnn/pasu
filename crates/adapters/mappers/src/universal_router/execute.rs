//! Universal Router outer dispatcher.
//!
//! Selectors:
//!   - `execute(bytes,bytes[])`            → 0x24856bc3
//!   - `execute(bytes,bytes[],uint256)`    → 0x3593564c
//!
//! Layout (after selector):
//!   word0 = offset to commands
//!   word1 = offset to inputs array
//!   [word2 = deadline]
//!   at commands offset: length(32) || bytes
//!   at inputs offset:   length(32) || [N offsets] || N items(length(32)||bytes)

use crate::context::{BuildContext, RawTx};
use crate::error::MapError;
use crate::types::envelope::ActionEnvelope;
use crate::universal_router::commands;

pub const SELECTOR_2ARGS: [u8; 4] = [0x24, 0x85, 0x6b, 0xc3];
pub const SELECTOR_3ARGS: [u8; 4] = [0x35, 0x93, 0x56, 0x4c];

pub fn map(ctx: &BuildContext, tx: &RawTx) -> Result<Vec<ActionEnvelope>, MapError> {
    if tx.input.len() < 4 + 64 {
        return Err(MapError::TooShort {
            need: 68,
            got: tx.input.len(),
        });
    }
    let body = &tx.input[4..];
    let (commands_bytes, inputs_vec) = decode_outer(body)?;

    let mut out: Vec<ActionEnvelope> = Vec::new();
    for (i, &cmd) in commands_bytes.iter().enumerate() {
        let cmd_id = cmd & 0x3F;
        let input = inputs_vec.get(i).map(|v| v.as_slice()).unwrap_or(&[]);
        let res = match cmd_id {
            0x00 => commands::v3_swap_in::map_command(ctx, tx, input),
            0x01 => commands::v3_swap_out::map_command(ctx, tx, input),
            0x08 => commands::v2_swap_in::map_command(ctx, tx, input),
            0x09 => commands::v2_swap_out::map_command(ctx, tx, input),
            0x10 => commands::v4_swap::map_command(ctx, tx, input),
            0x0b => commands::wrap_eth::map_command(ctx, tx, input),
            0x0c => commands::unwrap_weth::map_command(ctx, tx, input),
            0x0a | 0x02 | 0x03 | 0x0d => commands::permit2_permit::map_command(ctx, tx, input),
            _ => Ok(vec![]),
        };
        match res {
            Ok(envs) => out.extend(envs),
            Err(_) => continue,
        }
    }
    Ok(out)
}

fn read_u256_be_to_usize(buf: &[u8], off: usize) -> Result<usize, MapError> {
    if off + 32 > buf.len() {
        return Err(MapError::TooShort {
            need: off + 32,
            got: buf.len(),
        });
    }
    let mut hi = 0u128;
    let mut lo = 0u128;
    for &b in &buf[off..off + 16] {
        hi = (hi << 8) | b as u128;
    }
    for &b in &buf[off + 16..off + 32] {
        lo = (lo << 8) | b as u128;
    }
    if hi != 0 {
        return Err(MapError::AbiDecode("offset/length > u128".into()));
    }
    if lo > usize::MAX as u128 {
        return Err(MapError::AbiDecode("offset/length > usize".into()));
    }
    Ok(lo as usize)
}

fn decode_outer(body: &[u8]) -> Result<(Vec<u8>, Vec<Vec<u8>>), MapError> {
    let cmd_off = read_u256_be_to_usize(body, 0)?;
    let inputs_off = read_u256_be_to_usize(body, 32)?;

    let cmd_len = read_u256_be_to_usize(body, cmd_off)?;
    if cmd_off + 32 + cmd_len > body.len() {
        return Err(MapError::TooShort {
            need: cmd_off + 32 + cmd_len,
            got: body.len(),
        });
    }
    let commands_bytes = body[cmd_off + 32..cmd_off + 32 + cmd_len].to_vec();

    let n = read_u256_be_to_usize(body, inputs_off)?;
    let arr_base = inputs_off + 32;
    let mut inputs = Vec::with_capacity(n);
    for i in 0..n {
        let rel = read_u256_be_to_usize(body, arr_base + i * 32)?;
        let abs = arr_base + rel;
        let item_len = read_u256_be_to_usize(body, abs)?;
        if abs + 32 + item_len > body.len() {
            return Err(MapError::TooShort {
                need: abs + 32 + item_len,
                got: body.len(),
            });
        }
        inputs.push(body[abs + 32..abs + 32 + item_len].to_vec());
    }
    Ok((commands_bytes, inputs))
}
