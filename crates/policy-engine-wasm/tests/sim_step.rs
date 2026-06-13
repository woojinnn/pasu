//! Integration tests for `simulate_step_json`.
//!
//! Drives the JSON wire boundary: build a typed `WalletState` + `Action` +
//! `EvalContext`, serialize to JSON, call `simulate_step_json`, and assert the
//! `{ ok, data: { delta, next_state } }` envelope. Native (non-wasm) tests
//! suffice — `#[wasm_bindgen]` is a no-op when not targeting wasm, so the same
//! function the SW calls runs unchanged here.

use std::str::FromStr;

use policy_state::delta::TokenChange;
use policy_state::eval_context::RequestKind;
use policy_state::live_field::{DataSource, LiveField};
use policy_state::primitives::{Address, ChainId, Time, U256};
use policy_state::token::{
    Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
};
use policy_state::wallet::{WalletId, WalletState};
use policy_state::EvalContext;
use policy_state::StateDelta;
use policy_transition::action::token::Erc20TransferAction;
use policy_transition::action::{Action, ActionBody, ActionMeta, ActionNature, TokenAction};
use serde::Deserialize;
use serde_json::json;

use policy_engine_wasm::simulate_step_json;

// ── fixtures ──────────────────────────────────────────────────────────────

fn now() -> Time {
    Time::from_unix(1_738_000_000)
}

fn user() -> Address {
    Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
}

fn recipient() -> Address {
    Address::from_str("0x000000000000000000000000000000000000beef").unwrap()
}

fn usdc_ref() -> TokenRef {
    TokenRef {
        key: TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        },
    }
}

fn empty_state() -> WalletState {
    WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]))
}

fn make_usdc_holding(amount: u128) -> TokenHolding {
    let key = usdc_ref().key;
    let contract = key
        .contract()
        .copied()
        .unwrap_or_else(|| Address::from([0u8; 20]));
    TokenHolding {
        key,
        kind: TokenKind::Base {
            category: BaseCategory::Stable,
            peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
        },
        symbol: "USDC".into(),
        decimals: 6,
        balance: Balance::fungible(U256::from(amount)),
        committed: Balance::zero_fungible(),
        approved_to: None,
        price_usd: None,
        metadata: None,
        value_usd: None,
        last_synced_at: Time::from_unix(1_000_000),
        primitives_source: DataSource::OnchainView {
            chain: ChainId::ethereum_mainnet(),
            contract,
            function: "balanceOf(address)".into(),
            decoder_id: "erc20_balance".into(),
        },
    }
}

fn state_with_usdc(amount: u128) -> WalletState {
    let mut s = empty_state();
    let h = make_usdc_holding(amount);
    s.tokens.insert(h.key.clone(), h);
    s
}

fn ctx() -> EvalContext {
    EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
}

fn gas_price_field() -> LiveField<U256> {
    LiveField::new(
        U256::from(20_000_000_000u64),
        DataSource::OnchainView {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from([0u8; 20]),
            function: "gasPrice()".into(),
            decoder_id: "stub".into(),
        },
        now(),
    )
}

fn erc20_transfer_action(amount: u128) -> Action {
    Action {
        meta: ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 0,
                gas_limit: U256::from(100_000u64),
                gas_price: gas_price_field(),
                value: U256::from(0u64),
            },
        },
        body: ActionBody::Token(TokenAction::Erc20Transfer(Erc20TransferAction {
            token: usdc_ref(),
            recipient: recipient(),
            amount: U256::from(amount),
            is_router_egress: false,
        })),
    }
}

// ── envelope types (mirror the wasm boundary) ─────────────────────────────

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<EnvelopeError>,
}

#[derive(Debug, Deserialize)]
struct EnvelopeError {
    kind: String,
    #[allow(dead_code)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct StepOk {
    delta: StateDelta,
    next_state: WalletState,
}

fn call_step(state: &WalletState, action: &Action, ctx: &EvalContext) -> Envelope<StepOk> {
    let input = json!({ "state": state, "action": action, "ctx": ctx }).to_string();
    let out = simulate_step_json(input);
    serde_json::from_str(&out).expect("envelope parses")
}

// ── tests ─────────────────────────────────────────────────────────────────

#[test]
fn happy_path_erc20_transfer_produces_delta_and_decremented_state() {
    let state = state_with_usdc(1_000_000_000);
    let action = erc20_transfer_action(250_000_000);

    let env = call_step(&state, &action, &ctx());
    assert!(env.ok, "expected ok envelope, got {:?}", env.error);
    let data = env.data.expect("data present on ok");

    // delta carries the negative balance change for the sender's USDC.
    assert_eq!(data.delta.token_changes.len(), 1, "one balance delta");
    let TokenChange::BalanceDelta { key, delta: d } = &data.delta.token_changes[0] else {
        panic!(
            "expected BalanceDelta, got {:?}",
            data.delta.token_changes[0]
        );
    };
    assert_eq!(*key, usdc_ref().key);
    assert!(
        d.is_negative(),
        "balance delta must be negative on transfer"
    );

    // next_state already has the delta composed in: 1_000_000_000 - 250_000_000.
    let holding = data
        .next_state
        .tokens
        .get(&usdc_ref().key)
        .expect("USDC holding still present");
    assert_eq!(
        holding.balance.as_fungible().unwrap(),
        U256::from(750_000_000u64),
        "next_state should reflect post-transfer balance",
    );

    // Source state untouched — the WASM boundary is pure.
    let pre = state.tokens.get(&usdc_ref().key).unwrap();
    assert_eq!(
        pre.balance.as_fungible().unwrap(),
        U256::from(1_000_000_000u64),
        "input state must remain unchanged",
    );
}

#[test]
fn sequential_steps_thread_state_forward() {
    // Mirrors the host loop: feed next_state as the state of the next call.
    let mut state = state_with_usdc(1_000);

    let step1 = call_step(&state, &erc20_transfer_action(300), &ctx());
    assert!(step1.ok, "step1 ok: {:?}", step1.error);
    state = step1.data.unwrap().next_state;
    assert_eq!(
        state
            .tokens
            .get(&usdc_ref().key)
            .unwrap()
            .balance
            .as_fungible()
            .unwrap(),
        U256::from(700u64),
    );

    let step2 = call_step(&state, &erc20_transfer_action(200), &ctx());
    assert!(step2.ok, "step2 ok: {:?}", step2.error);
    state = step2.data.unwrap().next_state;
    assert_eq!(
        state
            .tokens
            .get(&usdc_ref().key)
            .unwrap()
            .balance
            .as_fungible()
            .unwrap(),
        U256::from(500u64),
        "two sequential transfers compose: 1000 - 300 - 200 = 500",
    );
}

#[test]
fn transfer_exceeding_balance_surfaces_apply_failed() {
    // The reducer's `apply` rejects an Erc20Transfer whose amount exceeds the
    // sender's balance with `ReducerError::Invariant` — the failure surfaces
    // before `apply_delta` ever runs, so the error kind is `apply_failed`.
    let state = state_with_usdc(100);
    let action = erc20_transfer_action(101);

    let env = call_step(&state, &action, &ctx());
    assert!(!env.ok, "expected error envelope");
    let err = env.error.expect("error present on fail");
    assert_eq!(
        err.kind, "apply_failed",
        "underflow surfaces at apply, before apply_delta",
    );
}

#[test]
fn invalid_input_json_returns_invalid_input_kind() {
    let out = simulate_step_json("{ not json".to_string());
    let env: Envelope<StepOk> = serde_json::from_str(&out).expect("envelope parses");
    assert!(!env.ok);
    assert_eq!(env.error.unwrap().kind, "invalid_input");
}
