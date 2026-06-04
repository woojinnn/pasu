//! One-shot fixture emitter: serializes a known-good `(state, action, ctx)`
//! triple as JSON to stdout, so the dashboard can pin a probe payload that
//! always matches the wasm wire shape.
//!
//! Run from the repo root:
//!   cargo run -p policy-engine-wasm --example emit_sim_step_sample \
//!     > browser-extension/dashboard/src/pages/simulation/sim_step_sample.json
//!
//! The fixture mirrors the happy-path test in `tests/sim_step.rs`:
//! wallet holds 1,000,000,000 USDC on mainnet, transfers 250,000,000.
//! Re-run whenever the upstream type shapes change so the dashboard probe
//! stays in lock-step with the wasm boundary.

use std::str::FromStr;

use policy_state::eval_context::RequestKind;
use policy_state::live_field::{DataSource, LiveField};
use policy_state::primitives::{Address, ChainId, Time, U256};
use policy_state::token::{
    Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
};
use policy_state::wallet::{WalletId, WalletState};
use policy_state::EvalContext;
use policy_transition::action::token::Erc20TransferAction;
use policy_transition::action::{Action, ActionBody, ActionMeta, ActionNature, TokenAction};
use serde_json::json;

fn main() {
    let owner = Address::from_str("0x000000000000000000000000000000000000a01c").unwrap();
    let recipient = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
    let usdc = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
    let now = Time::from_unix(1_738_000_000);

    let usdc_ref = TokenRef {
        key: TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: usdc,
        },
    };

    let mut state = WalletState::new(WalletId::new(owner, [ChainId::ethereum_mainnet()]));
    state.tokens.insert(
        usdc_ref.key.clone(),
        TokenHolding {
            key: usdc_ref.key.clone(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::fungible(U256::from(1_000_000_000u64)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract: usdc,
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        },
    );

    let action = Action {
        meta: ActionMeta {
            submitted_at: now,
            submitter: owner,
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 0,
                gas_limit: U256::from(100_000u64),
                gas_price: LiveField::new(
                    U256::from(20_000_000_000u64),
                    DataSource::OnchainView {
                        chain: ChainId::ethereum_mainnet(),
                        contract: Address::from([0u8; 20]),
                        function: "gasPrice()".into(),
                        decoder_id: "stub".into(),
                    },
                    now,
                ),
                value: U256::from(0u64),
            },
        },
        body: ActionBody::Token(TokenAction::Erc20Transfer(Erc20TransferAction {
            token: usdc_ref,
            recipient,
            amount: U256::from(250_000_000u64),
        })),
    };

    let ctx = EvalContext::new(ChainId::ethereum_mainnet(), now, RequestKind::Transaction);

    let payload = json!({ "state": state, "action": action, "ctx": ctx });
    println!("{}", serde_json::to_string_pretty(&payload).unwrap());
}
