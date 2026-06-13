//! `Token::Erc20Transfer` lowering → `Token::Erc20TransferContext`.

use serde_json::{Map, Value};

use policy_transition::action::token::Erc20TransferAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Token::Erc20Transfer` action into the `Token::Erc20TransferContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &Erc20TransferAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    if let Some(nano) = ctx.amount_nano(&action.token, action.amount) {
        m.insert("amountNano".into(), Value::from(nano));
    }
    // Surface the router-egress flag ONLY when set, so a normal user transfer's
    // context is byte-identical and the `sweep-recipient-not-self` policy stays
    // dormant for it (no alarm fatigue). See `Erc20TransferAction.is_router_egress`.
    if action.is_router_egress {
        m.insert("is_router_egress".into(), Value::Bool(true));
    }
    // `amountUsd` / `custom` are host-populated — OMITTED here.

    Ok(ctx.lowered(r#"Token::Action::"Erc20Transfer""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use policy_state::primitives::U256;
    use policy_state::token::TokenRef;
    use policy_transition::action::token::{Erc20TransferAction, TokenAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        onchain_meta, recipient, sample_erc1155_token, sample_erc20_token, sample_native_key,
    };

    /// Gate an `Erc20Transfer` carrying the given `token` ref. The `token` slot
    /// is the only place `lower_token_ref` / `lower_token_key` runs in this
    /// action, so varying the `TokenKey` standard exercises every `Core::TokenKey`
    /// discriminator arm end-to-end against the schema.
    fn assert_transfer_token_conforms(token: TokenRef) {
        let body = ActionBody::Token(TokenAction::Erc20Transfer(Erc20TransferAction {
            token,
            recipient: recipient(),
            amount: U256::from(1_234_567u64),
            is_router_egress: false,
        }));
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("erc20_transfer", &body, &meta);
    }

    /// ERC20 `token` (`standard = "erc20"`, carries `address`).
    #[test]
    fn erc20_transfer_lowering_conforms_to_schema() {
        assert_transfer_token_conforms(sample_erc20_token());
    }

    /// Native `token` (`standard = "native"`) — the `lower_token_key` arm that
    /// emits NEITHER `address` nor `contract`/`tokenId`.
    #[test]
    fn erc20_transfer_native_token_key_conforms() {
        assert_transfer_token_conforms(TokenRef {
            key: sample_native_key(),
        });
    }

    /// ERC1155 `token` (`standard = "erc1155"`) — the `{ contract, tokenId }`
    /// arm reached via `lower_token_ref`.
    #[test]
    fn erc20_transfer_erc1155_token_key_conforms() {
        assert_transfer_token_conforms(sample_erc1155_token());
    }

    /// A router-egress transfer (Uniswap `sweepToken` / `unwrapWETH9`) emits the
    /// `is_router_egress` context flag so a policy can gate a redirected sweep;
    /// a normal user transfer (the default) OMITS it (no alarm-fatigue surface).
    #[test]
    fn erc20_transfer_emits_is_router_egress_flag_only_when_set() {
        use crate::lowering_v2::{lower_action, TxMeta};

        const FROM: &str = "0x1111111111111111111111111111111111111111";
        const TO: &str = "0x2222222222222222222222222222222222222222";
        let meta = onchain_meta();
        let tx = TxMeta { from: FROM, to: TO };

        let sweep = ActionBody::Token(TokenAction::Erc20Transfer(Erc20TransferAction {
            token: sample_erc20_token(),
            recipient: recipient(),
            amount: U256::from(1u64),
            is_router_egress: true,
        }));
        let lowered = lower_action(&sweep, &meta, &tx).unwrap();
        assert_eq!(
            lowered.context["is_router_egress"],
            serde_json::json!(true),
            "router egress must surface the flag"
        );
        // The egress context (with the new flag) must validate against the schema.
        super::super::test_support::assert_conforms("erc20_transfer", &sweep, &meta);

        let normal = ActionBody::Token(TokenAction::Erc20Transfer(Erc20TransferAction {
            token: sample_erc20_token(),
            recipient: recipient(),
            amount: U256::from(1u64),
            is_router_egress: false,
        }));
        let lowered_normal = lower_action(&normal, &meta, &tx).unwrap();
        assert!(
            lowered_normal.context.get("is_router_egress").is_none(),
            "a normal user transfer must NOT surface the flag"
        );
    }

    /// With injected `decimals`, the lowering fills `amountNano` (the
    /// token-native `Long` sibling the quantity-cap policies read); without
    /// them it is omitted. 1000 USDC (6dp) raw `1_000_000_000` → nano `1e12`.
    #[test]
    fn erc20_transfer_fills_amount_nano_only_when_decimals_known() {
        use std::collections::BTreeMap;
        use std::str::FromStr;

        use policy_state::primitives::{Address, ChainId, U256};
        use policy_state::token::{TokenKey, TokenRef};

        use crate::lowering_v2::{lower_action_with_decimals, TokenDecimals, TxMeta};

        const FROM: &str = "0x1111111111111111111111111111111111111111";
        const TO: &str = "0x2222222222222222222222222222222222222222";
        let usdc = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

        let body = ActionBody::Token(TokenAction::Erc20Transfer(Erc20TransferAction {
            token: TokenRef {
                key: TokenKey::Erc20 {
                    chain: ChainId::ethereum_mainnet(),
                    address: Address::from_str(usdc).unwrap(),
                },
            },
            recipient: recipient(),
            amount: U256::from(1_000_000_000u64),
            is_router_egress: false,
        }));
        let meta = onchain_meta();
        let tx = TxMeta { from: FROM, to: TO };

        // WITH decimals → amountNano present and correct.
        let mut map = BTreeMap::new();
        map.insert(usdc.to_owned(), 6u8);
        let lowered =
            lower_action_with_decimals(&body, &meta, &tx, &TokenDecimals::new(map)).unwrap();
        assert_eq!(
            lowered.context["amountNano"],
            serde_json::json!(1_000_000_000_000i64),
            "1000 USDC (6dp) → 1e12 nano"
        );

        // WITHOUT decimals (the default-empty map, e.g. `lower_action`) → omitted.
        let bare =
            lower_action_with_decimals(&body, &meta, &tx, &TokenDecimals::default()).unwrap();
        assert!(
            bare.context.get("amountNano").is_none(),
            "amountNano must be omitted when decimals are unknown"
        );
    }
}
