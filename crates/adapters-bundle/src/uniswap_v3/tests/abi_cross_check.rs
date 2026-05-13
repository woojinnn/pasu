//! Cross-check the hand-rolled ABI encoder/decoder in `exact_input_single.rs` against
//! alloy's `sol!`-macro-generated bindings, which are derived from the same
//! Solidity signature the Uniswap V3 router uses.
//!
//! If our hand-rolled bytes ever drift from what the `sol!` codegen produces,
//! these tests will catch it. The macro is the trustworthy reference here.

use alloy_primitives::{
    aliases::{U160, U24},
    Address as AlloyAddress, U256,
};
use alloy_sol_types::{sol, SolCall};
use std::str::FromStr;

use super::{
    decode_exact_input_single, encode_exact_input_single,
    ExactInputSingleParams as OursExactInputSingleParams, SELECTOR_EXACT_INPUT_SINGLE,
};

// `sol!` declares the ABI of the function the way you'd write it in Solidity.
// The macro generates a struct (`ExactInputSingleParams`) and a call type
// (`exactInputSingleCall`) with `abi_encode` / `abi_decode` methods.
sol! {
    #[derive(Debug, PartialEq)]
    struct SolExactInputSingleParams {
        address tokenIn;
        address tokenOut;
        uint24  fee;
        address recipient;
        uint256 deadline;
        uint256 amountIn;
        uint256 amountOutMinimum;
        uint160 sqrtPriceLimitX96;
    }

    function exactInputSingle(SolExactInputSingleParams params) external payable returns (uint256 amountOut);
}

const USDT: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const RECIPIENT: &str = "0x1111111111111111111111111111111111111111";

fn ours_params(amount_in: U256) -> OursExactInputSingleParams {
    OursExactInputSingleParams {
        token_in: AlloyAddress::from_str(USDT).unwrap(),
        token_out: AlloyAddress::from_str(WETH).unwrap(),
        fee: 3000,
        recipient: AlloyAddress::from_str(RECIPIENT).unwrap(),
        deadline: U256::from(9_999_999_999u64),
        amount_in,
        amount_out_minimum: U256::ZERO,
        sqrt_price_limit_x96: U256::ZERO,
    }
}

fn sol_call(amount_in: U256) -> exactInputSingleCall {
    exactInputSingleCall {
        params: SolExactInputSingleParams {
            tokenIn: AlloyAddress::from_str(USDT).unwrap(),
            tokenOut: AlloyAddress::from_str(WETH).unwrap(),
            fee: U24::from(3000u32),
            recipient: AlloyAddress::from_str(RECIPIENT).unwrap(),
            deadline: U256::from(9_999_999_999u64),
            amountIn: amount_in,
            amountOutMinimum: U256::ZERO,
            sqrtPriceLimitX96: U160::ZERO,
        },
    }
}

#[test]
fn selector_matches_sol_macro() {
    // The macro derives the selector from the Solidity signature it parsed.
    // If we ever picked the wrong four bytes, this fails.
    let sol_selector = exactInputSingleCall::SELECTOR;
    assert_eq!(sol_selector, SELECTOR_EXACT_INPUT_SINGLE);
    assert_eq!(hex::encode(sol_selector), "414bf389");
}

#[test]
fn our_calldata_matches_sol_calldata_byte_for_byte() {
    let amount = U256::from(200_000_000u64);

    let ours = encode_exact_input_single(&ours_params(amount));
    let theirs = sol_call(amount).abi_encode();

    assert_eq!(
        ours,
        theirs,
        "hand-rolled encoder diverged from sol! macro output:\nours = 0x{}\nthrs = 0x{}",
        hex::encode(&ours),
        hex::encode(&theirs)
    );
}

#[test]
fn cross_check_with_zero_amount() {
    let ours = encode_exact_input_single(&ours_params(U256::ZERO));
    let theirs = sol_call(U256::ZERO).abi_encode();
    assert_eq!(ours, theirs);
}

#[test]
fn cross_check_with_max_amount() {
    // Largest u128 we can safely round-trip - uses the high bytes of the U256
    // word, exercising the encoder's big-endian layout.
    let big = U256::from(u128::MAX);
    let ours = encode_exact_input_single(&ours_params(big));
    let theirs = sol_call(big).abi_encode();
    assert_eq!(ours, theirs);
}

#[test]
fn cross_check_full_u256_amount() {
    // Truly maximal U256 amount.
    let max = U256::MAX;
    let ours = encode_exact_input_single(&ours_params(max));
    let theirs = sol_call(max).abi_encode();
    assert_eq!(ours, theirs);
}

#[test]
fn sol_macro_can_decode_our_calldata() {
    let amount = U256::from(200_000_000u64);
    let our_bytes = encode_exact_input_single(&ours_params(amount));

    // If our bytes are valid ABI, the macro-generated decoder accepts them.
    let decoded = exactInputSingleCall::abi_decode(&our_bytes, true)
        .expect("sol! decoder should accept our calldata");
    assert_eq!(
        decoded.params.tokenIn.to_string().to_lowercase(),
        USDT.to_lowercase()
    );
    assert_eq!(
        decoded.params.tokenOut.to_string().to_lowercase(),
        WETH.to_lowercase()
    );
    assert_eq!(decoded.params.fee, U24::from(3000u32));
    assert_eq!(decoded.params.amountIn, amount);
}

#[test]
fn we_can_decode_sol_macro_calldata() {
    let amount = U256::from(200_000_000u64);
    let their_bytes = sol_call(amount).abi_encode();

    // Symmetric: our hand-rolled decoder accepts macro-encoded calldata.
    let decoded =
        decode_exact_input_single(&their_bytes).expect("our decoder should accept sol! calldata");
    assert_eq!(
        decoded.token_in.to_string().to_lowercase(),
        USDT.to_lowercase()
    );
    assert_eq!(
        decoded.token_out.to_string().to_lowercase(),
        WETH.to_lowercase()
    );
    assert_eq!(decoded.fee, 3000);
    assert_eq!(decoded.amount_in, amount);
}

#[test]
fn cross_check_various_fee_tiers() {
    // V3 fee tiers: 100, 500, 3000, 10000. All must match.
    for fee in [100u32, 500, 3000, 10000] {
        let mut ours = ours_params(U256::from(123u64));
        ours.fee = fee;

        let mut their = sol_call(U256::from(123u64));
        their.params.fee = U24::from(fee);

        assert_eq!(
            encode_exact_input_single(&ours),
            their.abi_encode(),
            "encoder mismatch at fee tier {fee}"
        );
    }
}
