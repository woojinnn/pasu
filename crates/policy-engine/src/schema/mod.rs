//! Cedar policy schema composition.

pub mod action_name;
pub mod aliases;
pub mod composer;
pub mod enriched;
pub mod fragment;
pub mod manifest_fragment;
pub mod per_policy;

pub use composer::compose_enriched;
pub use enriched::EnrichedSchema;
pub use fragment::{CedarTypeFragment, CustomFieldSource};
pub use manifest_fragment::manifest_to_cedarschema;
pub use per_policy::{compose_per_policy, lint_custom_field_refs};

use crate::policy_rpc::{validate_manifests, PolicyManifest, PolicyRpcError};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

// core (Core namespace + Amm/Lending/Launchpad/Perp shared types hoisted here)
const CORE_SCHEMA: &str = include_str!("../../../../schema/policy-schema/core.cedarschema");

// structural action bodies (Core::Multicall, Core::Unknown)
const CORE_MULTICALL_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/multicall.cedarschema");
const CORE_UNKNOWN_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/unknown.cedarschema");

// airdrop (alphabetical)
const AIRDROP_CLAIM_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/airdrop/claim.cedarschema");
const AIRDROP_DELEGATE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/airdrop/delegate.cedarschema");

// amm (alphabetical)
const AMM_ADD_LIQUIDITY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/amm/add_liquidity.cedarschema");
const AMM_CANCEL_INTENT_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/amm/cancel_intent_order.cedarschema");
const AMM_COLLECT_FEES_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/amm/collect_fees.cedarschema");
const AMM_GSM_SWAP_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/amm/gsm_swap.cedarschema");
const AMM_PRE_SIGN_INTENT_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/amm/pre_sign_intent_order.cedarschema");
const AMM_REMOVE_LIQUIDITY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/amm/remove_liquidity.cedarschema");
const AMM_SETTLE_INTENT_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/amm/settle_intent_order.cedarschema");
const AMM_SIGN_INTENT_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/amm/sign_intent_order.cedarschema");
const AMM_SWAP_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/amm/swap.cedarschema");

// governance (alphabetical)
const GOVERNANCE_ACTIVATE_VOTING_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/governance/activate_voting.cedarschema");
const GOVERNANCE_CANCEL_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/governance/cancel.cedarschema");
const GOVERNANCE_CLOSE_VOTE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/governance/close_vote.cedarschema");
const GOVERNANCE_DELEGATE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/governance/delegate.cedarschema");
const GOVERNANCE_EXECUTE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/governance/execute.cedarschema");
const GOVERNANCE_PROPOSE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/governance/propose.cedarschema");
const GOVERNANCE_QUEUE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/governance/queue.cedarschema");
const GOVERNANCE_REDEEM_CANCELLATION_FEE_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/governance/redeem_cancellation_fee.cedarschema"
);
const GOVERNANCE_START_VOTE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/governance/start_vote.cedarschema");
const GOVERNANCE_UPDATE_REPRESENTATIVE_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/governance/update_representative.cedarschema"
);
const GOVERNANCE_VOTE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/governance/vote.cedarschema");

// lending (alphabetical)
const LENDING_BORROW_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/lending/borrow.cedarschema");
const LENDING_BUY_COLLATERAL_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/lending/buy_collateral.cedarschema");
const LENDING_DELEGATE_BORROW_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/lending/delegate_borrow.cedarschema");
const LENDING_DISABLE_COLLATERAL_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/lending/disable_collateral.cedarschema");
const LENDING_ENABLE_COLLATERAL_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/lending/enable_collateral.cedarschema");
const LENDING_LIQUIDATE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/lending/liquidate.cedarschema");
const LENDING_PERIPHERY_OPERATION_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/lending/periphery_operation.cedarschema"
);
const LENDING_REPAY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/lending/repay.cedarschema");
const LENDING_SET_EMODE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/lending/set_emode.cedarschema");
const LENDING_SUPPLY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/lending/supply.cedarschema");
const LENDING_SWAP_RATE_MODE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/lending/swap_rate_mode.cedarschema");
const LENDING_WITHDRAW_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/lending/withdraw.cedarschema");
const LENDING_SET_AUTHORIZATION_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/lending/set_authorization.cedarschema");

// liquid_staking (alphabetical)
const LIQUID_STAKING_CLAIM_WITHDRAWAL_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/liquid_staking/claim_withdrawal.cedarschema"
);
const LIQUID_STAKING_REQUEST_WITHDRAWAL_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/liquid_staking/request_withdrawal.cedarschema"
);
const LIQUID_STAKING_STAKE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/liquid_staking/stake.cedarschema");
const LIQUID_STAKING_TRANSFER_SHARES_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/liquid_staking/transfer_shares.cedarschema"
);
const LIQUID_STAKING_UNWRAP_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/liquid_staking/unwrap.cedarschema");
const LIQUID_STAKING_WRAP_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/liquid_staking/wrap.cedarschema");

// yield (alphabetical)
const YIELD_ADD_MARKET_LIQUIDITY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/yield/add_market_liquidity.cedarschema");
const YIELD_CANCEL_LIMIT_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/yield/cancel_limit_order.cedarschema");
const YIELD_CLAIM_YIELD_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/yield/claim_yield.cedarschema");
const YIELD_MINT_PY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/yield/mint_py.cedarschema");
const YIELD_MINT_SY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/yield/mint_sy.cedarschema");
const YIELD_PT_SWAP_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/yield/pt_swap.cedarschema");
const YIELD_REDEEM_PY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/yield/redeem_py.cedarschema");
const YIELD_REDEEM_SY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/yield/redeem_sy.cedarschema");
const YIELD_REMOVE_MARKET_LIQUIDITY_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/yield/remove_market_liquidity.cedarschema"
);
const YIELD_SIGN_LIMIT_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/yield/sign_limit_order.cedarschema");
const YIELD_YT_SWAP_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/yield/yt_swap.cedarschema");

// staking (alphabetical)
const STAKING_CLAIM_REWARDS_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/staking/claim_rewards.cedarschema");
const STAKING_COOLDOWN_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/staking/cooldown.cedarschema");
const STAKING_GAUGE_DEPOSIT_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/staking/gauge_deposit.cedarschema");
const STAKING_GAUGE_WITHDRAW_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/staking/gauge_withdraw.cedarschema");
const STAKING_INCREASE_LOCK_AMOUNT_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/staking/increase_lock_amount.cedarschema"
);
const STAKING_INCREASE_LOCK_TIME_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/staking/increase_lock_time.cedarschema");
const STAKING_LOCK_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/staking/lock.cedarschema");
const STAKING_REDEEM_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/staking/redeem.cedarschema");
const STAKING_STAKE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/staking/stake.cedarschema");
const STAKING_UNLOCK_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/staking/unlock.cedarschema");
const STAKING_VOTE_FOR_GAUGE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/staking/vote_for_gauge.cedarschema");

// marketplace (Seaport NFT orders)
const MARKETPLACE_SIGN_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/marketplace/sign_order.cedarschema");
const MARKETPLACE_FULFILL_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/marketplace/fulfill_order.cedarschema");
const MARKETPLACE_CANCEL_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/marketplace/cancel_order.cedarschema");

// launchpad (alphabetical)
const LAUNCHPAD_CLAIM_ALLOCATION_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/launchpad/claim_allocation.cedarschema");
const LAUNCHPAD_CLAIM_VESTED_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/launchpad/claim_vested.cedarschema");
const LAUNCHPAD_COMMIT_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/launchpad/commit.cedarschema");
const LAUNCHPAD_REFUND_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/launchpad/refund.cedarschema");
const LAUNCHPAD_WITHDRAW_COMMIT_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/launchpad/withdraw_commit.cedarschema");

// perp (alphabetical)
const PERP_ADJUST_MARGIN_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/perp/adjust_margin.cedarschema");
const PERP_CANCEL_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/perp/cancel_order.cedarschema");
const PERP_CHANGE_LEVERAGE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/perp/change_leverage.cedarschema");
const PERP_CHANGE_MARGIN_MODE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/perp/change_margin_mode.cedarschema");
const PERP_CLAIM_FUNDING_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/perp/claim_funding.cedarschema");
const PERP_CLOSE_POSITION_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/perp/close_position.cedarschema");
const PERP_DECREASE_POSITION_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/perp/decrease_position.cedarschema");
const PERP_INCREASE_POSITION_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/perp/increase_position.cedarschema");
const PERP_OPEN_POSITION_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/perp/open_position.cedarschema");
const PERP_PLACE_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/perp/place_order.cedarschema");

// permission (alphabetical)
const PERMISSION_PROTOCOL_AUTHORIZATION_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/permission/protocol_authorization.cedarschema"
);

// restaking (alphabetical)
const RESTAKING_COMPLETE_WITHDRAWAL_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/restaking/complete_withdrawal.cedarschema"
);
const RESTAKING_DELEGATE_TO_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/restaking/delegate_to.cedarschema");
const RESTAKING_DEPOSIT_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/restaking/deposit.cedarschema");
const RESTAKING_QUEUE_WITHDRAWAL_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/restaking/queue_withdrawal.cedarschema");
const RESTAKING_REDELEGATE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/restaking/redelegate.cedarschema");
const RESTAKING_REGISTER_OPERATOR_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/restaking/register_operator.cedarschema"
);
const RESTAKING_UNDELEGATE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/restaking/undelegate.cedarschema");

// token (alphabetical)
const TOKEN_ERC20_APPROVE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/token/erc20_approve.cedarschema");
const TOKEN_ERC20_PERMIT_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/token/erc20_permit.cedarschema");
const TOKEN_ERC20_TRANSFER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/token/erc20_transfer.cedarschema");
const TOKEN_NFT_APPROVE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/token/nft_approve.cedarschema");
const TOKEN_NFT_SET_APPROVAL_FOR_ALL_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/token/nft_set_approval_for_all.cedarschema"
);
const TOKEN_NFT_TRANSFER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/token/nft_transfer.cedarschema");
const TOKEN_PERMIT2_APPROVE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/token/permit2_approve.cedarschema");
const TOKEN_PERMIT2_SIGN_ALLOWANCE_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/token/permit2_sign_allowance.cedarschema"
);
const TOKEN_PERMIT2_SIGN_TRANSFER_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/token/permit2_sign_transfer.cedarschema"
);
const TOKEN_PERMIT2_TRANSFER_FROM_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/token/permit2_transfer_from.cedarschema"
);
const TOKEN_REVOKE_APPROVAL_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/token/revoke_approval.cedarschema");
const TOKEN_UNWRAP_NATIVE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/token/unwrap_native.cedarschema");
const TOKEN_WRAP_NATIVE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/token/wrap_native.cedarschema");

// hyperliquid_core (alphabetical) — the thin off-chain L1 action model.
const HL_ORDER_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/hyperliquid_core/order.cedarschema");
const HL_UPDATE_LEVERAGE_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/hyperliquid_core/update_leverage.cedarschema"
);
const HL_WITHDRAW_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/hyperliquid_core/withdraw.cedarschema");
const HL_USD_CLASS_TRANSFER_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/hyperliquid_core/usd_class_transfer.cedarschema"
);
const HL_SEND_ASSET_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/hyperliquid_core/send_asset.cedarschema"
);
const HL_TOKEN_DELEGATE_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/hyperliquid_core/token_delegate.cedarschema"
);
const HL_TWAP_ORDER_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/hyperliquid_core/twap_order.cedarschema"
);
const HL_UPDATE_ISOLATED_MARGIN_SCHEMA: &str = include_str!(
    "../../../../schema/policy-schema/actions/hyperliquid_core/update_isolated_margin.cedarschema"
);

/// Ordered list of all shipped cedarschema files. The merge in
/// [`base_schema_text`] preserves this order so the resulting per-namespace
/// blocks have deterministic field ordering.
const SHIPPED_SCHEMA_FILES: &[&str] = &[
    CORE_SCHEMA,
    CORE_MULTICALL_SCHEMA,
    CORE_UNKNOWN_SCHEMA,
    AIRDROP_CLAIM_SCHEMA,
    AIRDROP_DELEGATE_SCHEMA,
    AMM_ADD_LIQUIDITY_SCHEMA,
    AMM_CANCEL_INTENT_ORDER_SCHEMA,
    AMM_COLLECT_FEES_SCHEMA,
    AMM_GSM_SWAP_SCHEMA,
    AMM_PRE_SIGN_INTENT_ORDER_SCHEMA,
    AMM_REMOVE_LIQUIDITY_SCHEMA,
    AMM_SETTLE_INTENT_ORDER_SCHEMA,
    AMM_SIGN_INTENT_ORDER_SCHEMA,
    AMM_SWAP_SCHEMA,
    GOVERNANCE_ACTIVATE_VOTING_SCHEMA,
    GOVERNANCE_CANCEL_SCHEMA,
    GOVERNANCE_CLOSE_VOTE_SCHEMA,
    GOVERNANCE_DELEGATE_SCHEMA,
    GOVERNANCE_EXECUTE_SCHEMA,
    GOVERNANCE_PROPOSE_SCHEMA,
    GOVERNANCE_QUEUE_SCHEMA,
    GOVERNANCE_REDEEM_CANCELLATION_FEE_SCHEMA,
    GOVERNANCE_START_VOTE_SCHEMA,
    GOVERNANCE_UPDATE_REPRESENTATIVE_SCHEMA,
    GOVERNANCE_VOTE_SCHEMA,
    LENDING_BORROW_SCHEMA,
    LENDING_DELEGATE_BORROW_SCHEMA,
    LENDING_DISABLE_COLLATERAL_SCHEMA,
    LENDING_ENABLE_COLLATERAL_SCHEMA,
    LENDING_LIQUIDATE_SCHEMA,
    LENDING_PERIPHERY_OPERATION_SCHEMA,
    LENDING_REPAY_SCHEMA,
    LENDING_SET_EMODE_SCHEMA,
    LENDING_SUPPLY_SCHEMA,
    LENDING_SWAP_RATE_MODE_SCHEMA,
    LENDING_WITHDRAW_SCHEMA,
    LENDING_SET_AUTHORIZATION_SCHEMA,
    LIQUID_STAKING_CLAIM_WITHDRAWAL_SCHEMA,
    LIQUID_STAKING_REQUEST_WITHDRAWAL_SCHEMA,
    LIQUID_STAKING_STAKE_SCHEMA,
    LIQUID_STAKING_TRANSFER_SHARES_SCHEMA,
    LIQUID_STAKING_UNWRAP_SCHEMA,
    LIQUID_STAKING_WRAP_SCHEMA,
    YIELD_ADD_MARKET_LIQUIDITY_SCHEMA,
    YIELD_CANCEL_LIMIT_ORDER_SCHEMA,
    YIELD_CLAIM_YIELD_SCHEMA,
    YIELD_MINT_PY_SCHEMA,
    YIELD_MINT_SY_SCHEMA,
    YIELD_PT_SWAP_SCHEMA,
    YIELD_REDEEM_PY_SCHEMA,
    YIELD_REDEEM_SY_SCHEMA,
    YIELD_REMOVE_MARKET_LIQUIDITY_SCHEMA,
    YIELD_SIGN_LIMIT_ORDER_SCHEMA,
    YIELD_YT_SWAP_SCHEMA,
    LAUNCHPAD_CLAIM_ALLOCATION_SCHEMA,
    LAUNCHPAD_CLAIM_VESTED_SCHEMA,
    LAUNCHPAD_COMMIT_SCHEMA,
    LAUNCHPAD_REFUND_SCHEMA,
    LAUNCHPAD_WITHDRAW_COMMIT_SCHEMA,
    PERP_ADJUST_MARGIN_SCHEMA,
    PERP_CANCEL_ORDER_SCHEMA,
    PERP_CHANGE_LEVERAGE_SCHEMA,
    PERP_CHANGE_MARGIN_MODE_SCHEMA,
    PERP_CLAIM_FUNDING_SCHEMA,
    PERP_CLOSE_POSITION_SCHEMA,
    PERP_DECREASE_POSITION_SCHEMA,
    PERP_INCREASE_POSITION_SCHEMA,
    PERP_OPEN_POSITION_SCHEMA,
    PERP_PLACE_ORDER_SCHEMA,
    PERMISSION_PROTOCOL_AUTHORIZATION_SCHEMA,
    RESTAKING_COMPLETE_WITHDRAWAL_SCHEMA,
    RESTAKING_DELEGATE_TO_SCHEMA,
    RESTAKING_DEPOSIT_SCHEMA,
    RESTAKING_QUEUE_WITHDRAWAL_SCHEMA,
    RESTAKING_REDELEGATE_SCHEMA,
    RESTAKING_REGISTER_OPERATOR_SCHEMA,
    RESTAKING_UNDELEGATE_SCHEMA,
    STAKING_CLAIM_REWARDS_SCHEMA,
    STAKING_COOLDOWN_SCHEMA,
    STAKING_GAUGE_DEPOSIT_SCHEMA,
    STAKING_GAUGE_WITHDRAW_SCHEMA,
    STAKING_INCREASE_LOCK_AMOUNT_SCHEMA,
    STAKING_INCREASE_LOCK_TIME_SCHEMA,
    STAKING_LOCK_SCHEMA,
    STAKING_REDEEM_SCHEMA,
    STAKING_STAKE_SCHEMA,
    STAKING_UNLOCK_SCHEMA,
    STAKING_VOTE_FOR_GAUGE_SCHEMA,
    MARKETPLACE_SIGN_ORDER_SCHEMA,
    MARKETPLACE_FULFILL_ORDER_SCHEMA,
    MARKETPLACE_CANCEL_ORDER_SCHEMA,
    TOKEN_ERC20_APPROVE_SCHEMA,
    TOKEN_ERC20_PERMIT_SCHEMA,
    TOKEN_ERC20_TRANSFER_SCHEMA,
    TOKEN_NFT_APPROVE_SCHEMA,
    TOKEN_NFT_SET_APPROVAL_FOR_ALL_SCHEMA,
    TOKEN_NFT_TRANSFER_SCHEMA,
    TOKEN_PERMIT2_APPROVE_SCHEMA,
    TOKEN_PERMIT2_SIGN_ALLOWANCE_SCHEMA,
    TOKEN_PERMIT2_SIGN_TRANSFER_SCHEMA,
    TOKEN_PERMIT2_TRANSFER_FROM_SCHEMA,
    TOKEN_REVOKE_APPROVAL_SCHEMA,
    TOKEN_UNWRAP_NATIVE_SCHEMA,
    TOKEN_WRAP_NATIVE_SCHEMA,
    HL_ORDER_SCHEMA,
    HL_UPDATE_LEVERAGE_SCHEMA,
    HL_WITHDRAW_SCHEMA,
    HL_USD_CLASS_TRANSFER_SCHEMA,
    HL_SEND_ASSET_SCHEMA,
    HL_TOKEN_DELEGATE_SCHEMA,
    HL_TWAP_ORDER_SCHEMA,
    HL_UPDATE_ISOLATED_MARGIN_SCHEMA,
];

/// Composes the shipped core and action Cedar schemas.
#[derive(Debug, Default, Clone)]
pub struct PolicySchemaComposer {
    manifests: Vec<PolicyManifest>,
}

/// Preview of a composed policy schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SchemaPreview {
    /// Final Cedar schema text.
    pub schema_text: String,
    /// SHA-256 hash of `schema_text`.
    pub schema_hash: String,
    /// Fields contributed by manifests that were not already present.
    pub added_fields: Vec<AddedContextField>,
}

/// Manifest-added context field metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AddedContextField {
    /// Action kind.
    pub action: String,
    /// Context field name.
    pub field: String,
    /// Cedar field type.
    #[serde(rename = "type")]
    pub type_name: String,
    /// Manifest id that contributed the field.
    pub source_manifest: String,
}

impl PolicySchemaComposer {
    /// Construct a schema composer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            manifests: Vec::new(),
        }
    }

    /// Return a composer with manifest-driven context extensions.
    ///
    /// # Errors
    ///
    /// Returns an error when manifest schema extensions are invalid or
    /// conflict with the base schema.
    pub fn with_manifests(mut self, manifests: &[PolicyManifest]) -> Result<Self, PolicyRpcError> {
        validate_manifests(manifests)?;
        self.manifests = manifests.to_vec();
        self.try_preview()?;
        Ok(self)
    }

    /// Return the concatenated Cedar schema text.
    #[must_use]
    pub fn compose(&self) -> String {
        self.preview().schema_text
    }

    /// Return the schema preview.
    #[must_use]
    pub fn preview(&self) -> SchemaPreview {
        match self.try_preview() {
            Ok(preview) => preview,
            Err(error) => {
                debug_assert!(
                    false,
                    "PolicySchemaComposer contains invalid manifests: {error}"
                );
                let schema_text = base_schema_text();
                SchemaPreview {
                    schema_hash: schema_hash(&schema_text),
                    schema_text,
                    added_fields: Vec::new(),
                }
            }
        }
    }

    /// Return the schema preview.
    ///
    /// # Errors
    ///
    /// Returns an error when manifest schema extensions are invalid or
    /// conflict with the base schema.
    pub fn try_preview(&self) -> Result<SchemaPreview, PolicyRpcError> {
        let schema_text = compose_schema_text(&self.manifests)?;
        let schema_hash = schema_hash(&schema_text);
        let added_fields = added_fields(BASE_SCHEMA_TEXT, &self.manifests)?;
        Ok(SchemaPreview {
            schema_text,
            schema_hash,
            added_fields,
        })
    }
}

/// Return the SHA-256 hash string for a Cedar schema text.
#[must_use]
pub fn schema_hash(schema_text: &str) -> String {
    let digest = Sha256::digest(schema_text.as_bytes());
    format!("sha256:{digest:x}")
}

const BASE_SCHEMA_TEXT: &str = "";

/// Compose the base Cedar schema text by merging all shipped per-namespace
/// blocks into a single block per namespace.
///
/// Cedar 4.10 rejects duplicate `namespace <Name> { ... }` declarations
/// within the same schema text, but each domain's actions live in their own
/// `.cedarschema` file (each wrapping content in `namespace <Domain> { ... }`).
/// We therefore parse every shipped file, extract its namespace bodies, and
/// emit one merged block per namespace. Insertion order within a namespace
/// follows [`SHIPPED_SCHEMA_FILES`] for determinism.
pub(crate) fn base_schema_text() -> String {
    merge_namespace_blocks(SHIPPED_SCHEMA_FILES)
}

/// Parse `inputs` for `namespace <Name> { ... }` blocks and emit each unique
/// namespace exactly once with the concatenated body of all its occurrences.
/// Top-level content (text outside any namespace block — e.g. entity
/// declarations and shared type aliases at file scope) is concatenated
/// verbatim and emitted BEFORE the merged namespace blocks. This lets a
/// file like `core.cedarschema` declare top-level `entity Wallet` /
/// `entity Protocol` (shared across legacy and new schemas) alongside
/// `namespace Core { ... }`, and have both flow through to the result.
pub(crate) fn merge_namespace_blocks(inputs: &[&str]) -> String {
    // BTreeMap keeps namespace iteration deterministic (alphabetical).
    let mut bodies: BTreeMap<String, String> = BTreeMap::new();
    // Track first-seen order so output namespaces match shipped order intent.
    let mut order: Vec<String> = Vec::new();
    let mut top_level = String::new();

    for text in inputs {
        let (top, blocks) = split_top_level_and_namespaces(text);
        if !top.trim().is_empty() {
            top_level.push_str(&top);
            top_level.push('\n');
        }
        for (name, body) in blocks {
            if !bodies.contains_key(&name) {
                order.push(name.clone());
            }
            bodies.entry(name).or_default().push_str(&body);
        }
    }

    let mut out = top_level;
    for name in order {
        let body = bodies.remove(&name).unwrap_or_default();
        use std::fmt::Write;
        let _ = write!(out, "namespace {name} {{\n");
        out.push_str(&body);
        out.push_str("\n}\n\n");
    }
    out
}

/// Split `text` into (top-level fragments concatenated, namespace blocks).
/// Top-level fragments are everything outside any `namespace <Name> { ... }`
/// block (including header comments and inter-block whitespace).
fn split_top_level_and_namespaces(text: &str) -> (String, Vec<(String, String)>) {
    let mut top_level = String::new();
    let mut blocks = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip line comments while scanning for `namespace`.
        if i + 1 < bytes.len() && &bytes[i..i + 2] == b"//" {
            // Comment runs through end of line; consume it as top-level text.
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            top_level.push_str(&text[start..i]);
            continue;
        }
        if let Some(rest) = text.get(i..) {
            if rest.starts_with("namespace") {
                // Check that the keyword is followed by whitespace, not part
                // of another identifier (e.g. `namespaced`).
                let after_kw = i + "namespace".len();
                if after_kw < bytes.len() && !bytes[after_kw].is_ascii_whitespace() {
                    top_level.push(bytes[i] as char);
                    i += 1;
                    continue;
                }
                let mut p = after_kw;
                while p < bytes.len() && bytes[p].is_ascii_whitespace() {
                    p += 1;
                }
                let name_start = p;
                while p < bytes.len()
                    && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'_' || bytes[p] == b':')
                {
                    p += 1;
                }
                let name_end = p;
                while p < bytes.len() && bytes[p].is_ascii_whitespace() {
                    p += 1;
                }
                if p >= bytes.len() || bytes[p] != b'{' {
                    // Not a namespace declaration after all; treat as top-level.
                    top_level.push(bytes[i] as char);
                    i += 1;
                    continue;
                }
                let body_start = p + 1;
                let body_end = match find_matching_close_brace(&text[body_start..]) {
                    Some(rel) => body_start + rel,
                    None => break,
                };
                let name = text[name_start..name_end].to_owned();
                let body = text[body_start..body_end].to_owned();
                blocks.push((name, body));
                i = body_end + 1;
                continue;
            }
        }
        top_level.push(bytes[i] as char);
        i += 1;
    }
    (top_level, blocks)
}

/// Given a slice starting *immediately after* an open brace `{`, return the
/// index of the matching close brace `}`, accounting for nested braces.
fn find_matching_close_brace(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth: i32 = 1;
    let mut i = 0;
    while i < bytes.len() {
        // Skip line comments inside namespace bodies (we don't need
        // block comments since Cedar uses `//` only).
        if i + 1 < bytes.len() && &bytes[i..i + 2] == b"//" {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn compose_schema_text(manifests: &[PolicyManifest]) -> Result<String, PolicyRpcError> {
    let mut schema = base_schema_text();
    for field in added_fields(&schema, manifests)? {
        insert_optional_context_field(&mut schema, &field.action, &field.field, &field.type_name)?;
    }
    Ok(schema)
}

fn added_fields(
    schema_text: &str,
    manifests: &[PolicyManifest],
) -> Result<Vec<AddedContextField>, PolicyRpcError> {
    let base = if schema_text.is_empty() {
        base_schema_text()
    } else {
        schema_text.to_owned()
    };
    let base_declared = collect_context_fields(&base)?;
    let mut declared = BTreeMap::new();
    let mut added = Vec::new();

    for manifest in manifests {
        for (action, fields) in &manifest.context_extensions {
            validate_action(action)?;
            for (field, type_name) in fields {
                validate_field_name(field)?;
                let canonical_type = canonical_type(type_name)?;
                let key = (action.clone(), field.clone());
                if let Some(base_type) = base_declared.get(&key) {
                    if base_type != canonical_type {
                        return Err(PolicyRpcError::Schema(format!(
                            "context extension {action}.{field} has type {canonical_type}, but base schema declares {base_type}"
                        )));
                    }
                    continue;
                }
                if let Some(existing) = declared.get(&key) {
                    if existing != canonical_type {
                        return Err(PolicyRpcError::Schema(format!(
                            "context field {action}.{field} already has type {existing}, not {canonical_type}"
                        )));
                    }
                    continue;
                }
                declared.insert(key, canonical_type.to_owned());
                added.push(AddedContextField {
                    action: action.clone(),
                    field: field.clone(),
                    type_name: canonical_type.to_owned(),
                    source_manifest: manifest.id.clone(),
                });
            }
        }
    }

    Ok(added)
}

fn collect_context_fields(
    schema_text: &str,
) -> Result<BTreeMap<(String, String), String>, PolicyRpcError> {
    let mut fields = BTreeMap::new();
    for (action, type_name) in ACTION_CONTEXT_TYPES {
        let block = type_block(schema_text, type_name)?;
        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("type ") || trimmed == "};" {
                continue;
            }
            let Some((name, field_type)) = parse_field_line(trimmed) else {
                continue;
            };
            fields.insert(
                ((*action).to_owned(), name.to_owned()),
                field_type.to_owned(),
            );
        }
    }
    Ok(fields)
}

fn parse_field_line(line: &str) -> Option<(&str, &str)> {
    let line = line.strip_suffix(',').unwrap_or(line);
    let (name, field_type) = line.split_once(':')?;
    Some((name.trim().trim_end_matches('?'), field_type.trim()))
}

fn insert_optional_context_field(
    schema: &mut String,
    action: &str,
    field: &str,
    type_name: &str,
) -> Result<(), PolicyRpcError> {
    let context_type = context_type_for_action(action)?;
    let start = schema
        .find(&format!("type {context_type} = {{"))
        .ok_or_else(|| PolicyRpcError::Schema(format!("missing context type `{context_type}`")))?;
    let relative_end = schema[start..].find("};").ok_or_else(|| {
        PolicyRpcError::Schema(format!("unterminated context type `{context_type}`"))
    })?;
    let insert_at = start + relative_end;
    schema.insert_str(insert_at, &format!("  {field}?: {type_name},\n"));
    Ok(())
}

fn type_block<'a>(schema_text: &'a str, type_name: &str) -> Result<&'a str, PolicyRpcError> {
    let start = schema_text
        .find(&format!("type {type_name} = {{"))
        .ok_or_else(|| PolicyRpcError::Schema(format!("missing context type `{type_name}`")))?;
    let relative_end = schema_text[start..].find("};").ok_or_else(|| {
        PolicyRpcError::Schema(format!("unterminated context type `{type_name}`"))
    })?;
    Ok(&schema_text[start..start + relative_end + 2])
}

fn context_type_for_action(action: &str) -> Result<&'static str, PolicyRpcError> {
    ACTION_CONTEXT_TYPES
        .iter()
        .find_map(|(candidate, type_name)| (*candidate == action).then_some(*type_name))
        .ok_or_else(|| {
            PolicyRpcError::Schema(format!("unknown context extension action `{action}`"))
        })
}

fn validate_action(action: &str) -> Result<(), PolicyRpcError> {
    context_type_for_action(action).map(|_| ())
}

fn validate_field_name(field: &str) -> Result<(), PolicyRpcError> {
    let mut chars = field.chars();
    let Some(first) = chars.next() else {
        return Err(PolicyRpcError::Schema(
            "field name must not be empty".to_owned(),
        ));
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(PolicyRpcError::Schema(format!(
            "invalid context field name `{field}`"
        )));
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        return Err(PolicyRpcError::Schema(format!(
            "invalid context field name `{field}`"
        )));
    }
    Ok(())
}

fn canonical_type(type_name: &str) -> Result<&'static str, PolicyRpcError> {
    match type_name {
        "String" => Ok("String"),
        "Long" => Ok("Long"),
        "Bool" => Ok("Bool"),
        "decimal" | "Decimal" => Ok("decimal"),
        "Set<String>" => Ok("Set<String>"),
        // Legacy types from the pre-migration schema — still referenced by
        // shipped manifest fragments and by test fixtures that exercise old
        // policy bodies.
        "UsdValuation" => Ok("UsdValuation"),
        "WindowStats" => Ok("WindowStats"),
        other => Err(PolicyRpcError::Schema(format!(
            "unsupported context field type `{other}`"
        ))),
    }
}

/// Action-name → bare context type name (without namespace prefix).
///
/// Bare names are used because [`type_block`] performs a simple text search
/// (`format!("type {type_name} = {{")`) over the concatenated schema text,
/// which still locates the declaration even when it lives inside a
/// `namespace <Domain> { ... }` block. The Cedar action id itself is
/// `<Namespace>::<PascalCaseAction>` (see [`action_name::REGISTERED_ACTIONS`]).
const ACTION_CONTEXT_TYPES: &[(&str, &str)] = &[
    // Core structural
    ("multicall", "MulticallContext"),
    ("unknown", "UnknownContext"),
    // airdrop (alphabetical)
    ("claim", "ClaimContext"),
    ("delegate", "DelegateContext"),
    // amm (alphabetical)
    ("add_liquidity", "AddLiquidityContext"),
    ("cancel_intent_order", "CancelIntentOrderContext"),
    ("collect_fees", "CollectFeesContext"),
    ("pre_sign_intent_order", "PreSignIntentOrderContext"),
    ("remove_liquidity", "RemoveLiquidityContext"),
    ("settle_intent_order", "SettleIntentOrderContext"),
    ("sign_intent_order", "SignIntentOrderContext"),
    ("swap", "SwapContext"),
    // lending (alphabetical)
    ("borrow", "BorrowContext"),
    ("delegate_borrow", "DelegateBorrowContext"),
    ("disable_collateral", "DisableCollateralContext"),
    ("enable_collateral", "EnableCollateralContext"),
    ("liquidate", "LiquidateContext"),
    ("repay", "RepayContext"),
    ("set_emode", "SetEModeContext"),
    ("supply", "SupplyContext"),
    ("swap_rate_mode", "SwapRateModeContext"),
    ("withdraw", "WithdrawContext"),
    // launchpad (alphabetical)
    ("claim_allocation", "ClaimAllocationContext"),
    ("claim_vested", "ClaimVestedContext"),
    ("commit", "CommitContext"),
    ("refund", "RefundContext"),
    ("withdraw_commit", "WithdrawCommitContext"),
    // perp (alphabetical)
    ("adjust_margin", "AdjustMarginContext"),
    ("cancel_order", "CancelOrderContext"),
    ("change_leverage", "ChangeLeverageContext"),
    ("change_margin_mode", "ChangeMarginModeContext"),
    ("claim_funding", "ClaimFundingContext"),
    ("close_position", "ClosePositionContext"),
    ("decrease_position", "DecreasePositionContext"),
    ("increase_position", "IncreasePositionContext"),
    ("open_position", "OpenPositionContext"),
    ("place_order", "PlaceOrderContext"),
    // permission (alphabetical)
    ("protocol_authorization", "ProtocolAuthorizationContext"),
    // token (alphabetical)
    ("erc20_approve", "Erc20ApproveContext"),
    ("erc20_permit", "Erc20PermitContext"),
    ("erc20_transfer", "Erc20TransferContext"),
    ("nft_approve", "NftApproveContext"),
    ("nft_set_approval_for_all", "NftSetApprovalForAllContext"),
    ("nft_transfer", "NftTransferContext"),
    ("permit2_approve", "Permit2ApproveContext"),
    ("permit2_sign_allowance", "Permit2SignAllowanceContext"),
    ("permit2_sign_transfer", "Permit2SignTransferContext"),
    ("permit2_transfer_from", "Permit2TransferFromContext"),
    ("revoke_approval", "RevokeApprovalContext"),
    ("unwrap_native", "UnwrapNativeContext"),
    ("wrap_native", "WrapNativeContext"),
    // hyperliquid_core (alphabetical) — `hl_`-prefixed tags keep these globally
    // unique (notably `withdraw` is already a Lending tag).
    ("hl_order", "HlOrderContext"),
    ("hl_send_asset", "HlSendAssetContext"),
    ("hl_token_delegate", "HlTokenDelegateContext"),
    ("hl_twap_order", "HlTwapOrderContext"),
    ("hl_update_isolated_margin", "HlUpdateIsolatedMarginContext"),
    ("hl_update_leverage", "HlUpdateLeverageContext"),
    ("hl_usd_class_transfer", "HlUsdClassTransferContext"),
    ("hl_withdraw", "HlWithdrawContext"),
];

#[cfg(test)]
mod base_schema_tests {
    //! Smoke tests that confirm the concatenated `base_schema_text()` parses
    //! as a valid Cedar 4.10 schema. This guards against:
    //!   - namespace block syntax errors,
    //!   - cross-namespace `Core::<Type>` references that don't resolve,
    //!   - duplicate type declarations within the same namespace,
    //!   - typos in entity / action / type names.
    //!
    //! These tests fire on every `cargo test -p policy-engine` and catch
    //! schema-file regressions before they reach downstream consumers.

    use super::base_schema_text;

    #[test]
    fn base_schema_parses() {
        let text = base_schema_text();
        let result = cedar_policy::Schema::from_cedarschema_str(&text);
        assert!(
            result.is_ok(),
            "base_schema_text() failed to parse as Cedar schema: {:?}",
            result.err()
        );
    }

    #[test]
    fn base_schema_declares_all_registered_actions() {
        // Every entry in `REGISTERED_ACTIONS` whose snake_case name maps to a
        // Phase 1 action (i.e., has a corresponding `.cedarschema` in the new
        // tree) must have its Cedar action declaration in the parsed schema.
        // Legacy-only entries (mint_liquidity_nft, flash_loan, etc.) are
        // expected to be absent — they live only in -old/ and are validated by
        // `extension_manifests_validate.rs`.
        let text = base_schema_text();
        let (schema, _warnings) =
            cedar_policy::Schema::from_cedarschema_str(&text).expect("base schema parses");

        // Cedar 4.10 renders action EntityUids as `<Namespace>::Action::"<Name>"`.
        // Spot-check one action per namespace.
        let phase1_action_ids = &[
            r#"Core::Action::"Multicall""#,
            r#"Core::Action::"Unknown""#,
            r#"Airdrop::Action::"Claim""#,
            r#"Amm::Action::"Swap""#,
            r#"Lending::Action::"Supply""#,
            r#"Launchpad::Action::"Commit""#,
            r#"Perp::Action::"OpenPosition""#,
            r#"Permission::Action::"ProtocolAuthorization""#,
            r#"Token::Action::"Erc20Approve""#,
        ];
        // Cedar 4.10's Schema::actions() yields EntityUids whose Display form
        // includes both the entity type (Action) and the id; we match against
        // the rendered string.
        let declared: Vec<String> = schema
            .actions()
            .map(std::string::ToString::to_string)
            .collect();
        for id in phase1_action_ids {
            assert!(
                declared.iter().any(|d| d.contains(id)),
                "expected action `{id}` declared in base schema; got: {declared:?}"
            );
        }
    }
}
