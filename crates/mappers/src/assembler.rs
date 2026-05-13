//! `RootRequest` assembler — composes per-mapper `Vec<ActionEnvelope>` into
//! the final top-level container.
//!
//! This is the only place that fills `schemaVersion`, `requestKind`,
//! `protocol`, and `blockTimestamp`. Per-mapper code never touches these.

use crate::context::{BuildContext, RawTx};
use crate::types::envelope::ActionEnvelope;
use crate::types::root::{ProtocolRef, RequestKind, RootRequest};

pub fn assemble(
    tx: &RawTx,
    ctx: &BuildContext,
    protocol: Option<ProtocolRef>,
    actions: Vec<ActionEnvelope>,
) -> RootRequest {
    let selector = if tx.input.len() >= 4 {
        format!("0x{}", hex_encode4(&tx.input[..4]))
    } else {
        "0x00000000".into()
    };
    RootRequest {
        schema_version: RootRequest::SCHEMA_VERSION.into(),
        request_kind: RequestKind::Transaction,
        chain_id: tx.chain_id,
        from: tx.from.clone(),
        to: tx.to.clone(),
        value: tx.value.clone(),
        selector,
        protocol,
        actions,
        block_timestamp: Some(ctx.block_timestamp as u64),
    }
}

fn hex_encode4(b: &[u8]) -> String {
    let mut s = String::with_capacity(8);
    for byte in b {
        s.push_str(&format!("{:02x}", byte));
    }
    s
}
