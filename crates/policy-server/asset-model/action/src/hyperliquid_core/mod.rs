//! Hyperliquid CORE actions — the thin, off-chain L1 action model.
//! Unlike [`PerpAction`](crate::perp::PerpAction), which carries
//! venue-live inputs (mark price, order book, account state) that an order
//! payload does NOT contain, a Hyperliquid `/exchange` request is a small,
//! self-describing JSON intent signed by an agent key. This module models only
//! the order-/transfer-intrinsic fields the request actually carries, so the
//! policy engine can evaluate it WITHOUT fetching any live data from the venue.
//! v1 covers the high-risk subset: an order, a leverage change, and the three
//! fund-movement / delegation actions (`withdraw3`, `usdSend`, `approveAgent`)
//! that move or authorize control of funds.
//! ## Tag naming
//! The serde `action` tags are prefixed `hl_` (`hl_order`, `hl_withdraw`, …)
//! so they are globally unique across every domain's action set — notably
//! `withdraw` is already a Lending tag, and the engine's flat action registries
//! require unique tags. Policies match on these prefixed tags.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, Decimal};

/// A Hyperliquid CORE action, decoded from a `/exchange` POST body.
/// The serde `action` tag is the source of truth for the trigger tag a policy
/// matches on; [`Self::action_tag`] returns the same string and is verified
/// against serde by the `action_tag_matches_serde` test.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action")]
pub enum HyperliquidCoreAction {
    /// Place an order (`{"type":"order"}`, one leg of `orders[]`).
    #[serde(rename = "hl_order")]
    Order(HlOrderAction),
    /// Change leverage for a market (`{"type":"updateLeverage"}`).
    #[serde(rename = "hl_update_leverage")]
    UpdateLeverage(HlUpdateLeverageAction),
    /// Withdraw USDC off the L1 to a destination (`{"type":"withdraw3"}`).
    #[serde(rename = "hl_withdraw")]
    Withdraw(HlWithdrawAction),
    /// Send USDC to another account (`{"type":"usdSend"}`).
    #[serde(rename = "hl_usd_send")]
    UsdSend(HlUsdSendAction),
    /// Authorize an agent (API) wallet to sign on the account's behalf
    /// (`{"type":"approveAgent"}`).
    #[serde(rename = "hl_approve_agent")]
    ApproveAgent(HlApproveAgentAction),
    /// Transfer a spot token off the account (`{"type":"spotSend"}`).
    #[serde(rename = "hl_spot_send")]
    SpotSend(HlSpotSendAction),
    /// Move balance between the perp and spot wallets
    /// (`{"type":"usdClassTransfer"}`).
    #[serde(rename = "hl_usd_class_transfer")]
    UsdClassTransfer(HlUsdClassTransferAction),
    /// Send a token across DEXes / accounts (`{"type":"sendAsset"}`).
    #[serde(rename = "hl_send_asset")]
    SendAsset(HlSendAssetAction),
    /// Bridge a token to an EVM recipient with arbitrary calldata
    /// (`{"type":"sendToEvmWithData"}`). Highest-risk fund movement.
    #[serde(rename = "hl_send_to_evm_with_data")]
    SendToEvmWithData(HlSendToEvmWithDataAction),
    /// Deposit into HYPE staking (`{"type":"cDeposit"}`).
    #[serde(rename = "hl_c_deposit")]
    CDeposit(HlCDepositAction),
    /// Withdraw from HYPE staking (`{"type":"cWithdraw"}`).
    #[serde(rename = "hl_c_withdraw")]
    CWithdraw(HlCWithdrawAction),
    /// Deposit into / withdraw from a vault (`{"type":"vaultTransfer"}`).
    #[serde(rename = "hl_vault_transfer")]
    VaultTransfer(HlVaultTransferAction),
    /// Move USDC to / from a sub-account (`{"type":"subAccountTransfer"}`).
    #[serde(rename = "hl_sub_account_transfer")]
    SubAccountTransfer(HlSubAccountTransferAction),
    /// Authorize a builder to charge a fee (`{"type":"approveBuilderFee"}`).
    #[serde(rename = "hl_approve_builder_fee")]
    ApproveBuilderFee(HlApproveBuilderFeeAction),
    /// Delegate / undelegate stake to a validator (`{"type":"tokenDelegate"}`).
    #[serde(rename = "hl_token_delegate")]
    TokenDelegate(HlTokenDelegateAction),
    /// Place a TWAP order (`{"type":"twapOrder"}`).
    #[serde(rename = "hl_twap_order")]
    TwapOrder(HlTwapOrderAction),
    /// Add / remove isolated margin (`{"type":"updateIsolatedMargin"}`).
    #[serde(rename = "hl_update_isolated_margin")]
    UpdateIsolatedMargin(HlUpdateIsolatedMarginAction),
    /// Any `/exchange` action not explicitly modeled above. Carries only the raw
    /// wire `type` string so a policy can gate or surface unrecognized actions
    /// (`HlUnknown` — policy default: warn / deny). Closes the silent-allow gap:
    /// every non-benign `/exchange` action reaches the engine rather than passing
    /// through unevaluated.
    #[serde(rename = "hl_unknown")]
    Unknown(HlUnknownAction),
}

impl HyperliquidCoreAction {
    /// The serde `action` tag — the trigger tag a policy matches on.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::Order(_) => "hl_order",
            Self::UpdateLeverage(_) => "hl_update_leverage",
            Self::Withdraw(_) => "hl_withdraw",
            Self::UsdSend(_) => "hl_usd_send",
            Self::ApproveAgent(_) => "hl_approve_agent",
            Self::SpotSend(_) => "hl_spot_send",
            Self::UsdClassTransfer(_) => "hl_usd_class_transfer",
            Self::SendAsset(_) => "hl_send_asset",
            Self::SendToEvmWithData(_) => "hl_send_to_evm_with_data",
            Self::CDeposit(_) => "hl_c_deposit",
            Self::CWithdraw(_) => "hl_c_withdraw",
            Self::VaultTransfer(_) => "hl_vault_transfer",
            Self::SubAccountTransfer(_) => "hl_sub_account_transfer",
            Self::ApproveBuilderFee(_) => "hl_approve_builder_fee",
            Self::TokenDelegate(_) => "hl_token_delegate",
            Self::TwapOrder(_) => "hl_twap_order",
            Self::UpdateIsolatedMargin(_) => "hl_update_isolated_margin",
            Self::Unknown(_) => "hl_unknown",
        }
    }

    /// Every Hyperliquid CORE action is on the `"hyperliquid"` venue, so policies
    /// can scope on `context.venue.name == "hyperliquid"`.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        Some("hyperliquid")
    }
}

/// Place-order leg: `orders[i]` of a `{"type":"order"}` action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlOrderAction {
    /// Asset index (`a`): perp = `meta.universe` index; spot = 10000 + spot idx.
    pub asset_index: u32,
    /// Resolved market symbol (e.g. `"BTC"`); `None` until the venue meta cache
    /// resolves the numeric index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub symbol: Option<String>,
    /// `b` — `true` ⇒ long/buy, `false` ⇒ short/sell.
    pub is_buy: bool,
    /// Limit price (`p`), a decimal value held as a string (fractional-safe).
    pub price: Decimal,
    /// Size in base units (`s`), a decimal value held as a string.
    pub size: Decimal,
    /// `r` — reduce-only.
    pub reduce_only: bool,
    /// Time-in-force (`gtc` / `ioc` / `post_only`), normalized from `t`.
    pub tif: String,
}

/// Leverage change: `{"type":"updateLeverage"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlUpdateLeverageAction {
    /// Asset index (`asset`).
    pub asset_index: u32,
    /// Resolved market symbol; `None` until resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub symbol: Option<String>,
    /// `isCross` — cross (`true`) vs isolated (`false`) margin.
    pub is_cross: bool,
    /// New leverage multiplier (`leverage`).
    pub leverage: u32,
}

/// USDC withdrawal off the L1: `{"type":"withdraw3"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlWithdrawAction {
    /// Destination address funds are withdrawn to (`destination`).
    #[tsify(type = "string")]
    pub destination: Address,
    /// USDC amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
}

/// USDC transfer to another account: `{"type":"usdSend"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlUsdSendAction {
    /// Recipient address (`destination`).
    #[tsify(type = "string")]
    pub destination: Address,
    /// USDC amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
}

/// Agent-wallet authorization: `{"type":"approveAgent"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlApproveAgentAction {
    /// Agent (API) wallet address being authorized (`agentAddress`).
    #[tsify(type = "string")]
    pub agent_address: Address,
    /// Optional human-readable agent name (`agentName`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub agent_name: Option<String>,
}

/// Spot token transfer off the account: `{"type":"spotSend"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlSpotSendAction {
    /// Recipient address (`destination`).
    #[tsify(type = "string")]
    pub destination: Address,
    /// Token identifier (`token`), e.g. `"USDC:0x..."`.
    pub token: String,
    /// Amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
}

/// Move balance between the perp and spot wallets: `{"type":"usdClassTransfer"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlUsdClassTransferAction {
    /// USDC amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
    /// `toPerp` — `true` ⇒ spot → perp, `false` ⇒ perp → spot.
    pub to_perp: bool,
}

/// Send a token across DEXes / accounts: `{"type":"sendAsset"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlSendAssetAction {
    /// Recipient address (`destination`).
    #[tsify(type = "string")]
    pub destination: Address,
    /// Source DEX name (`sourceDex`).
    pub source_dex: String,
    /// Destination DEX name (`destinationDex`).
    pub destination_dex: String,
    /// Token identifier (`token`).
    pub token: String,
    /// Amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
}

/// Bridge a token to an EVM recipient with arbitrary calldata.
///
/// `{"type":"sendToEvmWithData"}` — the highest-risk fund movement: funds leave
/// `HyperCore` for an arbitrary EVM address with attacker-controllable `data`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlSendToEvmWithDataAction {
    /// Token identifier (`token`).
    pub token: String,
    /// Amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
    /// Source DEX name (`sourceDex`).
    pub source_dex: String,
    /// EVM recipient address (`destinationRecipient`).
    #[tsify(type = "string")]
    pub destination_recipient: Address,
    /// Raw calldata forwarded to the recipient (`data`), 0x-hex.
    pub data: String,
}

/// Deposit into HYPE staking: `{"type":"cDeposit"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlCDepositAction {
    /// Amount in token wei (`wei`), a decimal value held as a string.
    pub wei: Decimal,
}

/// Withdraw from HYPE staking: `{"type":"cWithdraw"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlCWithdrawAction {
    /// Amount in token wei (`wei`), a decimal value held as a string.
    pub wei: Decimal,
}

/// Vault deposit / withdrawal: `{"type":"vaultTransfer"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlVaultTransferAction {
    /// Vault address (`vaultAddress`).
    #[tsify(type = "string")]
    pub vault_address: Address,
    /// `isDeposit` — `true` ⇒ deposit into vault, `false` ⇒ withdraw.
    pub is_deposit: bool,
    /// USD amount (`usd`), a decimal value held as a string.
    pub usd: Decimal,
}

/// Sub-account USDC transfer: `{"type":"subAccountTransfer"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlSubAccountTransferAction {
    /// Sub-account address (`subAccountUser`).
    #[tsify(type = "string")]
    pub sub_account_user: Address,
    /// `isDeposit` — `true` ⇒ fund the sub-account, `false` ⇒ pull from it.
    pub is_deposit: bool,
    /// USD amount (`usd`), a decimal value held as a string.
    pub usd: Decimal,
}

/// Builder-fee authorization: `{"type":"approveBuilderFee"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlApproveBuilderFeeAction {
    /// Maximum fee rate the builder may charge (`maxFeeRate`), e.g. `"0.001%"`.
    /// Kept as the raw wire string (it carries a `%` suffix, not a plain decimal).
    pub max_fee_rate: String,
    /// Builder address being authorized (`builder`).
    #[tsify(type = "string")]
    pub builder: Address,
}

/// Stake delegation / undelegation: `{"type":"tokenDelegate"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlTokenDelegateAction {
    /// Validator address (`validator`).
    #[tsify(type = "string")]
    pub validator: Address,
    /// `isUndelegate` — `true` ⇒ undelegate, `false` ⇒ delegate.
    pub is_undelegate: bool,
    /// Amount in token wei (`wei`), a decimal value held as a string.
    pub wei: Decimal,
}

/// TWAP order: `{"type":"twapOrder"}` (`twap` sub-object).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlTwapOrderAction {
    /// Asset index (`twap.a`).
    pub asset_index: u32,
    /// Resolved market symbol; `None` until the venue meta cache resolves it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub symbol: Option<String>,
    /// `twap.b` — `true` ⇒ buy/long, `false` ⇒ sell/short.
    pub is_buy: bool,
    /// Total size (`twap.s`), a decimal value held as a string.
    pub size: Decimal,
    /// `twap.r` — reduce-only.
    pub reduce_only: bool,
    /// Duration in minutes the TWAP runs over (`twap.m`).
    pub minutes: u32,
    /// `twap.t` — randomize sub-order timing.
    pub randomize: bool,
}

/// Isolated-margin adjustment: `{"type":"updateIsolatedMargin"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlUpdateIsolatedMarginAction {
    /// Asset index (`asset`).
    pub asset_index: u32,
    /// Resolved market symbol; `None` until resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub symbol: Option<String>,
    /// `isBuy` — side the margin adjustment applies to.
    pub is_buy: bool,
    /// Notional to add (positive) or remove (negative) (`ntli`), a decimal value
    /// held as a string (can be negative).
    pub ntli: Decimal,
}

/// Catch-all for an `/exchange` action not explicitly modeled.
///
/// Holds only the raw wire `type` string (`{"type":"<actionType>"}`) — no
/// per-action fields — so a policy can `forbid`/`warn` on
/// `HyperliquidCore::Action::"HlUnknown"` or scope on `context.actionType`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlUnknownAction {
    /// The raw `/exchange` wire `type` string (e.g. `"convertToMultiSigUser"`).
    pub action_type: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn order() -> HyperliquidCoreAction {
        HyperliquidCoreAction::Order(HlOrderAction {
            asset_index: 0,
            symbol: Some("BTC".to_owned()),
            is_buy: false,
            price: Decimal::new("60000"),
            size: Decimal::new("0.1"),
            reduce_only: false,
            tif: "gtc".to_owned(),
        })
    }

    /// `action_tag()` must equal the serde `action` discriminant for every
    /// variant — a policy trigger matches on the serde tag.
    #[test]
    fn action_tag_matches_serde() {
        let cases: Vec<HyperliquidCoreAction> = vec![
            order(),
            HyperliquidCoreAction::UpdateLeverage(HlUpdateLeverageAction {
                asset_index: 0,
                symbol: None,
                is_cross: true,
                leverage: 5,
            }),
            HyperliquidCoreAction::Withdraw(HlWithdrawAction {
                destination: Address::from([0x11; 20]),
                amount: Decimal::new("100"),
            }),
            HyperliquidCoreAction::UsdSend(HlUsdSendAction {
                destination: Address::from([0x22; 20]),
                amount: Decimal::new("50"),
            }),
            HyperliquidCoreAction::ApproveAgent(HlApproveAgentAction {
                agent_address: Address::from([0x33; 20]),
                agent_name: None,
            }),
            HyperliquidCoreAction::SpotSend(HlSpotSendAction {
                destination: Address::from([0x44; 20]),
                token: "USDC:0xdeadbeef".to_owned(),
                amount: Decimal::new("500"),
            }),
            HyperliquidCoreAction::UsdClassTransfer(HlUsdClassTransferAction {
                amount: Decimal::new("100"),
                to_perp: true,
            }),
            HyperliquidCoreAction::SendAsset(HlSendAssetAction {
                destination: Address::from([0x55; 20]),
                source_dex: String::new(),
                destination_dex: "perp".to_owned(),
                token: "USDC".to_owned(),
                amount: Decimal::new("25"),
            }),
            HyperliquidCoreAction::SendToEvmWithData(HlSendToEvmWithDataAction {
                token: "USDC".to_owned(),
                amount: Decimal::new("1"),
                source_dex: String::new(),
                destination_recipient: Address::from([0x66; 20]),
                data: "0x".to_owned(),
            }),
            HyperliquidCoreAction::CDeposit(HlCDepositAction {
                wei: Decimal::new("1000000000"),
            }),
            HyperliquidCoreAction::CWithdraw(HlCWithdrawAction {
                wei: Decimal::new("1000000000"),
            }),
            HyperliquidCoreAction::VaultTransfer(HlVaultTransferAction {
                vault_address: Address::from([0x77; 20]),
                is_deposit: true,
                usd: Decimal::new("250"),
            }),
            HyperliquidCoreAction::SubAccountTransfer(HlSubAccountTransferAction {
                sub_account_user: Address::from([0x88; 20]),
                is_deposit: false,
                usd: Decimal::new("75"),
            }),
            HyperliquidCoreAction::ApproveBuilderFee(HlApproveBuilderFeeAction {
                max_fee_rate: "0.001%".to_owned(),
                builder: Address::from([0x99; 20]),
            }),
            HyperliquidCoreAction::TokenDelegate(HlTokenDelegateAction {
                validator: Address::from([0xaa; 20]),
                is_undelegate: false,
                wei: Decimal::new("1000000000"),
            }),
            HyperliquidCoreAction::TwapOrder(HlTwapOrderAction {
                asset_index: 0,
                symbol: None,
                is_buy: true,
                size: Decimal::new("10"),
                reduce_only: false,
                minutes: 30,
                randomize: true,
            }),
            HyperliquidCoreAction::UpdateIsolatedMargin(HlUpdateIsolatedMarginAction {
                asset_index: 0,
                symbol: None,
                is_buy: true,
                ntli: Decimal::new("-100"),
            }),
            HyperliquidCoreAction::Unknown(HlUnknownAction {
                action_type: "convertToMultiSigUser".to_owned(),
            }),
        ];
        for c in cases {
            let json = serde_json::to_value(&c).unwrap();
            let serde_tag = json.get("action").and_then(serde_json::Value::as_str);
            assert_eq!(
                serde_tag,
                Some(c.action_tag()),
                "serde `action` tag must equal action_tag()"
            );
        }
    }

    /// Fractional price/size must round-trip (the whole reason we use `Decimal`,
    /// not `U256`, which rejects `"0.1"`).
    #[test]
    fn fractional_size_round_trips() {
        let json = serde_json::to_string(&order()).unwrap();
        assert!(
            json.contains("\"0.1\""),
            "fractional size preserved: {json}"
        );
        let back: HyperliquidCoreAction = serde_json::from_str(&json).unwrap();
        assert_eq!(back, order());
    }
}
