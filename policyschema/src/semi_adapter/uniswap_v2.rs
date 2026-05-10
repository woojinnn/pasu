//! Uniswap V2 Router02 decoder.
//!
//! 9개 swap 함수 + 공통 `build_v2_swap_fields` 빌더.
//!
//! schema_v260508의 `decoders/uniswap-v2.ts` 패턴 차용.
//!
//! # 함수 셀렉터
//!
//! | selector | 시그니처 | mode | direction | FOT |
//! |---|---|---|---|---|
//! | 0x38ed1739 | swapExactTokensForTokens | ExactIn | TokenToken | false |
//! | 0x8803dbee | swapTokensForExactTokens | ExactOut | TokenToken | false |
//! | 0x7ff36ab5 | swapExactETHForTokens (payable) | ExactIn | EthToToken | false |
//! | 0x4a25d94a | swapTokensForExactETH | ExactOut | TokenToEth | false |
//! | 0x18cbafe5 | swapExactTokensForETH | ExactIn | TokenToEth | false |
//! | 0xfb3bdb41 | swapETHForExactTokens (payable) | ExactOut | EthToToken | false |
//! | 0x5c11d795 | swapExactTokensForTokensSupportingFeeOnTransferTokens | ExactIn | TokenToken | true |
//! | 0xb6f9de95 | swapExactETHForTokensSupportingFeeOnTransferTokens | ExactIn | EthToToken | true |
//! | 0x791ac947 | swapExactTokensForETHSupportingFeeOnTransferTokens | ExactIn | TokenToEth | true |

use serde_json::Value;

use crate::action::fields::{
    HopRef, SettlementKind, SlippageInfo, SlippageSource, SwapFields, SwapMode, SwapRoute,
};
use crate::confidence::Confidence;
use crate::semi_adapter::common::{
    amount_exact, amount_max, amount_min, as_address, as_address_array, as_u64, as_uint_string,
    deadline_horizon, recipients_from,
};
use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::registry::token_metadata;
use crate::semi_adapter::BuildContext;
use crate::types::{Address, DeadlineFields, Token};

// ===========================================================================
// 셀렉터 상수
// ===========================================================================

pub const SEL_SWAP_EXACT_TOKENS_FOR_TOKENS: [u8; 4] = [0x38, 0xed, 0x17, 0x39];
pub const SEL_SWAP_TOKENS_FOR_EXACT_TOKENS: [u8; 4] = [0x88, 0x03, 0xdb, 0xee];
pub const SEL_SWAP_EXACT_ETH_FOR_TOKENS: [u8; 4] = [0x7f, 0xf3, 0x6a, 0xb5];
pub const SEL_SWAP_TOKENS_FOR_EXACT_ETH: [u8; 4] = [0x4a, 0x25, 0xd9, 0x4a];
pub const SEL_SWAP_EXACT_TOKENS_FOR_ETH: [u8; 4] = [0x18, 0xcb, 0xaf, 0xe5];
pub const SEL_SWAP_ETH_FOR_EXACT_TOKENS: [u8; 4] = [0xfb, 0x3b, 0xdb, 0x41];
pub const SEL_SWAP_EXACT_TOKENS_FOR_TOKENS_FOT: [u8; 4] = [0x5c, 0x11, 0xd7, 0x95];
pub const SEL_SWAP_EXACT_ETH_FOR_TOKENS_FOT: [u8; 4] = [0xb6, 0xf9, 0xde, 0x95];
pub const SEL_SWAP_EXACT_TOKENS_FOR_ETH_FOT: [u8; 4] = [0x79, 0x1a, 0xc9, 0x47];

/// V2 풀의 고정 fee — 0.3% = 30 basis points.
const V2_FIXED_FEE_BPS: u32 = 30;

// ===========================================================================
// 변형 식별
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V2Direction {
    TokenToToken,
    EthToToken,
    TokenToEth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V2SwapVariant {
    pub mode: SwapMode,
    pub direction: V2Direction,
    pub supporting_fee_on_transfer: bool,
}

/// 셀렉터로 변형 식별. 알 수 없으면 `BadSelector`.
pub fn variant_from_selector(selector: &[u8; 4]) -> Result<V2SwapVariant, SemiAdapterError> {
    use V2Direction::*;
    let v = match *selector {
        SEL_SWAP_EXACT_TOKENS_FOR_TOKENS => V2SwapVariant {
            mode: SwapMode::ExactIn,
            direction: TokenToToken,
            supporting_fee_on_transfer: false,
        },
        SEL_SWAP_TOKENS_FOR_EXACT_TOKENS => V2SwapVariant {
            mode: SwapMode::ExactOut,
            direction: TokenToToken,
            supporting_fee_on_transfer: false,
        },
        SEL_SWAP_EXACT_ETH_FOR_TOKENS => V2SwapVariant {
            mode: SwapMode::ExactIn,
            direction: EthToToken,
            supporting_fee_on_transfer: false,
        },
        SEL_SWAP_TOKENS_FOR_EXACT_ETH => V2SwapVariant {
            mode: SwapMode::ExactOut,
            direction: TokenToEth,
            supporting_fee_on_transfer: false,
        },
        SEL_SWAP_EXACT_TOKENS_FOR_ETH => V2SwapVariant {
            mode: SwapMode::ExactIn,
            direction: TokenToEth,
            supporting_fee_on_transfer: false,
        },
        SEL_SWAP_ETH_FOR_EXACT_TOKENS => V2SwapVariant {
            mode: SwapMode::ExactOut,
            direction: EthToToken,
            supporting_fee_on_transfer: false,
        },
        SEL_SWAP_EXACT_TOKENS_FOR_TOKENS_FOT => V2SwapVariant {
            mode: SwapMode::ExactIn,
            direction: TokenToToken,
            supporting_fee_on_transfer: true,
        },
        SEL_SWAP_EXACT_ETH_FOR_TOKENS_FOT => V2SwapVariant {
            mode: SwapMode::ExactIn,
            direction: EthToToken,
            supporting_fee_on_transfer: true,
        },
        SEL_SWAP_EXACT_TOKENS_FOR_ETH_FOT => V2SwapVariant {
            mode: SwapMode::ExactIn,
            direction: TokenToEth,
            supporting_fee_on_transfer: true,
        },
        _ => {
            return Err(SemiAdapterError::BadSelector {
                expected: "Uniswap V2 swap selector".into(),
                got: format!("0x{}", hex::encode(selector)),
            })
        }
    };
    Ok(v)
}

// ===========================================================================
// 공통 빌더
// ===========================================================================

/// args(JSON) + variant + ctx → `SwapFields` 변환.
///
/// args는 다음 키를 가져야 함 (variant에 따라 일부 다름):
///  - `amountIn` 또는 `amountOut` (uint256 십진 문자열)
///  - `amountOutMin` 또는 `amountInMax`
///  - `path` (address[])
///  - `to` (address)
///  - `deadline` (u64)
pub fn build_v2_swap_fields(
    args: &Value,
    variant: V2SwapVariant,
    ctx: &BuildContext,
) -> Result<SwapFields, SemiAdapterError> {
    let path = as_address_array(args, "path")?;
    if path.len() < 2 {
        return Err(SemiAdapterError::BadV3Path { length: path.len() });
    }
    let recipient = as_address(args, "to")?;
    let deadline = as_u64(args, "deadline")?;

    // amountIn / amountOut 추출 — variant에 따라 다름
    let (amount_in, amount_out) = match variant.mode {
        SwapMode::ExactIn => {
            // ExactIn: amountIn (Exact) + amountOutMin (Min)
            // EthToToken은 msg.value를 amountIn으로 사용
            let amount_in_raw = if variant.direction == V2Direction::EthToToken {
                ctx.value_wei.clone()
            } else {
                as_uint_string(args, "amountIn")?
            };
            let amount_out_min = as_uint_string(args, "amountOutMin")?;
            (amount_exact(amount_in_raw), amount_min(amount_out_min))
        }
        SwapMode::ExactOut => {
            // ExactOut: amountOut (Exact) + amountInMax (Max)
            let amount_out_raw = as_uint_string(args, "amountOut")?;
            let amount_in_max = if variant.direction == V2Direction::EthToToken {
                ctx.value_wei.clone()
            } else {
                as_uint_string(args, "amountInMax")?
            };
            (amount_max(amount_in_max), amount_exact(amount_out_raw))
        }
        _ => {
            return Err(SemiAdapterError::AbiDecode {
                reason: format!("V2 doesn't support mode {:?}", variant.mode),
            })
        }
    };

    // path → 토큰들. ETH가 path 끝에 있으면 token_in/out에 native sentinel 표기
    let input_tokens = vec![token_at(&path, 0, ctx, variant.direction == V2Direction::EthToToken)];
    let output_tokens = vec![token_at(
        &path,
        path.len() - 1,
        ctx,
        variant.direction == V2Direction::TokenToEth,
    )];

    // hops — V2는 path[i] → path[i+1] 시퀀스
    let hops = path
        .windows(2)
        .enumerate()
        .map(|(i, _w)| HopRef {
            id: format!("h#{i}"),
            protocol: "uniswap.v2".into(),
            token_in: token_at(&path, i, ctx, false),
            token_out: token_at(&path, i + 1, ctx, false),
            pool: None, // V2 pair 주소는 calldata에 없음 (pairFor로 도출)
            fee_bps: Some(V2_FIXED_FEE_BPS),
            confidence: if variant.supporting_fee_on_transfer {
                Confidence::Medium
            } else {
                Confidence::High
            },
        })
        .collect::<Vec<_>>();

    let route = if hops.len() == 1 {
        SwapRoute::SingleHop {
            hop: hops.into_iter().next().unwrap(),
        }
    } else {
        SwapRoute::MultiHop { hops }
    };

    let amount_out_min_for_slippage = if matches!(variant.mode, SwapMode::ExactIn) {
        Some(amount_out.clone())
    } else {
        None
    };

    let has_zero_min_output = match variant.mode {
        SwapMode::ExactIn => amount_out.raw == "0",
        _ => false,
    };

    Ok(SwapFields {
        actor: ctx.actor,
        protocol_ids: vec!["uniswap.v2".into()],
        input_tokens,
        output_tokens,
        mode: variant.mode,
        amount_in,
        amount_out,
        route,
        slippage: SlippageInfo {
            source: SlippageSource::Calldata,
            amount_out_min: amount_out_min_for_slippage,
        },
        settlement: SettlementKind::Router,
        recipients: recipients_from(Some(recipient), ctx.actor),
        deadlines: DeadlineFields {
            deadline: Some(deadline),
            deadline_horizon_seconds: deadline_horizon(deadline, ctx.block_timestamp),
        },
        max_fee_bps: if variant.supporting_fee_on_transfer {
            None // FOT 토큰은 별도 transfer fee가 더해져 정확한 max_fee_bps 산정 불가
        } else {
            Some(V2_FIXED_FEE_BPS)
        },
        has_zero_min_output,
    })
}

/// path의 i번째 주소를 Token으로 변환. `is_native_endpoint`이 true이면
/// path 인덱스가 *WETH 위치*인 ETH 변형으로, native sentinel로 정규화.
fn token_at(
    path: &[Address],
    index: usize,
    ctx: &BuildContext,
    is_native_endpoint: bool,
) -> Token {
    let addr = path[index];
    if is_native_endpoint {
        // ETH 변형은 path 양 끝에 WETH 주소가 들어가 있지만 의미는 native ETH
        let mut t = token_metadata(addr, ctx.chain_id);
        // 만약 WETH 주소면 native로 정규화
        const WETH_MAINNET: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
        if format!("{addr:#x}").to_lowercase() == WETH_MAINNET.to_lowercase() {
            t.symbol = "ETH".into();
            t.is_native = true;
        }
        return t;
    }
    token_metadata(addr, ctx.chain_id)
}

// ===========================================================================
// 공개 진입 함수
// ===========================================================================

/// 셀렉터·args·ctx → V2 SwapFields. dispatch에서 호출되는 진입점.
pub fn route_v2_swap(
    selector: &[u8; 4],
    args: &Value,
    ctx: &BuildContext,
) -> Result<SwapFields, SemiAdapterError> {
    let variant = variant_from_selector(selector)?;
    build_v2_swap_fields(args, variant, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confidence::Confidence;
    use crate::target::{ContractTarget, DiscoveredBy, TargetRole, Verification};
    use serde_json::json;

    fn ctx_mainnet(actor: Address, value_wei: &str) -> (Vec<ContractTarget>, BuildContext<'static>) {
        let target: Address = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
            .parse()
            .unwrap();
        let targets: Vec<ContractTarget> = vec![ContractTarget {
            id: "t#router".into(),
            address: target,
            chain_id: 1,
            role: TargetRole::Router,
            protocol: None,
            discovered_by: DiscoveredBy::TxTo,
            verification: Verification {
                label_source: "curated".into(),
                abi_available: true,
                contract_verified: true,
                proxy_resolved: None,
            },
            confidence: Confidence::High,
        }];
        // 'static lifetime hack — leak 사용 (테스트만)
        let leaked: &'static [ContractTarget] = Box::leak(targets.clone().into_boxed_slice());
        let ctx = BuildContext {
            chain_id: 1,
            actor,
            target,
            value_wei: value_wei.into(),
            block_timestamp: Some(1_762_499_000),
            targets: leaked,
        };
        (targets, ctx)
    }

    #[test]
    fn variant_classification() {
        let v = variant_from_selector(&SEL_SWAP_EXACT_TOKENS_FOR_TOKENS).unwrap();
        assert_eq!(v.mode, SwapMode::ExactIn);
        assert_eq!(v.direction, V2Direction::TokenToToken);
        assert!(!v.supporting_fee_on_transfer);

        let v = variant_from_selector(&SEL_SWAP_EXACT_ETH_FOR_TOKENS).unwrap();
        assert_eq!(v.direction, V2Direction::EthToToken);

        let v = variant_from_selector(&SEL_SWAP_EXACT_TOKENS_FOR_TOKENS_FOT).unwrap();
        assert!(v.supporting_fee_on_transfer);

        assert!(variant_from_selector(&[0xde, 0xad, 0xbe, 0xef]).is_err());
    }

    #[test]
    fn decode_swap_exact_tokens_for_tokens() {
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let (_t, ctx) = ctx_mainnet(actor, "0");
        let args = json!({
            "amountIn": "1000000000",
            "amountOutMin": "300000000000000000",
            "path": [
                "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
            ],
            "to": "0x1111111111111111111111111111111111111111",
            "deadline": "1762500000"
        });
        let fields = route_v2_swap(&SEL_SWAP_EXACT_TOKENS_FOR_TOKENS, &args, &ctx).unwrap();

        assert_eq!(fields.mode, SwapMode::ExactIn);
        assert_eq!(fields.amount_in.raw, "1000000000");
        assert_eq!(fields.amount_out.raw, "300000000000000000");
        assert_eq!(fields.input_tokens[0].symbol, "USDC");
        assert_eq!(fields.output_tokens[0].symbol, "WETH");
        assert_eq!(fields.max_fee_bps, Some(30));
        assert!(matches!(fields.route, SwapRoute::SingleHop { .. }));
        assert!(fields.recipients.recipient_equals_actor);
        assert!(!fields.has_zero_min_output);
    }

    #[test]
    fn decode_fot_variant_lowers_confidence() {
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let (_t, ctx) = ctx_mainnet(actor, "0");
        let args = json!({
            "amountIn": "1000000000000000000000",
            "amountOutMin": "50000000000000000",
            "path": [
                "0x45804880De22913dAFE09f4980848ECE6EcbAf78",
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
            ],
            "to": "0x1111111111111111111111111111111111111111",
            "deadline": "1762500000"
        });
        let fields = route_v2_swap(&SEL_SWAP_EXACT_TOKENS_FOR_TOKENS_FOT, &args, &ctx).unwrap();

        assert_eq!(fields.max_fee_bps, None); // FOT는 정확한 fee 산정 불가
        if let SwapRoute::SingleHop { hop } = &fields.route {
            assert_eq!(hop.confidence, Confidence::Medium);
        } else {
            panic!("expected SingleHop");
        }
    }

    #[test]
    fn decode_eth_for_tokens_uses_value_wei() {
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let (_t, ctx) = ctx_mainnet(actor, "1000000000000000000"); // 1 ETH
        let args = json!({
            "amountOutMin": "2000000000",
            "path": [
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
                "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
            ],
            "to": "0x1111111111111111111111111111111111111111",
            "deadline": "1762500000"
        });
        let fields = route_v2_swap(&SEL_SWAP_EXACT_ETH_FOR_TOKENS, &args, &ctx).unwrap();

        assert_eq!(fields.amount_in.raw, "1000000000000000000"); // msg.value 사용
        assert_eq!(fields.input_tokens[0].symbol, "ETH"); // native로 정규화
        assert!(fields.input_tokens[0].is_native);
    }
}
