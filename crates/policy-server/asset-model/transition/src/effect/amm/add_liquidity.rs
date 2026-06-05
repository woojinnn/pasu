//! `AddLiquidityAction` reducer — deposit liquidity into a pool.
//!
//! ## Batch 3 — lifecycle activation
//!
//! Three deposit shapes are dispatched off the `AddLiquidityParams` variant:
//!
//! * [`Pooled`](crate::action::amm::AddLiquidityParams::Pooled) — `Uniswap V2`
//!   / `SushiV2` / `Curve` / `Balancer V2` constant-product or stable / weighted
//!   pools. Each underlying token is debited at its *desired* deposit amount
//!   (a conservative ceiling: actual deposit ≤ desired with leftover refunded
//!   by the router; debiting the ceiling is the worst-case for the wallet's
//!   balance check). The LP receipt is credited at `min_lp_out` (the floor the
//!   user signed for — actual mint ≥ floor, so the floor is the conservative
//!   lower-bound credit). The LP fungible token is keyed off the venue's pool
//!   address (the LP contract is the pool itself for `Uniswap V2` and `Curve`,
//!   and the `Balancer V2` pool token sits at `pool_id[0..20]`). If the user
//!   has no prior LP holding, the reducer emits [`TokenChange::Mint`] before
//!   the credit (the [`helpers::balance::credit`] helper rejects first-time
//!   receipts — see the helper's doc comment).
//! * [`ConcentratedMint`](crate::action::amm::AddLiquidityParams::ConcentratedMint)
//!   — `Uniswap V3` / `V4` mint of a brand-new position NFT. The two pair
//!   tokens are debited at `amount_desired`. The resulting NFT's `token_id`
//!   is assigned by the contract at mint time and is therefore unknown at
//!   sign time, so no NFT-side [`TokenChange::Mint`] is emitted — the NFT
//!   lands in `state.tokens` on the next sync, and this reducer cannot
//!   forge a synthetic `TokenKey::Erc721 { token_id, … }` without speculating
//!   on the post-mint id.
//! * [`ConcentratedIncrease`](crate::action::amm::AddLiquidityParams::ConcentratedIncrease)
//!   — `Uniswap V3` / `V4` top-up of an existing position. The pair tokens
//!   are looked up from the NFT holding's
//!   [`TokenKind::LpShare`](policy_state::token::TokenKind::LpShare)
//!   `underlyings` (a `Concentrated` LP carries exactly two underlyings in
//!   today's reducer scope; any other length surfaces as `Invariant`). The
//!   liquidity / `tokensOwed` mutations on the NFT itself are not represented
//!   today — `TokenChange` carries no variant for mutating an LP NFT's
//!   internal `LpShape` state, and the state crate is read-only in this
//!   sub-agent's scope. The internal liquidity update lands on the next
//!   sync; this reducer only models the user-visible balance delta on
//!   the two underlyings.
//!
//! ## Slippage convention
//!
//! `Pooled` credits `min_lp_out`; `ConcentratedMint` / `Increase` debit
//! `amount_desired`. Both are the *worst-case* end of the slippage tolerance
//! from the wallet's perspective — they understate any received receipt and
//! overstate any spent input, so a policy that approves the simulated delta
//! will tolerate the actual on-chain delta too.
//!
//! ## Venue mismatch
//!
//! A `Pooled` arm against a `UniswapV3` / `UniswapV4` venue, or a
//! `ConcentratedMint` / `ConcentratedIncrease` arm against a `UniswapV2` /
//! `SushiV2` / `Curve` / `Balancer` venue, is rejected as
//! [`ReducerError::UnsupportedProtocol`]. The mismatch must be caught at
//! decode time in production, but the reducer keeps the check defensive so a
//! malformed bundle does not silently route to a wrong-shape math path.

use policy_state::delta::TokenChange;
use policy_state::primitives::{Address, ChainId, SignedI256, U256};
use policy_state::token::{LpShape, TokenKey, TokenKind, TokenRef};
use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::amm::{AddLiquidityAction, AddLiquidityParams, AmmVenue};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

/// Returns the LP fungible token key for a `Pooled` deposit's receipt.
///
/// For pools whose LP token contract sits at the pool address itself
/// (`Uniswap V2`, `SushiV2`, `Curve V1` / `V2`, and `MaverickV2`), the LP key
/// is just `Erc20 { chain, address: pool }`.
///
/// `Balancer V2` exposes the LP token at the first 20 bytes of the
/// `pool_id` (the canonical pool-id encoding). `Balancer V3` does not carry
/// a `pool` field; today we still slice the same prefix from `pool_id` so
/// the LP receipt has a stable key, mirroring V2.
///
/// Concentrated venues (`UniswapV3`, `UniswapV4`, `TraderJoeLB`) and the
/// `AggregatorRoute` synthetic venue have no fungible LP receipt and
/// surface as `UnsupportedProtocol`.
fn pooled_lp_key(venue: &AmmVenue) -> ReducerResult<TokenKey> {
    match venue {
        AmmVenue::UniswapV2 { chain, pool, .. }
        | AmmVenue::SushiV2 { chain, pool }
        | AmmVenue::CurveV1 { chain, pool, .. }
        | AmmVenue::CurveV2 { chain, pool }
        | AmmVenue::MaverickV2 { chain, pool } => Ok(TokenKey::Erc20 {
            chain: chain.clone(),
            address: *pool,
        }),
        AmmVenue::BalancerV2 { chain, pool_id, .. }
        | AmmVenue::BalancerV3 { chain, pool_id, .. } => Ok(TokenKey::Erc20 {
            chain: chain.clone(),
            address: pool_id_to_address(pool_id)?,
        }),
        AmmVenue::UniswapV3 { .. }
        | AmmVenue::UniswapV4 { .. }
        | AmmVenue::TraderJoeLB { .. }
        | AmmVenue::AaveGsm { .. }
        | AmmVenue::AggregatorRoute { .. } => Err(ReducerError::UnsupportedProtocol {
            action: "add_liquidity".into(),
            protocol: pooled_venue_tag(venue).into(),
        }),
    }
}

/// `Balancer` pool-id encoding: the first 20 bytes hold the pool token's
/// contract address. We decode the leading 20 bytes from the hex prefix and
/// build an `Address`; anything shorter than 20 bytes or non-hex surfaces as
/// `Invariant`.
fn pool_id_to_address(pool_id: &str) -> ReducerResult<Address> {
    let stripped = pool_id.strip_prefix("0x").unwrap_or(pool_id);
    // 20 bytes = 40 hex chars.
    if stripped.len() < 40 {
        return Err(ReducerError::Invariant(format!(
            "balancer pool_id too short to extract LP address: {pool_id}"
        )));
    }
    let mut bytes = [0u8; 20];
    for (i, b) in bytes.iter_mut().enumerate() {
        let hi = hex_nibble(stripped.as_bytes()[i * 2])?;
        let lo = hex_nibble(stripped.as_bytes()[i * 2 + 1])?;
        *b = (hi << 4) | lo;
    }
    Ok(Address::from(bytes))
}

/// Lowercase / uppercase hex nibble → `u8`. Out-of-range bytes surface as
/// `Invariant` to keep `pool_id_to_address` total.
fn hex_nibble(c: u8) -> ReducerResult<u8> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        other => Err(ReducerError::Invariant(format!(
            "balancer pool_id contains non-hex byte: 0x{other:02x}"
        ))),
    }
}

/// Short tag used in `UnsupportedProtocol.protocol` for a venue when the
/// `Pooled` deposit shape doesn't apply.
const fn pooled_venue_tag(venue: &AmmVenue) -> &'static str {
    match venue {
        AmmVenue::UniswapV2 { .. } => "uniswap_v2",
        AmmVenue::UniswapV3 { .. } => "uniswap_v3",
        AmmVenue::UniswapV4 { .. } => "uniswap_v4",
        AmmVenue::SushiV2 { .. } => "sushi_v2",
        AmmVenue::CurveV1 { .. } => "curve_v1",
        AmmVenue::CurveV2 { .. } => "curve_v2",
        AmmVenue::BalancerV2 { .. } => "balancer_v2",
        AmmVenue::BalancerV3 { .. } => "balancer_v3",
        AmmVenue::TraderJoeLB { .. } => "trader_joe_lb",
        AmmVenue::MaverickV2 { .. } => "maverick_v2",
        AmmVenue::AaveGsm { .. } => "aave_gsm",
        AmmVenue::AggregatorRoute { .. } => "aggregator_route",
    }
}

/// Short tag for concentrated-only venues — used when the action requires
/// a concentrated pool and the supplied venue isn't one.
const fn concentrated_venue_tag(venue: &AmmVenue) -> &'static str {
    pooled_venue_tag(venue)
}

/// Validate that `venue` is concentrated-shaped (i.e. accepts a
/// `ConcentratedMint` / `ConcentratedIncrease` / `ConcentratedDecrease` /
/// `ConcentratedBurn` params variant).
fn require_concentrated_venue(venue: &AmmVenue, action: &str) -> ReducerResult<()> {
    match venue {
        AmmVenue::UniswapV3 { .. } | AmmVenue::UniswapV4 { .. } | AmmVenue::TraderJoeLB { .. } => {
            Ok(())
        }
        _ => Err(ReducerError::UnsupportedProtocol {
            action: action.into(),
            protocol: concentrated_venue_tag(venue).into(),
        }),
    }
}

/// Look up the two `underlyings` token refs of a concentrated-LP NFT holding,
/// erroring if the holding is missing, the kind is not `LpShare(Concentrated)`,
/// or the `underlyings` count is not exactly two.
pub(super) fn concentrated_underlyings(
    state: &WalletState,
    nft_key: &TokenKey,
) -> ReducerResult<(TokenRef, TokenRef)> {
    let holding = state
        .tokens
        .get(nft_key)
        .ok_or_else(|| ReducerError::TokenNotFound(nft_key.clone()))?;
    let TokenKind::LpShare {
        underlyings,
        shape: LpShape::Concentrated { .. },
        ..
    } = &holding.kind
    else {
        return Err(ReducerError::Invariant(format!(
            "concentrated lifecycle action targeted {nft_key:?} \
             but holding is not LpShare(Concentrated)"
        )));
    };
    if underlyings.len() != 2 {
        return Err(ReducerError::Invariant(format!(
            "concentrated LP {nft_key:?} has {len} underlyings, expected 2",
            len = underlyings.len()
        )));
    }
    Ok((underlyings[0].clone(), underlyings[1].clone()))
}

/// Push a `TokenChange::Mint` for `key` (LP fungible receipt) unless the
/// holding already exists in `state.tokens`. `kind_hint` is the
/// post-sync expected `TokenKind`.
pub(super) fn ensure_mint_stub(
    state: &WalletState,
    delta: &mut StateDelta,
    key: &TokenKey,
    kind_hint: TokenKind,
) {
    if !state.tokens.contains_key(key) {
        delta.token_changes.push(TokenChange::Mint {
            key: key.clone(),
            kind_hint,
        });
    }
}

/// Push a raw positive [`TokenChange::BalanceDelta`] for a first-time receipt
/// (the `helpers::balance::credit` helper rejects this case — see its docs).
/// Pairs with [`ensure_mint_stub`].
fn raw_credit(delta: &mut StateDelta, key: &TokenKey, amount: U256) {
    let signed = SignedI256::try_from(amount).unwrap_or(SignedI256::MAX);
    delta.token_changes.push(TokenChange::BalanceDelta {
        key: key.clone(),
        delta: signed,
    });
}

/// Build the `LpShape::Pooled` hint for a new LP fungible receipt. The
/// `weights` field is `None` (V2 / Curve / Balancer weighted information is
/// not synthesised reducer-side; the sync orchestrator overwrites it).
fn pooled_lp_kind_hint(underlyings: Vec<TokenRef>) -> TokenKind {
    use policy_state::primitives::PoolRef;
    use policy_state::primitives::ProtocolRef;
    use policy_state::token::ShareForm;
    TokenKind::LpShare {
        pool: PoolRef {
            protocol: ProtocolRef::new("unknown"),
            pool_addr: None,
            pool_id: None,
            fee_tier: None,
        },
        underlyings,
        share_form: ShareForm::Fungible,
        shape: LpShape::Pooled { weights: None },
    }
}

/// Chain accessor: every `AmmVenue` carries a `ChainId` in its first field.
const fn venue_chain(venue: &AmmVenue) -> &ChainId {
    match venue {
        AmmVenue::UniswapV2 { chain, .. }
        | AmmVenue::UniswapV3 { chain, .. }
        | AmmVenue::UniswapV4 { chain, .. }
        | AmmVenue::SushiV2 { chain, .. }
        | AmmVenue::CurveV1 { chain, .. }
        | AmmVenue::CurveV2 { chain, .. }
        | AmmVenue::BalancerV2 { chain, .. }
        | AmmVenue::BalancerV3 { chain, .. }
        | AmmVenue::TraderJoeLB { chain, .. }
        | AmmVenue::MaverickV2 { chain, .. }
        | AmmVenue::AaveGsm { chain, .. }
        | AmmVenue::AggregatorRoute { chain, .. } => chain,
    }
}

impl Reducer for AddLiquidityAction {
    fn apply(&self, state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();

        match &self.params {
            // ---------- Pooled (V2 / Curve / Balancer) ----------
            AddLiquidityParams::Pooled {
                tokens,
                min_lp_out,
                recipient: _,
            } => {
                if tokens.is_empty() {
                    return Err(ReducerError::Invariant(
                        "add_liquidity pooled: tokens list is empty".into(),
                    ));
                }
                // Debit each underlying at its desired amount (conservative
                // ceiling — actual deposit ≤ desired).
                for (token_ref, amount) in tokens {
                    helpers::balance::debit(state, &mut delta, &token_ref.key, *amount)?;
                }
                // LP receipt: synthesise the LP fungible key off the venue and
                // credit `min_lp_out` (conservative floor — actual ≥ floor).
                let lp_key = pooled_lp_key(&self.venue)?;
                if state.tokens.contains_key(&lp_key) {
                    helpers::balance::credit(state, &mut delta, &lp_key, *min_lp_out)?;
                } else {
                    let underlyings = tokens.iter().map(|(t, _)| t.clone()).collect();
                    ensure_mint_stub(state, &mut delta, &lp_key, pooled_lp_kind_hint(underlyings));
                    raw_credit(&mut delta, &lp_key, *min_lp_out);
                }
            }

            // ---------- Concentrated mint (V3 / V4 new NFT) ----------
            AddLiquidityParams::ConcentratedMint {
                pool_pair,
                amount_desired,
                amount_min: _,
                range: _,
                recipient: _,
            } => {
                require_concentrated_venue(&self.venue, "add_liquidity")?;
                // Sanity: pool_pair tokens must live on the venue's chain.
                let chain = venue_chain(&self.venue);
                for token in [&pool_pair.0, &pool_pair.1] {
                    if token.key.chain() != chain {
                        return Err(ReducerError::Invariant(format!(
                            "concentrated mint pool_pair token {:?} chain mismatch \
                             vs venue chain {chain:?}",
                            token.key
                        )));
                    }
                }
                helpers::balance::debit(state, &mut delta, &pool_pair.0.key, amount_desired.0)?;
                helpers::balance::debit(state, &mut delta, &pool_pair.1.key, amount_desired.1)?;
                // The NFT's token_id is assigned at mint time by the contract
                // and is unknown at sign time; we therefore cannot synthesise
                // a `TokenKey::Erc721 { token_id, … }` for the new mint. The
                // NFT lands in `state.tokens` on the next sync.
            }

            // ---------- Concentrated increase (V3 / V4 existing NFT) ----------
            AddLiquidityParams::ConcentratedIncrease {
                nft_key,
                amount_desired,
                amount_min: _,
            } => {
                require_concentrated_venue(&self.venue, "add_liquidity")?;
                let (token_a, token_b) = concentrated_underlyings(state, nft_key)?;
                helpers::balance::debit(state, &mut delta, &token_a.key, amount_desired.0)?;
                helpers::balance::debit(state, &mut delta, &token_b.key, amount_desired.1)?;
                // The NFT's internal liquidity / tokensOwed mutation is not
                // representable in today's `TokenChange` variants and is
                // out-of-scope for this sub-agent (state crate is read-only).
                // The next sync overwrites the NFT's `LpShape::Concentrated`.
            }
        }

        Ok(delta)
    }
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::amm::{AddLiquidityLiveInputs, AddLiquidityParams, AmmVenue};
    use policy_state::delta::TokenChange;
    use policy_state::eval_context::RequestKind;
    use policy_state::live_field::{DataSource, LiveField};
    use policy_state::primitives::{
        Address, ChainId, PoolRef, Price, ProtocolRef, Time, U128, U256,
    };
    use policy_state::token::{
        Balance, BaseCategory, FiatCurrency, LpShape, PegTarget, RangeSpec, ShareForm,
        TokenHolding, TokenKey, TokenKind, TokenRef,
    };
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    use crate::action::amm::PoolState;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn ctx() -> EvalContext {
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn usdc_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    fn weth_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
            },
        }
    }

    fn fungible_holding(token: &TokenRef, balance: U256) -> TokenHolding {
        let contract = token
            .key
            .contract()
            .copied()
            .unwrap_or_else(|| Address::from([0u8; 20]));
        TokenHolding {
            key: token.key.clone(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 18,
            balance: Balance::fungible(balance),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            last_synced_at: Time::from_unix(1_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract,
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
            metadata: None,
            value_usd: None,
        }
    }

    fn v2_venue() -> AmmVenue {
        AmmVenue::UniswapV2 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc").unwrap(),
            factory: Address::from_str("0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f").unwrap(),
        }
    }

    fn v3_venue() -> AmmVenue {
        AmmVenue::UniswapV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        }
    }

    fn empty_pool_state() -> PoolState {
        PoolState::XyConstant {
            reserve_in: U256::ZERO,
            reserve_out: U256::ZERO,
            fee_bp: 30,
        }
    }

    fn make_live_inputs() -> AddLiquidityLiveInputs {
        AddLiquidityLiveInputs {
            pool_state: LiveField::new(empty_pool_state(), DataSource::UserSupplied, now()),
            current_price: LiveField::new(Price::zero(), DataSource::UserSupplied, now()),
        }
    }

    fn state_with_pair(usdc: U256, weth: U256) -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens
            .insert(usdc_ref().key, fungible_holding(&usdc_ref(), usdc));
        s.tokens
            .insert(weth_ref().key, fungible_holding(&weth_ref(), weth));
        s
    }

    // ----------------------------------------------------------------------
    // Pooled (V2) — happy path, fresh LP receipt
    // ----------------------------------------------------------------------

    /// `Pooled` deposit on a `UniswapV2` venue with no prior LP holding emits,
    /// in order: USDC debit, WETH debit, `Mint` stub for the LP token (keyed
    /// at the venue's pool address), then a positive `BalanceDelta` of
    /// `min_lp_out` on the same LP key.
    #[test]
    fn pooled_v2_fresh_lp_receipt_emits_mint_then_credit() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::from(1_000_000u64));
        let action = AddLiquidityAction {
            venue: v2_venue(),
            params: AddLiquidityParams::Pooled {
                tokens: vec![
                    (usdc_ref(), U256::from(1_000u64)),
                    (weth_ref(), U256::from(2_000u64)),
                ],
                min_lp_out: U256::from(50u64),
                recipient: user(),
            },
            live_inputs: make_live_inputs(),
        };

        let delta = action.apply(&state, &ctx()).unwrap();
        // 2 debits + 1 mint + 1 credit = 4 token changes.
        assert_eq!(delta.token_changes.len(), 4);

        let pool_addr = Address::from_str("0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc").unwrap();
        let lp_key = TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: pool_addr,
        };

        // Debits first
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, usdc_ref().key);
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "1000");
            }
            other => panic!("expected USDC debit, got {other:?}"),
        }
        match &delta.token_changes[1] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, weth_ref().key);
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "2000");
            }
            other => panic!("expected WETH debit, got {other:?}"),
        }
        // Then Mint stub
        match &delta.token_changes[2] {
            TokenChange::Mint { key, kind_hint } => {
                assert_eq!(*key, lp_key);
                assert!(matches!(
                    kind_hint,
                    TokenKind::LpShare {
                        shape: LpShape::Pooled { weights: None },
                        ..
                    }
                ));
            }
            other => panic!("expected LP Mint stub, got {other:?}"),
        }
        // Then positive LP credit
        match &delta.token_changes[3] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, lp_key);
                assert!(d.is_positive());
                assert_eq!(d.unsigned_abs().to_string(), "50");
            }
            other => panic!("expected LP credit, got {other:?}"),
        }
    }

    /// `Pooled` deposit where the user already has the LP holding skips the
    /// `Mint` stub and uses the standard `credit` helper.
    #[test]
    fn pooled_v2_existing_lp_holding_skips_mint() {
        let mut state = state_with_pair(U256::from(1_000_000u64), U256::from(1_000_000u64));
        let pool_addr = Address::from_str("0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc").unwrap();
        let lp_key = TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: pool_addr,
        };
        // Pre-seed an LP holding with non-zero balance.
        let lp_ref = TokenRef {
            key: lp_key.clone(),
        };
        state
            .tokens
            .insert(lp_key, fungible_holding(&lp_ref, U256::from(100u64)));

        let action = AddLiquidityAction {
            venue: v2_venue(),
            params: AddLiquidityParams::Pooled {
                tokens: vec![
                    (usdc_ref(), U256::from(1_000u64)),
                    (weth_ref(), U256::from(2_000u64)),
                ],
                min_lp_out: U256::from(50u64),
                recipient: user(),
            },
            live_inputs: make_live_inputs(),
        };

        let delta = action.apply(&state, &ctx()).unwrap();
        // No Mint stub — exactly 3 BalanceDelta entries.
        assert_eq!(delta.token_changes.len(), 3);
        assert!(
            !delta
                .token_changes
                .iter()
                .any(|tc| matches!(tc, TokenChange::Mint { .. })),
            "expected no Mint stub, got {:?}",
            delta.token_changes
        );
    }

    /// Underflow on an underlying token surfaces as `Invariant` and emits an
    /// empty delta (subsequent changes are short-circuited).
    #[test]
    fn pooled_v2_underflow_rejects_with_invariant() {
        let state = state_with_pair(U256::from(100u64), U256::from(100u64));
        let action = AddLiquidityAction {
            venue: v2_venue(),
            params: AddLiquidityParams::Pooled {
                tokens: vec![
                    (usdc_ref(), U256::from(1_000u64)), // > balance
                    (weth_ref(), U256::from(1u64)),
                ],
                min_lp_out: U256::from(1u64),
                recipient: user(),
            },
            live_inputs: make_live_inputs(),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("underflow")));
    }

    /// Empty `tokens` list is an `Invariant` violation (pooled deposit needs
    /// at least one underlying).
    #[test]
    fn pooled_empty_tokens_is_invariant() {
        let state = state_with_pair(U256::from(100u64), U256::from(100u64));
        let action = AddLiquidityAction {
            venue: v2_venue(),
            params: AddLiquidityParams::Pooled {
                tokens: vec![],
                min_lp_out: U256::from(1u64),
                recipient: user(),
            },
            live_inputs: make_live_inputs(),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    /// `Pooled` arm against a concentrated venue (`UniswapV3`) surfaces as
    /// `UnsupportedProtocol` — the venue / param shape mismatch is caught
    /// defensively.
    #[test]
    fn pooled_against_v3_venue_returns_unsupported_protocol() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::from(1_000_000u64));
        let action = AddLiquidityAction {
            venue: v3_venue(),
            params: AddLiquidityParams::Pooled {
                tokens: vec![
                    (usdc_ref(), U256::from(1_000u64)),
                    (weth_ref(), U256::from(2_000u64)),
                ],
                min_lp_out: U256::from(50u64),
                recipient: user(),
            },
            live_inputs: make_live_inputs(),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        match err {
            ReducerError::UnsupportedProtocol { action, protocol } => {
                assert_eq!(action, "add_liquidity");
                assert_eq!(protocol, "uniswap_v3");
            }
            other => panic!("expected UnsupportedProtocol, got {other:?}"),
        }
    }

    // ----------------------------------------------------------------------
    // ConcentratedMint (V3) — happy path
    // ----------------------------------------------------------------------

    /// `ConcentratedMint` on a `UniswapV3` venue debits both pair tokens at
    /// `amount_desired` and emits no NFT-side change (`token_id` unknown at
    /// sign time).
    #[test]
    fn concentrated_mint_v3_debits_pair_only() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::from(1_000_000u64));
        let action = AddLiquidityAction {
            venue: v3_venue(),
            params: AddLiquidityParams::ConcentratedMint {
                pool_pair: (usdc_ref(), weth_ref()),
                amount_desired: (U256::from(1_000u64), U256::from(2_000u64)),
                amount_min: (U256::from(900u64), U256::from(1_800u64)),
                range: RangeSpec::Tick {
                    lower: -100,
                    upper: 100,
                    liquidity: U128::from(0u64),
                },
                recipient: user(),
            },
            live_inputs: make_live_inputs(),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 2);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, usdc_ref().key);
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "1000");
            }
            other => panic!("expected USDC debit, got {other:?}"),
        }
        match &delta.token_changes[1] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, weth_ref().key);
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "2000");
            }
            other => panic!("expected WETH debit, got {other:?}"),
        }
    }

    /// `ConcentratedMint` against a non-concentrated venue (`UniswapV2`)
    /// surfaces as `UnsupportedProtocol`.
    #[test]
    fn concentrated_mint_against_v2_venue_returns_unsupported_protocol() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::from(1_000_000u64));
        let action = AddLiquidityAction {
            venue: v2_venue(),
            params: AddLiquidityParams::ConcentratedMint {
                pool_pair: (usdc_ref(), weth_ref()),
                amount_desired: (U256::from(1_000u64), U256::from(2_000u64)),
                amount_min: (U256::from(900u64), U256::from(1_800u64)),
                range: RangeSpec::Tick {
                    lower: -100,
                    upper: 100,
                    liquidity: U128::from(0u64),
                },
                recipient: user(),
            },
            live_inputs: make_live_inputs(),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::UnsupportedProtocol { .. }));
    }

    // ----------------------------------------------------------------------
    // ConcentratedIncrease (V3) — happy path
    // ----------------------------------------------------------------------

    fn concentrated_nft_key() -> TokenKey {
        TokenKey::Erc721 {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from_str("0xc36442b4a4522e871399cd717abdd847ab11fe88").unwrap(),
            token_id: U256::from(42u64),
        }
    }

    fn concentrated_nft_holding() -> TokenHolding {
        TokenHolding {
            key: concentrated_nft_key(),
            kind: TokenKind::LpShare {
                pool: PoolRef {
                    protocol: ProtocolRef::new("uniswap_v3"),
                    pool_addr: None,
                    pool_id: None,
                    fee_tier: None,
                },
                underlyings: vec![usdc_ref(), weth_ref()],
                share_form: ShareForm::NonFungible,
                shape: LpShape::Concentrated {
                    range: RangeSpec::Tick {
                        lower: -100,
                        upper: 100,
                        liquidity: U128::from(1_000u64),
                    },
                    fees_owed: vec![],
                },
            },
            symbol: "UNI-V3-POS".into(),
            decimals: 0,
            balance: Balance::Owned,
            committed: Balance::Owned,
            approved_to: None,
            price_usd: None,
            last_synced_at: Time::from_unix(1_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract: Address::from_str("0xc36442b4a4522e871399cd717abdd847ab11fe88").unwrap(),
                function: "positions(uint256)".into(),
                decoder_id: "uniswap_v3_position".into(),
            },
            metadata: None,
            value_usd: None,
        }
    }

    /// `ConcentratedIncrease` looks up the NFT's `underlyings` and debits the
    /// two pair tokens. No NFT-side change is emitted (internal liquidity
    /// mutation is out-of-scope for this sub-agent's `TokenChange` variants).
    #[test]
    fn concentrated_increase_debits_underlyings_from_nft_holding() {
        let mut state = state_with_pair(U256::from(1_000_000u64), U256::from(1_000_000u64));
        state
            .tokens
            .insert(concentrated_nft_key(), concentrated_nft_holding());

        let action = AddLiquidityAction {
            venue: v3_venue(),
            params: AddLiquidityParams::ConcentratedIncrease {
                nft_key: concentrated_nft_key(),
                amount_desired: (U256::from(500u64), U256::from(1_000u64)),
                amount_min: (U256::from(450u64), U256::from(900u64)),
            },
            live_inputs: make_live_inputs(),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 2);
    }

    /// `ConcentratedIncrease` against a missing NFT holding returns
    /// `TokenNotFound`.
    #[test]
    fn concentrated_increase_missing_nft_returns_token_not_found() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::from(1_000_000u64));
        let action = AddLiquidityAction {
            venue: v3_venue(),
            params: AddLiquidityParams::ConcentratedIncrease {
                nft_key: concentrated_nft_key(),
                amount_desired: (U256::from(500u64), U256::from(1_000u64)),
                amount_min: (U256::from(450u64), U256::from(900u64)),
            },
            live_inputs: make_live_inputs(),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::TokenNotFound(_)));
    }

    /// `ConcentratedIncrease` against an NFT holding whose kind is not
    /// `LpShare(Concentrated)` surfaces as `Invariant`.
    #[test]
    fn concentrated_increase_wrong_kind_returns_invariant() {
        let mut state = state_with_pair(U256::from(1_000_000u64), U256::from(1_000_000u64));
        // Insert an NFT with wrong kind.
        let mut bogus = concentrated_nft_holding();
        bogus.kind = TokenKind::Unknown;
        state.tokens.insert(concentrated_nft_key(), bogus);

        let action = AddLiquidityAction {
            venue: v3_venue(),
            params: AddLiquidityParams::ConcentratedIncrease {
                nft_key: concentrated_nft_key(),
                amount_desired: (U256::from(500u64), U256::from(1_000u64)),
                amount_min: (U256::from(450u64), U256::from(900u64)),
            },
            live_inputs: make_live_inputs(),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    /// `pool_id_to_address` decodes the leading 20 bytes of a Balancer-style
    /// `bytes32` pool id.
    #[test]
    fn pool_id_to_address_decodes_20_byte_prefix() {
        // Address 0xba12222222228d8ba445958a75a0704d566bf2c8 followed by 12 bytes pad.
        let pool_id = "0xba12222222228d8ba445958a75a0704d566bf2c8000200000000000000000001";
        let addr = pool_id_to_address(pool_id).unwrap();
        let expected = Address::from_str("0xba12222222228d8ba445958a75a0704d566bf2c8").unwrap();
        assert_eq!(addr, expected);
    }

    /// `pool_id_to_address` rejects a short / malformed id.
    #[test]
    fn pool_id_to_address_too_short_errors() {
        let err = pool_id_to_address("0xdead").unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }
}
