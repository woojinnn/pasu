//! Decode an arbitrary EVM transaction's calldata against the abi-resolver's
//! built-in seed signatures.
//!
//! Usage:
//!   cargo run -p abi-resolver --example decode_any -- \
//!       <chain_id> <to_address> <calldata_hex>
//!
//! Example (USDT approve to V3 SwapRouter, 100 USDT):
//!   cargo run -p abi-resolver --example decode_any -- \
//!       1 \
//!       0xdAC17F958D2ee523a2206206994597C13D831ec7 \
//!       0x095ea7b3000000000000000000000000e592427a0aece92de3edee1f18e0157c0586156400000000000000000000000000000000000000000000000000000000000000064
//!
//! The seed list (built into this binary) covers a handful of common selectors
//! so the example is runnable without an imported Sourcify/openchain dump. The
//! production setup imports a real dump into the same indices.

use abi_resolver::{
    decode::format_value,
    openchain::{OpenchainIndex, SignatureCandidate},
    resolver::{ResolveOutcome, Resolver, Source},
    sourcify::SourcifyIndex,
};
use alloy_primitives::Address;
use std::process::ExitCode;
use std::str::FromStr;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: {} <chain_id> <to_address> <calldata_hex>", args[0]);
        return ExitCode::from(2);
    }

    let chain_id: u64 = match args[1].parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid chain_id {}: {e}", args[1]);
            return ExitCode::from(2);
        }
    };
    let address = match Address::from_str(&args[2]) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("invalid address {}: {e}", args[2]);
            return ExitCode::from(2);
        }
    };
    let calldata_hex = args[3].strip_prefix("0x").unwrap_or(&args[3]);
    let calldata = match hex::decode(calldata_hex) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("invalid calldata hex: {e}");
            return ExitCode::from(2);
        }
    };

    let resolver = build_seeded_resolver();

    println!("─── INPUT ───");
    println!("  chain    : {chain_id}");
    println!("  to       : 0x{}", hex::encode(address.0));
    println!("  calldata : {} bytes", calldata.len());
    if calldata.len() >= 4 {
        println!("  selector : 0x{}", hex::encode(&calldata[..4]));
    }
    println!();

    match resolver.resolve(chain_id, &address, &calldata) {
        ResolveOutcome::Resolved(r) => {
            let source = match r.source {
                Source::Sourcify => "Sourcify (curated, parameter names available)",
                #[cfg(feature = "sqlite")]
                Source::SourcifyDb => "Sourcify DB dump (parameter names available)",
                Source::Openchain => "openchain (selector match, names synthesised)",
            };
            println!("─── RESOLVED via {source} ───");
            println!("  function  : {}", r.decoded.function_name);
            println!("  signature : {}", r.decoded.signature);
            println!("  args:");
            for arg in &r.decoded.args {
                println!(
                    "    {} : {} = {}",
                    arg.name,
                    arg.sol_type,
                    format_value(&arg.value)
                );
            }
            ExitCode::SUCCESS
        }
        ResolveOutcome::NotFound => {
            println!("─── NOT FOUND ───");
            println!("  No matching signature in the resolver. With a real");
            println!("  Sourcify/openchain dump imported, more selectors would");
            println!("  resolve. Falling through to LegacyAction::Other upstream.");
            ExitCode::from(1)
        }
    }
}

/// Embedded Sourcify bundle (curated set of major mainnet contracts).
/// Populated by the data/ prep script; see `data/sourcify.json`.
const SOURCIFY_BUNDLE: &[u8] = include_bytes!("../data/sourcify.json");

/// Built-in seeds so the example is self-contained.
///
/// Two layers:
/// - **Sourcify** (precise, with parameter names) — bundled major contracts.
/// - **openchain** fallback — selector → signature for common functions when
///   the contract isn't in the Sourcify bundle.
fn build_seeded_resolver() -> Resolver {
    let sourcify = SourcifyIndex::load_bundle(SOURCIFY_BUNDLE)
        .expect("bundled sourcify.json should deserialize");

    let mut openchain = OpenchainIndex::empty();
    for (selector, signature) in seed_signatures() {
        openchain.insert(
            *selector,
            SignatureCandidate {
                signature: (*signature).into(),
                verified: true,
            },
        );
    }
    Resolver::new(sourcify, openchain)
}

fn seed_signatures() -> &'static [([u8; 4], &'static str)] {
    &[
        // ERC-20
        ([0x09, 0x5e, 0xa7, 0xb3], "approve(address,uint256)"),
        ([0xa9, 0x05, 0x9c, 0xbb], "transfer(address,uint256)"),
        (
            [0x23, 0xb8, 0x72, 0xdd],
            "transferFrom(address,address,uint256)",
        ),
        ([0x70, 0xa0, 0x82, 0x31], "balanceOf(address)"),
        // Uniswap V3 SwapRouter
        (
            [0x41, 0x4b, 0xf3, 0x89],
            "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))",
        ),
        (
            [0xc0, 0x4b, 0x8d, 0x59],
            "exactInput((bytes,address,uint256,uint256,uint256))",
        ),
        ([0xac, 0x96, 0x50, 0xd8], "multicall(bytes[])"),
        ([0x5a, 0xe4, 0x01, 0xdc], "multicall(uint256,bytes[])"),
        // Uniswap V2 Router02
        (
            [0x38, 0xed, 0x17, 0x39],
            "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)",
        ),
        (
            [0x7f, 0xf3, 0x6a, 0xb5],
            "swapExactETHForTokens(uint256,address[],address,uint256)",
        ),
        // PancakeSwap V2 (BSC) — fee-on-transfer variants are common
        (
            [0xb6, 0xf9, 0xde, 0x95],
            "swapExactETHForTokensSupportingFeeOnTransferTokens(uint256,address[],address,uint256)",
        ),
        (
            [0x79, 0x1a, 0xc9, 0x47],
            "swapExactTokensForETHSupportingFeeOnTransferTokens(uint256,uint256,address[],address,uint256)",
        ),
        // Universal Router
        ([0x24, 0x85, 0x6b, 0xc3], "execute(bytes,bytes[])"),
        (
            [0x35, 0x93, 0x56, 0x4c],
            "execute(bytes,bytes[],uint256)",
        ),
        // Aave V3 Pool
        (
            [0x61, 0x7b, 0xa0, 0x37],
            "supply(address,uint256,address,uint16)",
        ),
        ([0x69, 0x32, 0x8d, 0xec], "withdraw(address,uint256,address)"),
        (
            [0xa4, 0x15, 0xbc, 0xad],
            "borrow(address,uint256,uint256,uint16,address)",
        ),
        (
            [0x57, 0x3a, 0xde, 0x81],
            "repay(address,uint256,uint256,address)",
        ),
        // Morpho Bundler — most user flows go through this multicall variant
        (
            [0x37, 0x4f, 0x43, 0x5d],
            "multicall((address,bytes,uint256,bool,bytes32)[])",
        ),
    ]
}
