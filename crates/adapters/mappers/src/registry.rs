//! Dispatch: `(to, selector)` → mapper function.

use abi_resolver::decode::DecodedCall;

use crate::context::{BuildContext, RawTx};
use crate::error::MapError;
use crate::swap_router_02::{
    exact_input as sr02_in, exact_input_single as sr02_in_single, exact_output as sr02_out,
    exact_output_single as sr02_out_single,
};
use crate::types::envelope::ActionEnvelope;
use crate::types::root::ProtocolRef;
use crate::uniswap_v2::{
    swap_eth_for_exact_tokens, swap_exact_eth_for_tokens,
    swap_exact_eth_for_tokens_supporting_fee_on_transfer_tokens, swap_exact_tokens_for_eth,
    swap_exact_tokens_for_eth_supporting_fee_on_transfer_tokens, swap_exact_tokens_for_tokens,
    swap_exact_tokens_for_tokens_supporting_fee_on_transfer_tokens, swap_tokens_for_exact_eth,
    swap_tokens_for_exact_tokens,
};
use crate::uniswap_v3::{exact_input, exact_input_single, exact_output, exact_output_single};
use crate::universal_router::execute as ur_execute;

const V2_ROUTER_LC: &str = "0x7a250d5630b4cf539739df2c5dacb4c659f2488d";
const V3_ROUTER_LC: &str = "0xe592427a0aece92de3edee1f18e0157c05861564";
const SR02_LC: &str = "0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45";

/// Universal Router deployments on Ethereum mainnet. Mirror the list in
/// `abi-resolver::subdecode::protocols::universal_router::UNISWAP_UR_ADDRESSES`.
/// Add new addresses here when abi-resolver picks them up.
const UR_ADDRESSES_LC: &[&str] = &[
    // Original Universal Router (in Sourcify curated bundle)
    "0x66a9893cc07d91d95644aedd05d03f95e1dba8af",
    // V4-supporting Universal Router (Etherscan-verified)
    "0x4c82d1fbfe28c977cbb58d8c7ff8fcf9f70a2cca",
    // Pre-V4 Universal Router (still receives traffic)
    "0x3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad",
];

fn is_universal_router(to_lc: &str) -> bool {
    UR_ADDRESSES_LC.contains(&to_lc)
}

pub fn protocol_for(to_lc: &str) -> Option<ProtocolRef> {
    if to_lc == V2_ROUTER_LC {
        Some(ProtocolRef {
            name: "uniswap".into(),
            version: Some("v2".into()),
            component: Some("router".into()),
        })
    } else if to_lc == V3_ROUTER_LC {
        Some(ProtocolRef {
            name: "uniswap".into(),
            version: Some("v3".into()),
            component: Some("router".into()),
        })
    } else if to_lc == SR02_LC {
        Some(ProtocolRef {
            name: "uniswap".into(),
            version: Some("swap-router-02".into()),
            component: Some("router".into()),
        })
    } else if is_universal_router(to_lc) {
        Some(ProtocolRef {
            name: "uniswap".into(),
            version: Some("universal-router".into()),
            component: Some("router".into()),
        })
    } else {
        None
    }
}

/// Dispatch entry point. The `call` argument is the abi-resolver-produced
/// `DecodedCall` for `tx.input`. Migrated mappers consume `call` directly;
/// legacy mappers ignore it and continue to `sol!`-decode `tx.input`
/// themselves until their migration lands.
pub fn dispatch(
    ctx: &BuildContext,
    tx: &RawTx,
    call: &DecodedCall,
) -> Result<Vec<ActionEnvelope>, MapError> {
    if tx.input.len() < 4 {
        return Err(MapError::TooShort {
            need: 4,
            got: tx.input.len(),
        });
    }
    let sel = [tx.input[0], tx.input[1], tx.input[2], tx.input[3]];
    let to_lc = tx.to.to_lowercase();

    if to_lc == V2_ROUTER_LC {
        if sel == swap_exact_tokens_for_tokens::SELECTOR {
            return swap_exact_tokens_for_tokens::map(ctx, tx, call);
        }
        if sel == swap_tokens_for_exact_tokens::SELECTOR {
            return swap_tokens_for_exact_tokens::map(ctx, tx, call);
        }
        if sel == swap_exact_eth_for_tokens::SELECTOR {
            return swap_exact_eth_for_tokens::map(ctx, tx, call);
        }
        if sel == swap_tokens_for_exact_eth::SELECTOR {
            return swap_tokens_for_exact_eth::map(ctx, tx, call);
        }
        if sel == swap_exact_tokens_for_eth::SELECTOR {
            return swap_exact_tokens_for_eth::map(ctx, tx, call);
        }
        if sel == swap_eth_for_exact_tokens::SELECTOR {
            return swap_eth_for_exact_tokens::map(ctx, tx, call);
        }
        if sel == swap_exact_tokens_for_tokens_supporting_fee_on_transfer_tokens::SELECTOR {
            return swap_exact_tokens_for_tokens_supporting_fee_on_transfer_tokens::map(
                ctx, tx, call,
            );
        }
        if sel == swap_exact_eth_for_tokens_supporting_fee_on_transfer_tokens::SELECTOR {
            return swap_exact_eth_for_tokens_supporting_fee_on_transfer_tokens::map(ctx, tx, call);
        }
        if sel == swap_exact_tokens_for_eth_supporting_fee_on_transfer_tokens::SELECTOR {
            return swap_exact_tokens_for_eth_supporting_fee_on_transfer_tokens::map(ctx, tx, call);
        }
    }

    if to_lc == V3_ROUTER_LC {
        if sel == exact_input_single::SELECTOR {
            return exact_input_single::map(ctx, tx, call);
        }
        if sel == exact_input::SELECTOR {
            return exact_input::map(ctx, tx, call);
        }
        if sel == exact_output_single::SELECTOR {
            return exact_output_single::map(ctx, tx, call);
        }
        if sel == exact_output::SELECTOR {
            return exact_output::map(ctx, tx, call);
        }
    }

    if to_lc == SR02_LC {
        if sel == sr02_in_single::SELECTOR {
            return sr02_in_single::map(ctx, tx, call);
        }
        if sel == sr02_in::SELECTOR {
            return sr02_in::map(ctx, tx, call);
        }
        if sel == sr02_out_single::SELECTOR {
            return sr02_out_single::map(ctx, tx, call);
        }
        if sel == sr02_out::SELECTOR {
            return sr02_out::map(ctx, tx, call);
        }
    }

    if is_universal_router(&to_lc)
        && (sel == ur_execute::SELECTOR_2ARGS || sel == ur_execute::SELECTOR_3ARGS)
    {
        return ur_execute::map(ctx, tx);
    }

    Err(MapError::UnsupportedOpcode(sel[0]))
}
