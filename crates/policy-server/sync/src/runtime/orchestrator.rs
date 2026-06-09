//! Runtime orchestrator for wallet-state and action live-input refresh.
//!
//! The orchestrator walks stale `LiveField`s, batches them by external source,
//! dispatches each batch to the matching fetcher, and writes successful results
//! back into the state or action being refreshed.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use policy_state::pending::{PendingKind, PendingTx};
use policy_state::{
    Confidence, DataSource, LiveField, Position, PositionKind, Price, ProtocolRef, SignedI256,
    Time, WalletState, U256,
};
use policy_transition::action::{Action, ActionBody, PerpAction, TokenAction};

use crate::batcher::{batch_by_source, BatchKind, FetchBatch};
use crate::calc::{CalcContext, CalcRegistry};
use crate::error::SyncError;
use crate::fetchers::onchain::OnchainCall;
use crate::fetchers::oracle::{provider_key, PriceFetcher, RestJsonOracleFetcher};
use crate::fetchers::{
    ChainlinkFetcher, CowSwapFetcher, HyperliquidFetcher, IntentFetcher, OnchainViewFetcher,
    OneInchFusionFetcher, OneInchFusionPlusFetcher, OneInchLopFetcher, RegistryFetcher,
    UniswapFetcher, UniswapXFetcher,
};
use crate::walker::{walk_stale, ActionSlot, FieldLocation, WalkStats};

#[derive(Debug, Default, Clone)]
pub struct RefreshReport {
    pub walked: WalkStats,
    pub batches_processed: usize,
    pub fields_updated: usize,
    pub fields_failed: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct HyperliquidAccountReport {
    pub account_updated: bool,
    pub errors: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct IntentOrdersReport {
    pub orders_updated: usize,
    /// Orders snapshot-pruned because they left an active-orderbook venue's
    /// listing (see `IntentFetcher::authoritative_prefix`).
    pub orders_pruned: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct PermitReconcileReport {
    /// Signed permit/permit2 pending entries retired this tick (expired or
    /// consumed) and pruned from `state.pending`.
    pub permits_retired: usize,
    /// Non-fatal errors (e.g. a Permit2 nonce-bitmap RPC read failed). The
    /// affected entry is left `Active` and retried next tick.
    pub errors: Vec<String>,
}

pub struct Orchestrator {
    onchain: OnchainViewFetcher,
    // Normalized oracle provider key to fetcher.
    price_fetchers: HashMap<String, Arc<dyn PriceFetcher>>,
    registry: Option<RegistryFetcher>,
    hyperliquid: Option<HyperliquidFetcher>,
    uniswap: Option<UniswapFetcher>,
    uniswap_x: Option<UniswapXFetcher>,
    cow_swap: Option<CowSwapFetcher>,
    one_inch_fusion: Option<OneInchFusionFetcher>,
    one_inch_fusion_plus: Option<OneInchFusionPlusFetcher>,
    one_inch_lop: Option<OneInchLopFetcher>,
    calc: CalcRegistry,
    // Global values used by derived live-field inputs.
    globals: crate::resolver::GlobalValues,
    // Direct router access for primitive sync and receipt watchers.
    router: Option<Arc<crate::RpcRouter>>,
}

impl Orchestrator {
    #[must_use]
    pub fn new(onchain: OnchainViewFetcher) -> Self {
        Self {
            onchain,
            price_fetchers: HashMap::new(),
            registry: None,
            hyperliquid: None,
            uniswap: Some(UniswapFetcher::new()),
            uniswap_x: None,
            cow_swap: None,
            one_inch_fusion: None,
            one_inch_fusion_plus: None,
            one_inch_lop: None,
            calc: CalcRegistry::with_builtins(),
            globals: crate::resolver::GlobalValues::new(),
            router: None,
        }
    }

    pub fn set_global(&mut self, name: impl Into<String>, value: serde_json::Value) {
        self.globals.insert(name.into(), value);
    }

    pub(crate) fn router_ref(&self) -> Option<Arc<crate::RpcRouter>> {
        self.router.clone()
    }

    #[must_use]
    pub fn router_arc(&self) -> Option<Arc<crate::RpcRouter>> {
        self.router.clone()
    }

    pub fn with_price_fetcher(
        mut self,
        name: impl Into<String>,
        fetcher: Arc<dyn PriceFetcher>,
    ) -> Self {
        self.price_fetchers.insert(name.into(), fetcher);
        self
    }

    #[must_use]
    pub fn with_chainlink(self, chainlink: ChainlinkFetcher) -> Self {
        self.with_price_fetcher("chainlink", Arc::new(chainlink))
    }

    #[must_use]
    pub fn with_registry(mut self, registry: RegistryFetcher) -> Self {
        self.registry = Some(registry);
        self
    }

    #[must_use]
    pub fn with_hyperliquid(mut self, hl: HyperliquidFetcher) -> Self {
        self.hyperliquid = Some(hl);
        self
    }

    #[must_use]
    pub fn with_uniswap(mut self, uniswap: UniswapFetcher) -> Self {
        self.uniswap = Some(uniswap);
        self
    }

    #[must_use]
    pub fn with_uniswap_x(mut self, uni: UniswapXFetcher) -> Self {
        self.uniswap_x = Some(uni);
        self
    }

    #[must_use]
    pub fn with_cowswap(mut self, cow: CowSwapFetcher) -> Self {
        self.cow_swap = Some(cow);
        self
    }

    #[must_use]
    pub fn with_one_inch_fusion(mut self, fusion: OneInchFusionFetcher) -> Self {
        self.one_inch_fusion = Some(fusion);
        self
    }

    #[must_use]
    pub fn with_one_inch_fusion_plus(mut self, fusion_plus: OneInchFusionPlusFetcher) -> Self {
        self.one_inch_fusion_plus = Some(fusion_plus);
        self
    }

    #[must_use]
    pub fn with_one_inch_lop(mut self, lop: OneInchLopFetcher) -> Self {
        self.one_inch_lop = Some(lop);
        self
    }

    /// Collect every configured intent-order fetcher as `&dyn IntentFetcher`.
    /// An unconfigured venue (`None` Option) is simply absent from the list, so
    /// `sync_intent_orders` polls exactly the venues that are wired.
    fn intent_fetchers(&self) -> Vec<&dyn IntentFetcher> {
        let mut fetchers: Vec<&dyn IntentFetcher> = Vec::new();
        if let Some(f) = self.uniswap_x.as_ref() {
            fetchers.push(f);
        }
        if let Some(f) = self.cow_swap.as_ref() {
            fetchers.push(f);
        }
        if let Some(f) = self.one_inch_fusion.as_ref() {
            fetchers.push(f);
        }
        if let Some(f) = self.one_inch_fusion_plus.as_ref() {
            fetchers.push(f);
        }
        if let Some(f) = self.one_inch_lop.as_ref() {
            fetchers.push(f);
        }
        fetchers
    }

    #[must_use]
    pub fn with_calc(mut self, calc: CalcRegistry) -> Self {
        self.calc = calc;
        self
    }

    #[must_use]
    pub fn from_rpc_router(router: Arc<crate::RpcRouter>) -> Self {
        let onchain = OnchainViewFetcher::new(router.clone());
        let chainlink = ChainlinkFetcher::new(router.clone());
        let mut price_fetchers: HashMap<String, Arc<dyn PriceFetcher>> = HashMap::new();
        price_fetchers.insert("chainlink".into(), Arc::new(chainlink));
        Self {
            onchain,
            price_fetchers,
            registry: Some(RegistryFetcher::new()),
            hyperliquid: Some(HyperliquidFetcher::new()),
            uniswap: Some(UniswapFetcher::new()),
            uniswap_x: None,
            cow_swap: None,
            one_inch_fusion: None,
            one_inch_fusion_plus: None,
            one_inch_lop: None,
            calc: CalcRegistry::with_builtins(),
            globals: crate::resolver::GlobalValues::new(),
            router: Some(router),
        }
    }

    /// - `RpcRouter` ← `cfg.rpc`
    pub fn from_sync_config(cfg: &crate::SyncConfig) -> Result<Self, SyncError> {
        let router = Arc::new(crate::RpcRouter::from_config(cfg.rpc.clone())?);
        let onchain = OnchainViewFetcher::new(router.clone());

        let mut price_fetchers: HashMap<String, Arc<dyn PriceFetcher>> = HashMap::new();

        // Chainlink (on-chain).
        let chainlink = ChainlinkFetcher::from_sync_config(router.clone(), &cfg.oracles.chainlink);
        price_fetchers.insert("chainlink".into(), Arc::new(chainlink));

        for (name, rest_cfg) in &cfg.oracles.rest {
            let f = RestJsonOracleFetcher::from_sync_config(name.clone(), rest_cfg);
            price_fetchers.insert(name.clone(), Arc::new(f));
        }

        let hyperliquid = cfg
            .venues
            .hyperliquid
            .as_ref()
            .map(HyperliquidFetcher::from_sync_config);
        let uniswap_x = cfg
            .venues
            .uniswap
            .as_ref()
            .map(UniswapXFetcher::from_sync_config);
        let cow_swap = cfg
            .venues
            .cowswap
            .as_ref()
            .map(CowSwapFetcher::from_sync_config);
        let one_inch_fusion = cfg
            .venues
            .one_inch_fusion
            .as_ref()
            .map(OneInchFusionFetcher::from_sync_config);
        let one_inch_fusion_plus = cfg
            .venues
            .one_inch_fusion_plus
            .as_ref()
            .map(OneInchFusionPlusFetcher::from_sync_config);
        let one_inch_lop = cfg
            .venues
            .one_inch_lop
            .as_ref()
            .map(OneInchLopFetcher::from_sync_config);
        Ok(Self {
            onchain,
            price_fetchers,
            registry: Some(RegistryFetcher::new()),
            hyperliquid,
            uniswap: Some(UniswapFetcher::new()),
            uniswap_x,
            cow_swap,
            one_inch_fusion,
            one_inch_fusion_plus,
            one_inch_lop,
            calc: CalcRegistry::with_builtins(),
            globals: crate::resolver::GlobalValues::new(),
            router: Some(router),
        })
    }

    fn price_fetcher_for(
        &self,
        source: &policy_state::DataSource,
    ) -> Option<&Arc<dyn PriceFetcher>> {
        match source {
            policy_state::DataSource::OracleFeed { provider, .. } => {
                self.price_fetchers.get(&provider_key(provider))
            }
            _ => None,
        }
    }

    pub async fn refresh(
        &self,
        state: &mut WalletState,
        now: Time,
    ) -> Result<RefreshReport, SyncError> {
        let (stale, walked) = walk_stale(state, now);
        let mut report = RefreshReport {
            walked,
            ..Default::default()
        };
        if stale.is_empty() {
            return Ok(report);
        }

        let batches = batch_by_source(stale);
        for batch in batches {
            report.batches_processed += 1;
            match self.process_batch(batch, state, now).await {
                Ok((ok, fail)) => {
                    report.fields_updated += ok;
                    report.fields_failed += fail;
                }
                Err(e) => {
                    report.errors.push(format!("{e}"));
                }
            }
        }
        Ok(report)
    }

    pub async fn sync_hyperliquid_account(
        &self,
        state: &mut WalletState,
        now: Time,
    ) -> Result<HyperliquidAccountReport, SyncError> {
        let Some(hl) = self.hyperliquid.as_ref() else {
            return Ok(HyperliquidAccountReport {
                account_updated: false,
                errors: vec!["hyperliquid fetcher is not configured".into()],
            });
        };

        let user = state.wallet_id.address;
        let account = hl.fetch_account_snapshot("", &user).await?;
        let source = DataSource::VenueApi {
            endpoint: hl.info_endpoint(),
            parser_id: "hl_account".into(),
            auth: None,
        };
        upsert_hyperliquid_account(state, account, source, now)?;
        Ok(HyperliquidAccountReport {
            account_updated: true,
            errors: Vec::new(),
        })
    }

    /// Discover and reconcile off-chain intent-order status for this wallet
    /// across every configured venue (`UniswapX` / `CowSwap` / `1inch Fusion`).
    /// Each fetcher is the source of truth for its venue: it is polled and the
    /// returned orders are upserted into `state.pending` (keyed by the venue
    /// order id embedded in `PendingTx.id`). A single fetcher error is recorded
    /// in `report.errors` and the tick continues — one dead venue never aborts
    /// the others.
    pub async fn sync_intent_orders(
        &self,
        state: &mut WalletState,
        now: Time,
    ) -> Result<IntentOrdersReport, SyncError> {
        let swapper = state.wallet_id.address;
        let fetchers = self.intent_fetchers();
        Ok(run_intent_sync(&fetchers, state, &swapper, now).await)
    }

    /// Reconcile signed off-chain permit / permit2 entries in `state.pending`
    /// and retire (prune) the ones that have settled.
    ///
    /// This is the lifecycle-closer for the permits the extension reports via
    /// `POST /wallets/:address/permits`. It runs as a **separate** per-tick step
    /// from `sync_intent_orders` (it must not disturb the intent-fetcher flow)
    /// and touches ONLY the three `Signed*` `PendingKind` variants — intent
    /// orders share `state.pending`, so a loose filter would prune live intents.
    ///
    /// Retire rules:
    /// * **Expiry** (all three kinds, no RPC): `valid_until < now` → mark
    ///   `Expired` and prune. Always runs.
    /// * **Consumed — `SignedPermit2Transfer`**: read the Permit2 unordered nonce
    ///   bitmap on-chain (`nonceBitmap(owner, word)`); if the signed bit is set
    ///   the `SignatureTransfer` was executed → prune.
    /// * **Consumed — `SignedEIP2612`**: read the token's `nonces(owner)`; once it
    ///   has advanced past the signed (sequential, strictly in-order) nonce the
    ///   permit was used or invalidated → prune.
    ///
    /// All consumed-checks are best-effort: with no `RpcRouter` (or on RPC error)
    /// the entry is left `Active` and retried next tick — never aborts the tick.
    ///
    /// FOLLOW-UP (deliberately NOT built here): consumed-detection for the Permit2
    /// `AllowanceTransfer` (`SignedPermit2`). It uses a sequential uint48 nonce in
    /// the on-chain `allowance(owner, token, spender)` struct, but Phase 3 stores
    /// its nonce as `(word, bit)` bitmap coords (the wrong shape — and inconsistent
    /// across the extension report / ingest / reducer). A sound reader needs that
    /// nonce model corrected end-to-end first (expiration-matching is NOT sound:
    /// Permit2's default 30-day expiration + max amount make two permits to the
    /// same `(token, spender)` indistinguishable). Until then `SignedPermit2` is
    /// expiry-only.
    pub async fn reconcile_permits(
        &self,
        state: &mut WalletState,
        now: Time,
    ) -> Result<PermitReconcileReport, SyncError> {
        let mut report = PermitReconcileReport::default();
        let owner = state.wallet_id.address;

        // Collect retire decisions first (immutable walk), then mutate. Pure
        // expiry needs no I/O; the consumed-check fetches the bitmap inline but
        // the decision logic is factored into pure helpers for unit testing.
        let mut to_prune: Vec<String> = Vec::new();

        for pending in &state.pending {
            // Only signed permit/permit2 entries — intents are reconciled by
            // `sync_intent_orders`, perp/limit pendings by their own paths.
            if !is_signed_permit_kind(&pending.kind) {
                continue;
            }

            // 1) Expiry — uniform across all three kinds, no RPC.
            if permit_is_expired(pending.lifecycle.valid_until, now) {
                to_prune.push(pending.id.clone());
                report.permits_retired += 1;
                continue;
            }

            // 2) Consumed — SignatureTransfer (unordered nonce bitmap).
            if let PendingKind::SignedPermit2Transfer { token, nonce, .. } = &pending.kind {
                let (word, bit) = *nonce;
                let chain = token.key.chain();
                match self.permit2_nonce_consumed(chain, owner, word, bit).await {
                    Ok(true) => {
                        to_prune.push(pending.id.clone());
                        report.permits_retired += 1;
                    }
                    Ok(false) => {}
                    Err(e) => report
                        .errors
                        .push(format!("permit2 nonce-bitmap read for {}: {e}", pending.id)),
                }
            }

            // 3) Consumed — EIP-2612 (sequential per-(owner,token) nonce).
            if let PendingKind::SignedEIP2612 { token, nonce, .. } = &pending.kind {
                if let policy_state::token::TokenKey::Erc20 { address, .. } = &token.key {
                    let chain = token.key.chain();
                    match self
                        .eip2612_nonce_consumed(chain, *address, owner, *nonce)
                        .await
                    {
                        Ok(true) => {
                            to_prune.push(pending.id.clone());
                            report.permits_retired += 1;
                        }
                        Ok(false) => {}
                        Err(e) => report
                            .errors
                            .push(format!("eip2612 nonces read for {}: {e}", pending.id)),
                    }
                }
            }
        }

        if !to_prune.is_empty() {
            state.pending.retain(|p| !to_prune.contains(&p.id));
        }
        Ok(report)
    }

    /// Read the Permit2 unordered-nonce bitmap word for `(owner, word)` on
    /// `chain` and report whether `bit` is set (i.e. the `SignatureTransfer` was
    /// consumed). Best-effort: returns `Ok(false)` only on a clear "not set";
    /// any inability to read (no router, RPC error, short return) is an `Err`
    /// the caller records and the entry stays `Active`.
    async fn permit2_nonce_consumed(
        &self,
        chain: &policy_state::ChainId,
        owner: policy_state::primitives::Address,
        word: U256,
        bit: u8,
    ) -> Result<bool, SyncError> {
        let router = self.router.as_ref().ok_or_else(|| SyncError::FetchFailed {
            source_id: "permit2_nonce_bitmap".into(),
            reason: "no RpcRouter configured".into(),
        })?;
        let permit2 = permit2_contract_address()?;
        let req =
            crate::fetchers::rpc::EthCallRequest::new(permit2, encode_nonce_bitmap(owner, word));
        let return_data = router.eth_call(chain, req).await?;
        let bitmap = decode_u256_be(&return_data).ok_or_else(|| SyncError::FetchFailed {
            source_id: "permit2_nonce_bitmap".into(),
            reason: format!("short nonceBitmap return ({} bytes)", return_data.len()),
        })?;
        Ok(bitmap_bit_is_set(bitmap, bit))
    }

    /// Read the EIP-2612 `nonces(owner)` for `token` on `chain`. The signed permit
    /// is consumed once the on-chain nonce has advanced past the signed nonce:
    /// EIP-2612 nonces are strictly sequential and in-order, so `nonces(owner) >
    /// signed_nonce` proves that nonce was used (or invalidated by a later
    /// in-order use). Best-effort: any inability to read (no router, RPC error,
    /// short return) is an `Err` the caller records, leaving the entry `Active`.
    async fn eip2612_nonce_consumed(
        &self,
        chain: &policy_state::ChainId,
        token: policy_state::primitives::Address,
        owner: policy_state::primitives::Address,
        signed_nonce: U256,
    ) -> Result<bool, SyncError> {
        let router = self.router.as_ref().ok_or_else(|| SyncError::FetchFailed {
            source_id: "eip2612_nonces".into(),
            reason: "no RpcRouter configured".into(),
        })?;
        let req = crate::fetchers::rpc::EthCallRequest::new(token, encode_nonces(owner));
        let return_data = router.eth_call(chain, req).await?;
        let onchain = decode_u256_be(&return_data).ok_or_else(|| SyncError::FetchFailed {
            source_id: "eip2612_nonces".into(),
            reason: format!("short nonces return ({} bytes)", return_data.len()),
        })?;
        Ok(eip2612_nonce_is_consumed(onchain, signed_nonce))
    }

    /// Best-effort **core** account sync (every tick): fetch the native-dex core,
    /// then field-scoped-merge so a partial failure preserves prior values
    /// instead of clobbering them to defaults.
    pub async fn sync_hyperliquid_core(
        &self,
        state: &mut WalletState,
        now: Time,
    ) -> Result<HyperliquidAccountReport, SyncError> {
        let Some(hl) = self.hyperliquid.as_ref() else {
            return Ok(HyperliquidAccountReport {
                account_updated: false,
                errors: vec!["hyperliquid fetcher is not configured".into()],
            });
        };
        let user = state.wallet_id.address;
        let (core, fresh, errors) = hl.fetch_hl_core("", &user, now).await;
        upsert_hyperliquid_merge(state, |a| a.merge_core(core, fresh), now);
        Ok(HyperliquidAccountReport {
            account_updated: true,
            errors,
        })
    }

    /// Best-effort **long-tail** account sync (sub-cadence): staking / vaults /
    /// borrow-lend / agents, each preserved on failure.
    pub async fn sync_hyperliquid_longtail(
        &self,
        state: &mut WalletState,
        now: Time,
    ) -> Result<HyperliquidAccountReport, SyncError> {
        let Some(hl) = self.hyperliquid.as_ref() else {
            return Ok(HyperliquidAccountReport {
                account_updated: false,
                errors: vec!["hyperliquid fetcher is not configured".into()],
            });
        };
        let user = state.wallet_id.address;
        let (lt, fresh, errors) = hl.fetch_hl_longtail("", &user).await;
        upsert_hyperliquid_merge(state, |a| a.merge_longtail(lt, fresh), now);
        Ok(HyperliquidAccountReport {
            account_updated: true,
            errors,
        })
    }

    pub async fn refresh_action(
        &self,
        action: &mut policy_transition::action::Action,
        state: &WalletState,
        now: Time,
    ) -> Result<RefreshReport, SyncError> {
        let (stale, walked) = crate::action_walk::walk_action_stale(action, now);
        let mut report = RefreshReport {
            walked,
            ..Default::default()
        };
        if stale.is_empty() {
            return Ok(report);
        }

        let batches = batch_by_source(stale);
        for batch in batches {
            report.batches_processed += 1;
            match self
                .process_batch_for_action(batch, action, state, now)
                .await
            {
                Ok((ok, fail)) => {
                    report.fields_updated += ok;
                    report.fields_failed += fail;
                }
                Err(e) => {
                    report.errors.push(format!("{e}"));
                }
            }
        }
        Ok(report)
    }

    async fn process_batch_for_action(
        &self,
        batch: FetchBatch,
        action: &mut policy_transition::action::Action,
        state: &WalletState,
        now: Time,
    ) -> Result<(usize, usize), SyncError> {
        let mut ok = 0usize;
        let mut fail = 0usize;
        match &batch.kind {
            BatchKind::Oracle => {
                for item in batch.items {
                    let Some(fetcher) = self.price_fetcher_for(&item.source) else {
                        fail += 1;
                        continue;
                    };
                    match fetcher.fetch_price(&item.source).await {
                        Ok(price) => {
                            crate::action_walk::apply_value_to_action(
                                action,
                                &item.location,
                                serde_json::Value::String(price.0),
                                now,
                            );
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
            }
            BatchKind::Onchain { chain } => {
                let calls: Result<Vec<_>, _> = batch
                    .items
                    .iter()
                    .map(|item| {
                        let args = match &item.location {
                            crate::walker::FieldLocation::Action { .. } => {
                                crate::args_resolver::resolve_args_for_location(
                                    &item.location,
                                    action,
                                    state,
                                )
                            }
                            _ => Vec::new(),
                        };
                        crate::fetchers::onchain::OnchainCall::from_source(&item.source, args)
                    })
                    .collect();
                let Ok(calls) = calls else {
                    return Ok((0, batch.items.len()));
                };
                let outcomes = self.onchain.fetch_batch(chain, &calls).await?;
                for (item, outcome) in batch.items.into_iter().zip(outcomes.into_iter()) {
                    if outcome.success {
                        if let Some(value) = outcome.value {
                            let value = if is_permit2_nonce_bitmap_source(&item.source) {
                                match permit2_nonce_bitmap_apply_value(
                                    action,
                                    &item.location,
                                    &value,
                                ) {
                                    Some(Ok(v)) => v,
                                    Some(Err(_)) => {
                                        fail += 1;
                                        continue;
                                    }
                                    None => value,
                                }
                            } else {
                                value
                            };
                            crate::action_walk::apply_value_to_action(
                                action,
                                &item.location,
                                value,
                                now,
                            );
                            ok += 1;
                        } else {
                            fail += 1;
                        }
                    } else {
                        fail += 1;
                    }
                }
            }
            BatchKind::Registry { .. } => {
                let Some(reg) = self.registry.as_ref() else {
                    return Ok((0, batch.items.len()));
                };
                for item in batch.items {
                    match reg.fetch(&item.source).await {
                        Ok(v) => {
                            crate::action_walk::apply_value_to_action(
                                action,
                                &item.location,
                                v,
                                now,
                            );
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
            }
            BatchKind::Venue { endpoint } => {
                let is_hl = is_hyperliquid_endpoint(endpoint);
                let is_uniswap = is_uniswap_endpoint(endpoint);
                for item in batch.items {
                    let FieldLocation::Action { slot, .. } = &item.location else {
                        fail += 1;
                        continue;
                    };
                    let fetched = if is_hl {
                        let Some(hl) = self.hyperliquid.as_ref() else {
                            fail += 1;
                            continue;
                        };
                        let market_symbol =
                            action_market_symbol(action, state, &item.location).unwrap_or_default();
                        hl.fetch_action_value(
                            &item.source,
                            slot,
                            &market_symbol,
                            &state.wallet_id.address,
                        )
                        .await
                    } else if is_uniswap {
                        let Some(uniswap) = self.uniswap.as_ref() else {
                            fail += 1;
                            continue;
                        };
                        let Some(body) = action_body_for_location(action, &item.location) else {
                            fail += 1;
                            continue;
                        };
                        uniswap
                            .fetch_action_value(&item.source, slot, body, &action.meta.submitter)
                            .await
                    } else {
                        fail += 1;
                        continue;
                    };
                    match fetched {
                        Ok(v) => {
                            crate::action_walk::apply_value_to_action(
                                action,
                                &item.location,
                                v,
                                now,
                            );
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
            }
            BatchKind::Derived | BatchKind::UserSupplied => {}
        }
        Ok((ok, fail))
    }

    pub(crate) async fn process_batch_public(
        &self,
        batch: FetchBatch,
        state: &mut WalletState,
        now: Time,
    ) -> Result<(usize, usize), SyncError> {
        self.process_batch(batch, state, now).await
    }

    async fn process_batch(
        &self,
        batch: FetchBatch,
        state: &mut WalletState,
        now: Time,
    ) -> Result<(usize, usize), SyncError> {
        match batch.kind {
            BatchKind::Onchain { chain } => {
                // State-level on-chain live fields only support no-arg calls.
                // Action refresh resolves call arguments through `actions::args`.
                let calls: Result<Vec<OnchainCall>, _> = batch
                    .items
                    .iter()
                    .map(|item| OnchainCall::from_source(&item.source, vec![]))
                    .collect();
                let calls = calls?;

                let outcomes = self.onchain.fetch_batch(&chain, &calls).await?;

                let mut ok = 0;
                let mut fail = 0;
                for (item, outcome) in batch.items.into_iter().zip(outcomes.into_iter()) {
                    if outcome.success {
                        if let Some(value) = outcome.value {
                            apply_value(state, &item.location, value, now);
                            ok += 1;
                        } else {
                            fail += 1;
                        }
                    } else {
                        fail += 1;
                    }
                }
                Ok((ok, fail))
            }

            BatchKind::Oracle => {
                let mut ok = 0;
                let mut fail = 0;
                for item in batch.items {
                    let Some(fetcher) = self.price_fetcher_for(&item.source) else {
                        // No fetcher registered for this provider — price can never
                        // populate. Log it: this is otherwise an invisible cause of
                        // an empty USD column.
                        tracing::warn!(source = ?item.source, "oracle price fetch skipped: no fetcher for provider");
                        fail += 1;
                        continue;
                    };
                    match fetcher.fetch_price(&item.source).await {
                        Ok(price) => {
                            apply_value(
                                state,
                                &item.location,
                                serde_json::Value::String(price.0),
                                now,
                            );
                            ok += 1;
                        }
                        Err(e) => {
                            // Previously swallowed silently — a missing feed / RPC
                            // failure left the price stale with no trace.
                            tracing::warn!(source = ?item.source, error = %e, "oracle price fetch failed");
                            fail += 1;
                        }
                    }
                }
                Ok((ok, fail))
            }

            BatchKind::Registry { .. } => {
                let registry = match self.registry.as_ref() {
                    Some(r) => r,
                    None => return Ok((0, batch.items.len())),
                };
                let mut ok = 0;
                let mut fail = 0;
                for item in batch.items {
                    match registry.fetch(&item.source).await {
                        Ok(value) => {
                            // Registry values can be assigned directly to the
                            // requested live-field location.
                            apply_value(state, &item.location, value, now);
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
                Ok((ok, fail))
            }

            BatchKind::Derived => {
                // Derived fields in one batch are assumed independent. Callers
                // rerun refresh for multi-layer derived dependencies.
                let mut ok = 0;
                let mut fail = 0;
                for item in batch.items {
                    if let policy_state::DataSource::DerivedFrom { calc_id, inputs } = &item.source
                    {
                        let resolved =
                            crate::resolver::resolve_inputs(state, &self.globals, inputs);
                        let ctx = CalcContext {
                            state,
                            inputs: resolved,
                        };
                        match self.calc.run(calc_id, &ctx) {
                            Ok(value) => {
                                apply_value(state, &item.location, value, now);
                                ok += 1;
                            }
                            Err(_) => fail += 1,
                        }
                    } else {
                        fail += 1;
                    }
                }
                Ok((ok, fail))
            }

            BatchKind::Venue { endpoint } => {
                // Endpoint matching currently routes venue live fields to the
                // Hyperliquid fetcher.
                let is_hl = is_hyperliquid_endpoint(&endpoint);
                let hl = if is_hl {
                    self.hyperliquid.as_ref()
                } else {
                    None
                };
                let hl = match hl {
                    Some(h) => h,
                    None => return Ok((0, batch.items.len())),
                };
                let mut ok = 0;
                let mut fail = 0;
                for item in batch.items {
                    let fetched = match state_market_symbol(state, &item.location) {
                        Some(market_symbol) => {
                            hl.fetch_state_value(
                                &item.source,
                                &item.location,
                                &market_symbol,
                                &state.wallet_id.address,
                            )
                            .await
                        }
                        None => hl.fetch(&item.source).await,
                    };
                    match fetched {
                        Ok(value) => {
                            apply_value(state, &item.location, value, now);
                            ok += 1;
                        }
                        Err(_) => fail += 1,
                    }
                }
                Ok((ok, fail))
            }

            BatchKind::UserSupplied => Ok((0, 0)),
        }
    }
}

fn apply_value(state: &mut WalletState, loc: &FieldLocation, value: Value, now: Time) {
    match loc {
        FieldLocation::TokenPrice { token_key_json } => {
            if let Ok(key) = serde_json::from_str::<policy_state::TokenKey>(token_key_json) {
                if let Some(holding) = state.tokens.get_mut(&key) {
                    if let Some(price) = holding.price_usd.as_mut() {
                        if let Some(p) = value_to_price(&value) {
                            price.value = p;
                            price.synced_at = now;
                            price.confidence = Some(Confidence::fresh());
                        }
                    }
                }
            }
        }
        FieldLocation::LendingHealthFactor { position_id } => {
            if let Some(field) = lending_field_mut(state, position_id, LendingMetric::Hf) {
                set_decimal(field, &value, now);
            }
        }
        FieldLocation::LendingLtv { position_id } => {
            if let Some(field) = lending_field_mut(state, position_id, LendingMetric::Ltv) {
                set_decimal(field, &value, now);
            }
        }
        FieldLocation::LendingLiquidationThreshold { position_id } => {
            if let Some(field) = lending_field_mut(state, position_id, LendingMetric::LiqThr) {
                set_decimal(field, &value, now);
            }
        }
        FieldLocation::PerpMarkPrice { position_id } => {
            if let Some(price) = perp_position_mut(state, position_id).map(|p| &mut p.mark_price) {
                if let Some(p) = value_to_price(&value) {
                    price.value = p;
                    price.synced_at = now;
                    price.confidence = Some(Confidence::fresh());
                }
            }
        }
        FieldLocation::PerpLiqPrice { position_id } => {
            if let Some(field) = perp_position_mut(state, position_id).map(|p| &mut p.liq_price) {
                match &value {
                    Value::Null => {
                        field.value = None;
                        field.synced_at = now;
                        field.confidence = Some(Confidence::fresh());
                    }
                    _ => {
                        if let Some(p) = value_to_price(&value) {
                            field.value = Some(p);
                            field.synced_at = now;
                            field.confidence = Some(Confidence::fresh());
                        }
                    }
                }
            }
        }
        FieldLocation::PerpUnrealizedPnl { position_id } => {
            if let Some(field) =
                perp_position_mut(state, position_id).map(|p| &mut p.unrealized_pnl)
            {
                if let Some(v) = value_to_i256(&value) {
                    field.value = v;
                    field.synced_at = now;
                    field.confidence = Some(Confidence::fresh());
                }
            }
        }
        FieldLocation::PerpFundingOwed { position_id } => {
            if let Some(field) = perp_position_mut(state, position_id).map(|p| &mut p.funding_owed)
            {
                if let Some(v) = value_to_i256(&value) {
                    field.value = v;
                    field.synced_at = now;
                    field.confidence = Some(Confidence::fresh());
                }
            }
        }
        FieldLocation::PerpLeverage { position_id } => {
            if let Some(field) = perp_position_mut(state, position_id).map(|p| &mut p.leverage) {
                set_decimal(field, &value, now);
            }
        }
        FieldLocation::Action { .. } => {}
    }
}

fn value_to_price(v: &Value) -> Option<Price> {
    match v {
        Value::String(s) => Some(policy_state::Decimal::new(s.clone())),
        Value::Number(n) => Some(policy_state::Decimal::new(n.to_string())),
        _ => None,
    }
}

fn value_to_i256(v: &Value) -> Option<SignedI256> {
    use std::str::FromStr;
    match v {
        Value::String(s) => SignedI256::from_str(s).ok(),
        Value::Number(n) => n.as_i64().and_then(|i| SignedI256::try_from(i).ok()),
        _ => None,
    }
}

fn set_decimal(field: &mut LiveField<policy_state::Decimal>, v: &Value, now: Time) {
    if let Some(d) = value_to_price(v) {
        field.value = d;
        field.synced_at = now;
        field.confidence = Some(Confidence::fresh());
    }
}

enum LendingMetric {
    Hf,
    Ltv,
    LiqThr,
}

fn lending_field_mut<'a>(
    state: &'a mut WalletState,
    position_id: &str,
    metric: LendingMetric,
) -> Option<&'a mut LiveField<policy_state::Decimal>> {
    let pos = state.positions.iter_mut().find(|p| p.id == position_id)?;
    match &mut pos.kind {
        PositionKind::LendingAccount(la) => Some(match metric {
            LendingMetric::Hf => &mut la.health_factor,
            LendingMetric::Ltv => &mut la.ltv,
            LendingMetric::LiqThr => &mut la.liquidation_threshold,
        }),
        _ => None,
    }
}

fn perp_position_mut<'a>(
    state: &'a mut WalletState,
    position_id: &str,
) -> Option<&'a mut policy_state::PerpPosition> {
    let pos = state.positions.iter_mut().find(|p| p.id == position_id)?;
    match &mut pos.kind {
        PositionKind::PerpPosition(p) => Some(p),
        _ => None,
    }
}

const HL_ACCOUNT_ID: &str = "hyperliquid/account";

fn upsert_hyperliquid_account(
    state: &mut WalletState,
    account: policy_state::HlAccount,
    source: DataSource,
    now: Time,
) -> Result<(), SyncError> {
    let position = Position {
        id: HL_ACCOUNT_ID.to_owned(),
        protocol: ProtocolRef::new("hyperliquid"),
        chain: None,
        kind: PositionKind::HyperliquidAccount(account),
        primitives_synced_at: now,
        primitives_source: source,
    };

    if let Some(existing) = state.positions.iter_mut().find(|p| p.id == HL_ACCOUNT_ID) {
        if !matches!(existing.kind, PositionKind::HyperliquidAccount(_)) {
            return Err(SyncError::FetchFailed {
                source_id: "hyperliquid".into(),
                reason: format!("{HL_ACCOUNT_ID} exists but is not a HyperliquidAccount"),
            });
        }
        *existing = position;
    } else {
        state.positions.push(position);
    }
    Ok(())
}

/// Upsert discovered intent orders into `state.pending`, keyed by the venue
/// order id embedded in `PendingTx.id`. Existing entries are replaced in place
/// (status transitions); new ones are appended. Each fetcher has already
/// projected its venue's orders into the canonical `PendingTx` shape.
/// Run the intent-order sync loop over `fetchers`, mutating `state` and
/// returning a report. Extracted from `sync_intent_orders` so the loop —
/// including the snapshot-prune safety property — is unit-testable with stub
/// fetchers (the public `Orchestrator` exposes no fetcher injection).
///
/// Per fetcher: a successful fetch is applied via `apply_intent_orders` (which
/// upserts and, when authoritative, snapshot-prunes); a failed fetch is recorded
/// and **never prunes** (the snapshot-prune lives only on the `Ok` arm), so a
/// transient venue error can never drop live tracked orders.
pub(crate) async fn run_intent_sync(
    fetchers: &[&dyn IntentFetcher],
    state: &mut WalletState,
    swapper: &policy_state::primitives::Address,
    now: Time,
) -> IntentOrdersReport {
    let mut report = IntentOrdersReport::default();
    for fetcher in fetchers {
        match fetcher.fetch_orders(swapper, now).await {
            Ok(orders) => {
                report.orders_updated += orders.len();
                report.orders_pruned +=
                    apply_intent_orders(state, &orders, fetcher.authoritative_prefix());
            }
            Err(e) => report.errors.push(format!("{e}")),
        }
    }
    report
}

/// Apply one fetcher's **successful** result to `state`: `upsert_intent_orders`
/// (prune terminal, upsert active/partial), then — when the fetcher is
/// snapshot-authoritative for `prefix` — prune any tracked id under `prefix`
/// absent from `orders` (it left the active listing → no longer open). Returns
/// the number snapshot-pruned. MUST only be called for an `Ok` fetch; see
/// `IntentFetcher::authoritative_prefix` for the completeness contract.
pub(crate) fn apply_intent_orders(
    state: &mut WalletState,
    orders: &[PendingTx],
    authoritative_prefix: Option<&str>,
) -> usize {
    upsert_intent_orders(state, orders);
    let Some(prefix) = authoritative_prefix else {
        return 0;
    };
    let returned: std::collections::HashSet<&str> = orders.iter().map(|o| o.id.as_str()).collect();
    let before = state.pending.len();
    state
        .pending
        .retain(|p| !p.id.starts_with(prefix) || returned.contains(p.id.as_str()));
    before - state.pending.len()
}

pub(crate) fn upsert_intent_orders(state: &mut WalletState, orders: &[PendingTx]) {
    use policy_state::pending::PendingStatus;
    for pending in orders {
        // Terminal orders are pruned from `pending` — filled / cancelled /
        // expired / failed no longer need tracking. Active / partially-filled
        // ones are upserted in place (status transitions) or appended.
        let terminal = matches!(
            pending.lifecycle.status,
            PendingStatus::Filled
                | PendingStatus::Cancelled
                | PendingStatus::Expired
                | PendingStatus::Failed
        );
        if terminal {
            state.pending.retain(|p| p.id != pending.id);
        } else if let Some(existing) = state.pending.iter_mut().find(|p| p.id == pending.id) {
            *existing = pending.clone();
        } else {
            state.pending.push(pending.clone());
        }
    }
}

/// Load-or-create the HL account position, apply `f` (a field-scoped merge), and
/// store it back. Preserves whatever fields `f` leaves untouched; only ever
/// creates/updates the single reserved `HL_ACCOUNT_ID` position. A pre-existing
/// position of a different kind is left alone (the id is reserved for HL).
fn upsert_hyperliquid_merge(
    state: &mut WalletState,
    f: impl FnOnce(&mut policy_state::HlAccount),
    now: Time,
) {
    if let Some(pos) = state.positions.iter_mut().find(|p| p.id == HL_ACCOUNT_ID) {
        if let PositionKind::HyperliquidAccount(acct) = &mut pos.kind {
            f(acct);
            pos.primitives_synced_at = now;
        }
    } else {
        let mut acct = policy_state::HlAccount::default();
        f(&mut acct);
        state.positions.push(Position {
            id: HL_ACCOUNT_ID.to_owned(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::HyperliquidAccount(acct),
            primitives_synced_at: now,
            primitives_source: DataSource::VenueApi {
                endpoint: "https://api.hyperliquid.xyz/info".into(),
                parser_id: "hl_account".into(),
                auth: None,
            },
        });
    }
}

fn is_hyperliquid_endpoint(endpoint: &str) -> bool {
    endpoint.is_empty()
        || endpoint.contains("hyperliquid")
        || endpoint == "https://api.hyperliquid.xyz/info"
}

fn is_uniswap_endpoint(endpoint: &str) -> bool {
    endpoint.contains("api.uniswap.org") || endpoint.contains("uniswap")
}

fn is_permit2_nonce_bitmap_source(source: &DataSource) -> bool {
    matches!(
        source,
        DataSource::OnchainView { decoder_id, .. } if decoder_id == "permit2_nonce_bitmap"
    )
}

fn permit2_nonce_bitmap_apply_value(
    action: &Action,
    location: &FieldLocation,
    value: &Value,
) -> Option<Result<Value, SyncError>> {
    let FieldLocation::Action { slot, .. } = location else {
        return None;
    };
    if !matches!(slot, ActionSlot::TokenPermit2SignNonce) {
        return None;
    }
    let Some((word, bit)) = permit2_nonce_pair_for_location(action, location) else {
        return Some(Err(SyncError::FetchFailed {
            source_id: "permit2_nonce_bitmap".into(),
            reason: "action location is not a Permit2 unordered nonce".into(),
        }));
    };
    let Some(bitmap) = u256_from_json_decimal(value) else {
        return Some(Err(SyncError::FetchFailed {
            source_id: "permit2_nonce_bitmap".into(),
            reason: format!("expected bitmap u256 string, got {value}"),
        }));
    };
    if bitmap_bit_is_set(bitmap, bit) {
        return Some(Err(SyncError::FetchFailed {
            source_id: "permit2_nonce_bitmap".into(),
            reason: format!("Permit2 nonce bit already used: word={word}, bit={bit}"),
        }));
    }
    Some(Ok(serde_json::json!([word.to_string(), bit])))
}

fn permit2_nonce_pair_for_location(
    action: &Action,
    location: &FieldLocation,
) -> Option<(U256, u8)> {
    let FieldLocation::Action { action_index, .. } = location else {
        return None;
    };
    match body_at_index(&action.body, *action_index)? {
        ActionBody::Token(TokenAction::Permit2SignAllowance(p)) => Some(p.nonce.value),
        ActionBody::Token(TokenAction::Permit2SignTransfer(p)) => Some(p.nonce.value),
        ActionBody::Token(TokenAction::Permit2TransferFrom(p)) => Some(p.nonce.value),
        _ => None,
    }
}

fn u256_from_json_decimal(value: &Value) -> Option<U256> {
    match value {
        Value::String(s) => U256::from_str_radix(s, 10).ok(),
        Value::Number(n) => U256::from_str_radix(&n.to_string(), 10).ok(),
        _ => None,
    }
}

fn bitmap_bit_is_set(bitmap: U256, bit: u8) -> bool {
    let bytes = bitmap.to_be_bytes::<32>();
    let byte_index = 31usize.saturating_sub(usize::from(bit / 8));
    let bit_index = bit % 8;
    (bytes[byte_index] & (1u8 << bit_index)) != 0
}

// ---------------------------------------------------------------------------
// Permit reconciler helpers (pure decision logic, factored for unit tests).
// ---------------------------------------------------------------------------

/// Canonical Permit2 contract address — same on every EVM chain (matches the
/// hardcoded spender in `discovery/known_spenders.rs`).
const PERMIT2_ADDRESS_HEX: &str = "0x000000000022d473030f116ddee9f6b43ac78ba3";

/// `nonceBitmap(address,uint256)` selector — `keccak256(sig)[..4]`. Verified by
/// `nonce_bitmap_selector_is_correct`.
const NONCE_BITMAP_SELECTOR: [u8; 4] = [0x4f, 0xe0, 0x2b, 0x44];

fn permit2_contract_address() -> Result<policy_state::primitives::Address, SyncError> {
    use std::str::FromStr;
    policy_state::primitives::Address::from_str(PERMIT2_ADDRESS_HEX).map_err(|e| {
        SyncError::FetchFailed {
            source_id: "permit2_nonce_bitmap".into(),
            reason: format!("bad Permit2 address constant: {e}"),
        }
    })
}

/// ABI-encode `nonceBitmap(owner, word)` calldata: selector + owner (left-
/// padded) + word.
fn encode_nonce_bitmap(owner: policy_state::primitives::Address, word: U256) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 32 + 32);
    out.extend_from_slice(&NONCE_BITMAP_SELECTOR);
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(owner.as_slice());
    out.extend_from_slice(&word.to_be_bytes::<32>());
    out
}

/// `nonces(address)` (EIP-2612) selector — `keccak256(sig)[..4]`. Verified by
/// `nonces_selector_is_correct`.
const NONCES_SELECTOR: [u8; 4] = [0x7e, 0xce, 0xbe, 0x00];

/// An EIP-2612 permit signed with `signed` is consumed once the on-chain
/// `nonces(owner)` has advanced strictly past it. Nonces are sequential and
/// in-order, so `onchain > signed` ⟹ the signed nonce was used (or invalidated by
/// a later in-order use); `onchain == signed` means it is still the next usable
/// nonce → NOT yet consumed.
fn eip2612_nonce_is_consumed(onchain: U256, signed: U256) -> bool {
    onchain > signed
}

/// ABI-encode `nonces(owner)` calldata: selector + owner (left-padded).
fn encode_nonces(owner: policy_state::primitives::Address) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 32);
    out.extend_from_slice(&NONCES_SELECTOR);
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(owner.as_slice());
    out
}

/// Decode a single `uint256` return value (the bitmap word). `None` when the
/// return is shorter than 32 bytes (treat as unreadable, not "unset").
fn decode_u256_be(return_data: &[u8]) -> Option<U256> {
    if return_data.len() < 32 {
        return None;
    }
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&return_data[..32]);
    Some(U256::from_be_bytes(bytes))
}

/// True for the three signed off-chain permit/permit2 pending kinds the
/// reconciler owns. Excludes intent/perp/limit pendings that share `pending`.
const fn is_signed_permit_kind(kind: &PendingKind) -> bool {
    matches!(
        kind,
        PendingKind::SignedEIP2612 { .. }
            | PendingKind::SignedPermit2 { .. }
            | PendingKind::SignedPermit2Transfer { .. }
    )
}

/// A signed permit is expired when it has a `valid_until` strictly before `now`.
/// A `None` deadline (shouldn't happen for these kinds — the reducers always set
/// it) is treated as never-expiring.
fn permit_is_expired(valid_until: Option<Time>, now: Time) -> bool {
    matches!(valid_until, Some(until) if until.as_unix() < now.as_unix())
}

fn action_body_for_location<'a>(
    action: &'a Action,
    location: &FieldLocation,
) -> Option<&'a ActionBody> {
    let FieldLocation::Action { action_index, .. } = location else {
        return None;
    };
    body_at_index(&action.body, *action_index)
}

fn state_market_symbol(state: &WalletState, location: &FieldLocation) -> Option<String> {
    match location {
        FieldLocation::PerpMarkPrice { position_id }
        | FieldLocation::PerpLiqPrice { position_id }
        | FieldLocation::PerpUnrealizedPnl { position_id }
        | FieldLocation::PerpFundingOwed { position_id }
        | FieldLocation::PerpLeverage { position_id } => state
            .positions
            .iter()
            .find(|p| p.id == *position_id)
            .and_then(|p| match &p.kind {
                PositionKind::PerpPosition(perp) => Some(perp.market.symbol.clone()),
                _ => None,
            }),
        _ => None,
    }
}

fn action_market_symbol(
    action: &Action,
    state: &WalletState,
    location: &FieldLocation,
) -> Option<String> {
    let FieldLocation::Action { action_index, .. } = location else {
        return None;
    };
    let body = body_at_index(&action.body, *action_index)?;
    let ActionBody::Perp(perp) = body else {
        return None;
    };
    perp_action_market_symbol(perp, state)
}

fn body_at_index(body: &ActionBody, index: usize) -> Option<&ActionBody> {
    match body {
        ActionBody::Multicall { actions } => actions.get(index),
        single if index == 0 => Some(single),
        _ => None,
    }
}

fn perp_action_market_symbol(perp: &PerpAction, state: &WalletState) -> Option<String> {
    match perp {
        PerpAction::OpenPosition(a) => Some(a.market.symbol.clone()),
        PerpAction::ClosePosition(a) => state_position_market_symbol(state, &a.position_id),
        PerpAction::IncreasePosition(a) => state_position_market_symbol(state, &a.position_id),
        PerpAction::DecreasePosition(a) => state_position_market_symbol(state, &a.position_id),
        PerpAction::AdjustMargin(a) => state_position_market_symbol(state, &a.position_id),
        PerpAction::ChangeLeverage(a) => Some(a.market.symbol.clone()),
        PerpAction::ChangeMarginMode(a) => Some(a.market.symbol.clone()),
        PerpAction::PlaceOrder(a) => Some(a.market.symbol.clone()),
        PerpAction::CancelOrder(_) => None,
        PerpAction::ClaimFunding(a) => a.market.as_ref().map(|m| m.symbol.clone()),
    }
}

fn state_position_market_symbol(state: &WalletState, position_id: &str) -> Option<String> {
    state
        .positions
        .iter()
        .find(|p| p.id == position_id)
        .and_then(|p| match &p.kind {
            PositionKind::PerpPosition(perp) => Some(perp.market.symbol.clone()),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::{Address, ChainId};

    #[test]
    fn upsert_intent_orders_tracks_active_and_prunes_on_terminal() {
        use crate::fetchers::UniswapXOrder;
        use policy_state::pending::PendingStatus;
        use policy_state::{WalletId, U256};

        let reactor = Address::ZERO;
        let swapper = Address::ZERO;
        let now = Time::from_unix(1_738_000_000);

        let mut state =
            WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));

        // Round 1: one open order is discovered → added as Active.
        let open = UniswapXOrder {
            order_hash: "0xhash1".into(),
            order_status: "open".into(),
            order_type: "Dutch_V2".into(),
            chain_id: 1,
            deadline: Some(1_738_003_600),
            sell_token: Address::ZERO,
            sell_amount: U256::from(600u64),
            buy_token: Address::ZERO,
            buy_min: U256::from(1u64),
        };
        super::upsert_intent_orders(&mut state, &[open.to_pending_tx(reactor, &swapper, now)]);
        assert_eq!(state.pending.len(), 1);
        assert_eq!(state.pending[0].id, "intent:uniswap_x:0xhash1");
        assert_eq!(state.pending[0].lifecycle.status, PendingStatus::Active);

        // Round 2: same hash now filled → pruned from pending (terminal cleanup).
        let filled = UniswapXOrder {
            order_status: "filled".into(),
            ..open
        };
        super::upsert_intent_orders(&mut state, &[filled.to_pending_tx(reactor, &swapper, now)]);
        assert!(
            state.pending.is_empty(),
            "terminal order pruned from pending"
        );
    }

    #[tokio::test]
    async fn intent_fetcher_trait_dispatch_persists_orders() {
        use crate::fetchers::IntentFetcher;
        use async_trait::async_trait;
        use policy_state::pending::{
            AssetCommitment, OrderKind, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
        };
        use policy_state::primitives::{Address, ChainId, Time, VenueRef, U256};
        use policy_state::token::{TokenKey, TokenRef};
        use policy_state::{DataSource, StateDelta, WalletId};

        fn make_pending(id: &str) -> PendingTx {
            let token = TokenRef {
                key: TokenKey::Native {
                    chain: ChainId::ethereum_mainnet(),
                },
            };
            PendingTx {
                id: id.into(),
                kind: PendingKind::OffchainLimitOrder {
                    venue: VenueRef {
                        name: "stub".into(),
                        chain: Some(ChainId::ethereum_mainnet()),
                    },
                    sell: token.clone(),
                    buy: token.clone(),
                    sell_max: U256::from(1u64),
                    buy_min: U256::from(1u64),
                    order_kind: OrderKind::Limit,
                },
                commitment: AssetCommitment::PermitCap {
                    token,
                    spender: Address::ZERO,
                    max_out: U256::from(1u64),
                },
                fill_effect: Box::new(StateDelta::new()),
                lifecycle: PendingLifecycle {
                    status: PendingStatus::Active,
                    valid_until: None,
                    nonce: None,
                    on_chain_tx: None,
                    raw_status: None,
                },
                sync: DataSource::UserSupplied,
                signed_at: Time::from_unix(0),
                signature_payload: Vec::new(),
            }
        }

        struct StubFetcher;
        #[async_trait]
        impl IntentFetcher for StubFetcher {
            async fn fetch_orders(
                &self,
                _swapper: &Address,
                _now: Time,
            ) -> Result<Vec<PendingTx>, SyncError> {
                Ok(vec![
                    make_pending("intent:stub:a"),
                    make_pending("intent:stub:b"),
                ])
            }
        }

        let mut state =
            WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
        let orders = StubFetcher
            .fetch_orders(&Address::ZERO, Time::from_unix(0))
            .await
            .unwrap();
        assert_eq!(orders.len(), 2);
        super::upsert_intent_orders(&mut state, &orders);

        assert_eq!(state.pending.len(), 2);
        assert!(state.pending.iter().any(|p| p.id == "intent:stub:a"));
        assert!(state.pending.iter().any(|p| p.id == "intent:stub:b"));
    }

    // --- snapshot-prune (authoritative_prefix) for active-orderbook venues ---

    fn snapshot_pending(id: &str) -> PendingTx {
        use policy_state::pending::{
            AssetCommitment, OrderKind, PendingKind, PendingLifecycle, PendingStatus,
        };
        use policy_state::primitives::{Address, ChainId, Time, VenueRef, U256};
        use policy_state::token::{TokenKey, TokenRef};
        use policy_state::{DataSource, StateDelta};

        let token = TokenRef {
            key: TokenKey::Native {
                chain: ChainId::ethereum_mainnet(),
            },
        };
        PendingTx {
            id: id.into(),
            kind: PendingKind::OffchainLimitOrder {
                venue: VenueRef::new("one_inch_limit_order"),
                sell: token.clone(),
                buy: token.clone(),
                sell_max: U256::from(1u64),
                buy_min: U256::from(1u64),
                order_kind: OrderKind::Limit,
            },
            commitment: AssetCommitment::PermitCap {
                token,
                spender: Address::ZERO,
                max_out: U256::from(1u64),
            },
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status: PendingStatus::Active,
                valid_until: None,
                nonce: None,
                on_chain_tx: None,
                raw_status: None,
            },
            sync: DataSource::UserSupplied,
            signed_at: Time::from_unix(0),
            signature_payload: Vec::new(),
        }
    }

    /// A configurable stub: returns a fixed `Ok`/`Err`, with a configurable
    /// `authoritative_prefix`.
    struct PrefixStub {
        result: Result<Vec<PendingTx>, ()>,
        prefix: Option<&'static str>,
    }

    #[async_trait::async_trait]
    impl IntentFetcher for PrefixStub {
        async fn fetch_orders(
            &self,
            _swapper: &policy_state::primitives::Address,
            _now: Time,
        ) -> Result<Vec<PendingTx>, SyncError> {
            self.result.clone().map_err(|()| SyncError::FetchFailed {
                source_id: "prefix_stub".into(),
                reason: "boom".into(),
            })
        }
        fn authoritative_prefix(&self) -> Option<&str> {
            self.prefix
        }
    }

    fn state_with(ids: &[&str]) -> WalletState {
        use policy_state::primitives::{Address, ChainId};
        use policy_state::WalletId;
        let mut state =
            WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
        for id in ids {
            state.pending.push(snapshot_pending(id));
        }
        state
    }

    #[tokio::test]
    async fn snapshot_prune_retires_orders_absent_from_authoritative_fetch() {
        // Two LOP orders tracked; the fetch returns only `a` (b left the book) +
        // a non-LOP order is untouched.
        let mut state = state_with(&[
            "intent:one_inch_limit_order:a",
            "intent:one_inch_limit_order:b",
            "intent:cow_swap:z",
        ]);
        let fetcher = PrefixStub {
            result: Ok(vec![snapshot_pending("intent:one_inch_limit_order:a")]),
            prefix: Some("intent:one_inch_limit_order:"),
        };
        let report = run_intent_sync(
            &[&fetcher],
            &mut state,
            &policy_state::primitives::Address::ZERO,
            Time::from_unix(0),
        )
        .await;

        assert_eq!(report.orders_pruned, 1);
        assert!(report.errors.is_empty());
        let ids: Vec<&str> = state.pending.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"intent:one_inch_limit_order:a"));
        assert!(
            !ids.contains(&"intent:one_inch_limit_order:b"),
            "b left the active book → pruned"
        );
        assert!(
            ids.contains(&"intent:cow_swap:z"),
            "a different venue's order is never touched by this prefix"
        );
    }

    #[tokio::test]
    async fn failed_authoritative_fetch_never_prunes() {
        // THE load-bearing safety test: a venue error must NOT drop live orders.
        let mut state = state_with(&[
            "intent:one_inch_limit_order:a",
            "intent:one_inch_limit_order:b",
        ]);
        let fetcher = PrefixStub {
            result: Err(()),
            prefix: Some("intent:one_inch_limit_order:"),
        };
        let report = run_intent_sync(
            &[&fetcher],
            &mut state,
            &policy_state::primitives::Address::ZERO,
            Time::from_unix(0),
        )
        .await;

        assert_eq!(report.orders_pruned, 0);
        assert_eq!(report.errors.len(), 1, "the error is recorded");
        assert_eq!(
            state.pending.len(),
            2,
            "a transient fetch error must leave every tracked order in place"
        );
    }

    #[tokio::test]
    async fn no_prefix_fetcher_does_not_snapshot_prune() {
        // A default (None-prefix) fetcher returning a subset must NOT prune the
        // orders it didn't return — only `upsert_intent_orders` semantics apply.
        let mut state = state_with(&[
            "intent:one_inch_limit_order:a",
            "intent:one_inch_limit_order:b",
        ]);
        let fetcher = PrefixStub {
            result: Ok(vec![snapshot_pending("intent:one_inch_limit_order:a")]),
            prefix: None,
        };
        let report = run_intent_sync(
            &[&fetcher],
            &mut state,
            &policy_state::primitives::Address::ZERO,
            Time::from_unix(0),
        )
        .await;

        assert_eq!(report.orders_pruned, 0);
        assert_eq!(
            state.pending.len(),
            2,
            "without an authoritative prefix, absent orders are left as-is"
        );
    }

    #[tokio::test]
    async fn refresh_empty_state_is_noop() {
        let toml = r#"
[chains."eip155:1"]
multicall_addr = "0xcA11bde05977b3631167028862bE2a173976CA11"
[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
        let cfg = crate::RpcConfig::load_str(toml).unwrap();
        let router = std::sync::Arc::new(crate::RpcRouter::from_config(cfg).unwrap());
        let orch = Orchestrator::from_rpc_router(router);

        let mut state = WalletState::new(policy_state::WalletId::new(
            Address::ZERO,
            [ChainId::ethereum_mainnet()],
        ));
        let report = orch.refresh(&mut state, Time::from_unix(0)).await.unwrap();
        assert_eq!(report.walked.total_live_fields, 0);
        assert_eq!(report.fields_updated, 0);
        assert_eq!(report.batches_processed, 0);
    }

    #[test]
    fn upsert_hl_merge_creates_updates_and_preserves_across_domains() {
        use policy_state::{CoreFresh, Decimal, HlAccount, LongtailFresh, WalletId};

        let mut state =
            WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));

        // A core snapshot whose clearinghouse domain is fresh (rest stale).
        let fresh = CoreFresh {
            clearinghouse: true,
            ..Default::default()
        };

        // (1) first core sync creates the HL position.
        let core = HlAccount {
            perp_usdc: Some(Decimal::new("5")),
            ..Default::default()
        };
        upsert_hyperliquid_merge(
            &mut state,
            |a| a.merge_core(core, fresh),
            Time::from_unix(0),
        );

        // (2) a long-tail sync (all-stale here) must NOT wipe the core perp_usdc.
        upsert_hyperliquid_merge(
            &mut state,
            |a| a.merge_longtail(HlAccount::default(), LongtailFresh::default()),
            Time::from_unix(1),
        );

        // (3) a second core sync updates the same position in place.
        let core2 = HlAccount {
            perp_usdc: Some(Decimal::new("9")),
            ..Default::default()
        };
        upsert_hyperliquid_merge(
            &mut state,
            |a| a.merge_core(core2, fresh),
            Time::from_unix(2),
        );

        let hits: Vec<_> = state
            .positions
            .iter()
            .filter(|p| p.id == HL_ACCOUNT_ID)
            .collect();
        assert_eq!(hits.len(), 1); // upsert, not duplicate
        let PositionKind::HyperliquidAccount(acct) = &hits[0].kind else {
            panic!("not an HL account");
        };
        assert_eq!(acct.perp_usdc, Some(Decimal::new("9"))); // last core, preserved across long-tail
    }

    #[tokio::test]
    async fn derived_hf_computes_from_globals() {
        use policy_state::{
            DataSource, Decimal, Duration, FieldRef, LendingAccount, LiveField, MarketRef,
            Position, PositionKind, Time as T, VenueRef, WalletId,
        };

        let toml = r#"
[chains."eip155:1"]
[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
        let cfg = crate::RpcConfig::load_str(toml).unwrap();
        let router = std::sync::Arc::new(crate::RpcRouter::from_config(cfg).unwrap());
        let mut orch = Orchestrator::from_rpc_router(router);

        // collateral=1000, debt=500, liq_threshold=0.8 → HF = (1000*0.8)/500 = 1.6
        orch.set_global("collateral_usd", serde_json::json!("1000"));
        orch.set_global("debt_usd", serde_json::json!("500"));
        orch.set_global("liq_threshold", serde_json::json!("0.8"));

        let hf_source = DataSource::DerivedFrom {
            calc_id: "aave_hf".into(),
            inputs: vec![
                FieldRef::Global {
                    name: "collateral_usd".into(),
                },
                FieldRef::Global {
                    name: "debt_usd".into(),
                },
                FieldRef::Global {
                    name: "liq_threshold".into(),
                },
            ],
        };

        let stale_at = T::from_unix(0);
        let now = T::from_unix(10_000);
        let fresh_source = DataSource::UserSupplied;

        let lending = LendingAccount {
            market: MarketRef {
                symbol: "aave-v3".into(),
                venue: VenueRef::new("aave"),
            },
            collaterals: vec![],
            debts: vec![],
            emode: None,
            is_isolated: false,
            health_factor: LiveField::new(Decimal::new("0"), hf_source, stale_at)
                .with_ttl(Duration::from_secs(60)),
            ltv: LiveField::new(Decimal::new("0"), fresh_source.clone(), now)
                .with_ttl(Duration::from_secs(60)),
            liquidation_threshold: LiveField::new(Decimal::new("0.8"), fresh_source, now)
                .with_ttl(Duration::from_secs(60)),
        };

        let mut state =
            WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
        state.positions.push(Position {
            id: "aave_v3:main".into(),
            protocol: policy_state::ProtocolRef::new("aave_v3"),
            chain: Some(ChainId::ethereum_mainnet()),
            kind: PositionKind::LendingAccount(lending),
            primitives_synced_at: now,
            primitives_source: DataSource::UserSupplied,
        });

        let report = orch.refresh(&mut state, now).await.unwrap();
        assert!(report.fields_updated >= 1, "HF should have been updated");

        if let PositionKind::LendingAccount(la) = &state.positions[0].kind {
            assert_eq!(la.health_factor.value.as_str(), "1.6");
        } else {
            panic!("expected lending account");
        }
    }

    #[tokio::test]
    async fn sync_hyperliquid_account_replaces_local_account_with_snapshot() {
        use std::str::FromStr;

        use policy_state::{
            DataSource, Decimal, HlAccount, HlBorrowLendAccount, HlBorrowLendBalance,
            HlBorrowLendTokenState, HlOpenOrder, HlSpotBalance, HlStakingAccount, HlVaultEquity,
            Position, PositionKind, ProtocolRef, Time as T, WalletId,
        };
        use serde_json::{json, Value};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::{TcpListener, TcpStream};

        async fn read_request_json(stream: &mut TcpStream) -> Value {
            let mut buf = Vec::new();
            let mut tmp = [0u8; 1024];
            loop {
                let n = stream.read(&mut tmp).await.unwrap();
                assert!(n > 0, "connection closed before request body");
                buf.extend_from_slice(&tmp[..n]);
                let Some(header_end) = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&buf[..header_end]);
                let len = headers
                    .lines()
                    .find_map(|line| {
                        let lower = line.to_ascii_lowercase();
                        lower
                            .strip_prefix("content-length:")
                            .and_then(|s| s.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if buf.len() >= header_end + len {
                    return serde_json::from_slice(&buf[header_end..header_end + len]).unwrap();
                }
            }
        }

        async fn write_json(stream: &mut TcpStream, body: Value) {
            let body = body.to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }

        async fn spawn_hl_info_server() -> String {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                loop {
                    let Ok((mut stream, _)) = listener.accept().await else {
                        break;
                    };
                    tokio::spawn(async move {
                        let req = read_request_json(&mut stream).await;
                        let dex = req.get("dex").and_then(Value::as_str);
                        let body = match (req["type"].as_str().unwrap_or_default(), dex) {
                            ("clearinghouseState", Some("xyz")) => json!({
                                "marginSummary": {
                                    "accountValue": "1077.754757",
                                    "totalNtlPos": "5257.5954",
                                    "totalRawUsd": "-4179.840643",
                                    "totalMarginUsed": "1077.754757"
                                },
                                "crossMarginSummary": {
                                    "accountValue": "1077.754757",
                                    "totalNtlPos": "5257.5954",
                                    "totalRawUsd": "-4179.840643",
                                    "totalMarginUsed": "1077.754757"
                                },
                                "crossMaintenanceMarginUsed": "0",
                                "withdrawable": "0",
                                "assetPositions": [{
                                    "type": "oneWay",
                                    "position": {
                                        "coin": "xyz:SPCX",
                                        "szi": "25.77",
                                        "leverage": { "type": "isolated", "value": 5, "rawUsd": "-4179.840643" },
                                        "entryPx": "202.74",
                                        "positionValue": "5257.5954",
                                        "unrealizedPnl": "32.9856",
                                        "returnOnEquity": "0.033",
                                        "liquidationPx": "180.2199216574",
                                        "marginUsed": "1077.754757",
                                        "maxLeverage": 5,
                                        "cumFunding": {
                                            "allTime": "0.003908",
                                            "sinceOpen": "0.003908",
                                            "sinceChange": "0.003908"
                                        }
                                    }
                                }],
                                "time": 1_710_000_000_123_u64
                            }),
                            ("clearinghouseState", None) => json!({
                                "marginSummary": {
                                    "accountValue": "1000",
                                    "totalNtlPos": "6000",
                                    "totalRawUsd": "1000",
                                    "totalMarginUsed": "200"
                                },
                                "crossMarginSummary": {
                                    "accountValue": "1000",
                                    "totalNtlPos": "6000",
                                    "totalRawUsd": "1000",
                                    "totalMarginUsed": "200"
                                },
                                "crossMaintenanceMarginUsed": "50",
                                "withdrawable": "800",
                                "assetPositions": [{
                                    "type": "oneWay",
                                    "position": {
                                        "coin": "BTC",
                                        "szi": "0.1",
                                        "leverage": { "type": "cross", "value": 5 },
                                        "entryPx": "60000",
                                        "positionValue": "6000",
                                        "unrealizedPnl": "12",
                                        "returnOnEquity": "0.02",
                                        "liquidationPx": "50000",
                                        "marginUsed": "1200",
                                        "maxLeverage": 50,
                                        "cumFunding": {
                                            "allTime": "-1",
                                            "sinceOpen": "-1",
                                            "sinceChange": "0"
                                        }
                                    }
                                }],
                                "time": 1_710_000_000_123_u64
                            }),
                            ("frontendOpenOrders", Some("xyz")) => json!([{
                                "timestamp": 1_780_211_477_428_u64,
                                "coin": "xyz:SPCX",
                                "side": "A",
                                "limitPx": "170.2",
                                "sz": "0.0",
                                "oid": 449_792_035_550_u64,
                                "origSz": "0.0",
                                "cloid": null,
                                "orderType": "Stop Market",
                                "tif": null,
                                "reduceOnly": true,
                                "triggerCondition": "Price below 185",
                                "isTrigger": true,
                                "triggerPx": "185.0",
                                "children": [],
                                "isPositionTpsl": true
                            }]),
                            ("frontendOpenOrders", None) => json!([{
                                "timestamp": 1_710_000_000_124_u64,
                                "coin": "ETH",
                                "side": "B",
                                "limitPx": "3000",
                                "sz": "0.25",
                                "oid": 42,
                                "origSz": "0.25",
                                "cloid": null,
                                "orderType": "Limit",
                                "tif": "Gtc",
                                "reduceOnly": false
                            }]),
                            ("extraAgents", None) => json!([{
                                "name": "bot",
                                "address": "0x1111111111111111111111111111111111111111",
                                "validUntil": 1_710_000_000_999_u64
                            }]),
                            ("spotClearinghouseState", None) => json!({
                                "balances": [{
                                    "coin": "USDC",
                                    "token": 0,
                                    "total": "1125.961894",
                                    "hold": "1077.497057",
                                    "entryNtl": "0.0"
                                }],
                                "tokenToAvailableAfterMaintenance": [[0, "48.464837"]]
                            }),
                            ("delegatorSummary", None) => json!({
                                "delegated": "0.0",
                                "undelegated": "0.0",
                                "totalPendingWithdrawal": "46.84529183",
                                "nPendingWithdrawals": 1
                            }),
                            ("delegations", None) => json!([]),
                            ("userVaultEquities", None) => json!([{
                                "vaultAddress": "0x3333333333333333333333333333333333333333",
                                "equity": "742500.082809",
                                "lockedUntilTimestamp": 1_741_132_800_000_u64
                            }]),
                            ("borrowLendUserState", None) => json!({
                                "tokenToState": [[
                                    0,
                                    {
                                        "borrow": { "basis": "0.0", "value": "0.0" },
                                        "supply": {
                                            "basis": "44.69295862",
                                            "value": "44.69692314"
                                        }
                                    }
                                ]],
                                "health": "healthy",
                                "healthFactor": null
                            }),
                            ("meta", Some("xyz")) => json!({
                                "universe": [
                                    { "name": "xyz:SPCX", "maxLeverage": 5, "szDecimals": 2 }
                                ],
                                "collateralToken": 0
                            }),
                            ("meta", None) => json!({
                                "universe": [
                                    { "name": "BTC", "maxLeverage": 50, "szDecimals": 5 },
                                    { "name": "ETH", "maxLeverage": 25, "szDecimals": 4 }
                                ],
                                "collateralToken": 0
                            }),
                            ("perpDexs", None) => json!([{ "name": "xyz", "fullName": "XYZ" }]),
                            (other, dex) => panic!("unexpected info request: {other}/{dex:?}"),
                        };
                        write_json(&mut stream, body).await;
                    });
                }
            });
            format!("http://{addr}")
        }

        let toml = r#"
[chains."eip155:1"]
[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
        let cfg = crate::RpcConfig::load_str(toml).unwrap();
        let router = std::sync::Arc::new(crate::RpcRouter::from_config(cfg).unwrap());
        let base_url = spawn_hl_info_server().await;
        let orch = Orchestrator::from_rpc_router(router)
            .with_hyperliquid(crate::fetchers::HyperliquidFetcher::with_base_url(base_url));

        let now = T::from_unix(10_000);
        let user = Address::from_str("0x2222222222222222222222222222222222222222").unwrap();
        let mut state =
            policy_state::WalletState::new(WalletId::new(user, [ChainId::ethereum_mainnet()]));
        state.positions.push(Position {
            id: HL_ACCOUNT_ID.to_owned(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                perp_usdc: Some(Decimal::new("10")),
                pending_outflow: Decimal::new("99"),
                positions: Vec::new(),
                open_orders: vec![HlOpenOrder {
                    asset_index: 99,
                    symbol: Some("OLD".to_owned()),
                    is_buy: false,
                    price: Decimal::new("1"),
                    size: Decimal::new("1"),
                    reduce_only: false,
                    tif: "gtc".to_owned(),
                    oid: Some(1),
                    order_type: None,
                    is_trigger: None,
                    trigger_price: None,
                    trigger_condition: None,
                    is_position_tpsl: None,
                }],
                spot_balances: vec![HlSpotBalance {
                    coin: "OLD".to_owned(),
                    token: 999,
                    total: Decimal::new("1"),
                    hold: Decimal::new("1"),
                    entry_ntl: Decimal::new("1"),
                    available_after_maintenance: None,
                }],
                staking: Some(HlStakingAccount {
                    delegated: Decimal::new("1"),
                    undelegated: Decimal::new("1"),
                    total_pending_withdrawal: Decimal::new("1"),
                    n_pending_withdrawals: 1,
                    delegations: Vec::new(),
                }),
                vault_equities: vec![HlVaultEquity {
                    vault_address: Address::from([0x99; 20]),
                    equity: Decimal::new("1"),
                    locked_until_timestamp: None,
                }],
                borrow_lend: Some(HlBorrowLendAccount {
                    token_states: vec![HlBorrowLendTokenState {
                        token: 999,
                        borrow: HlBorrowLendBalance {
                            basis: Decimal::new("1"),
                            value: Decimal::new("1"),
                        },
                        supply: HlBorrowLendBalance {
                            basis: Decimal::new("1"),
                            value: Decimal::new("1"),
                        },
                    }],
                    health: Some("old".to_owned()),
                    health_factor: Some(Decimal::new("1")),
                }),
                ..HlAccount::default()
            }),
            primitives_synced_at: T::from_unix(0),
            primitives_source: DataSource::UserSupplied,
        });

        let report = orch
            .sync_hyperliquid_account(&mut state, now)
            .await
            .unwrap();
        assert!(report.account_updated);

        let account = state
            .positions
            .iter()
            .find_map(|p| match &p.kind {
                PositionKind::HyperliquidAccount(a) if p.id == HL_ACCOUNT_ID => Some(a),
                _ => None,
            })
            .unwrap();
        assert_eq!(account.perp_usdc, Some(Decimal::new("800")));
        assert_eq!(
            account.perp_account_value_usd,
            Some(Decimal::new("2077.754757"))
        );
        assert_eq!(account.pending_outflow, Decimal::new("0"));
        assert_eq!(account.positions.len(), 2);
        assert_eq!(account.positions[0].symbol.as_deref(), Some("BTC"));
        assert_eq!(account.positions[1].symbol.as_deref(), Some("xyz:SPCX"));
        assert_eq!(account.open_orders.len(), 2);
        assert_eq!(account.open_orders[0].symbol.as_deref(), Some("ETH"));
        assert_eq!(account.open_orders[0].oid, Some(42));
        assert_eq!(account.open_orders[1].symbol.as_deref(), Some("xyz:SPCX"));
        assert_eq!(
            account.open_orders[1].order_type.as_deref(),
            Some("Stop Market")
        );
        assert_eq!(account.open_orders[1].is_trigger, Some(true));
        assert_eq!(
            account.open_orders[1].trigger_price,
            Some(Decimal::new("185"))
        );
        assert_eq!(
            account.open_orders[1].trigger_condition.as_deref(),
            Some("Price below 185")
        );
        assert_eq!(account.open_orders[1].is_position_tpsl, Some(true));
        assert_eq!(account.spot_balances.len(), 1);
        assert_eq!(account.spot_balances[0].coin, "USDC");
        assert_eq!(
            account.spot_balances[0].available_after_maintenance,
            Some(Decimal::new("48.464837"))
        );
        let staking = account.staking.as_ref().unwrap();
        assert_eq!(
            staking.total_pending_withdrawal,
            Decimal::new("46.84529183")
        );
        assert_eq!(staking.n_pending_withdrawals, 1);
        assert!(staking.delegations.is_empty());
        assert_eq!(account.vault_equities.len(), 1);
        assert_eq!(
            account.vault_equities[0].equity,
            Decimal::new("742500.082809")
        );
        assert_eq!(
            account.vault_equities[0].locked_until_timestamp,
            Some(1_741_132_800_000_u64)
        );
        let borrow_lend = account.borrow_lend.as_ref().unwrap();
        assert_eq!(borrow_lend.health.as_deref(), Some("healthy"));
        assert_eq!(borrow_lend.token_states.len(), 1);
        assert_eq!(
            borrow_lend.token_states[0].supply.value,
            Decimal::new("44.69692314")
        );
        assert_eq!(account.agents.len(), 1);
    }

    // ---------- permit reconciler ----------

    mod permit_reconcile {
        use super::super::*;
        use policy_state::pending::{
            AssetCommitment, NonceKey, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
        };
        use policy_state::primitives::{Address, ChainId, Time, U256};
        use policy_state::token::{TokenKey, TokenRef};
        use policy_state::{DataSource, StateDelta, WalletId, WalletState};

        fn usdc() -> TokenRef {
            TokenRef {
                key: TokenKey::Erc20 {
                    chain: ChainId::ethereum_mainnet(),
                    address: Address::from([0x11; 20]),
                },
            }
        }

        fn lifecycle(valid_until: Option<Time>) -> PendingLifecycle {
            PendingLifecycle {
                status: PendingStatus::Active,
                valid_until,
                nonce: None,
                on_chain_tx: None,
                raw_status: None,
            }
        }

        fn signed_eip2612(id: &str, valid_until: Option<Time>) -> PendingTx {
            PendingTx {
                id: id.into(),
                kind: PendingKind::SignedEIP2612 {
                    token: usdc(),
                    spender: Address::from([0x22; 20]),
                    amount: U256::from(1u64),
                    expires_at: valid_until.unwrap_or(Time::from_unix(0)),
                    nonce: U256::from(7u64),
                },
                commitment: AssetCommitment::PermitCap {
                    token: usdc(),
                    spender: Address::from([0x22; 20]),
                    max_out: U256::from(1u64),
                },
                fill_effect: Box::new(StateDelta::new()),
                lifecycle: lifecycle(valid_until),
                sync: DataSource::UserSupplied,
                signed_at: Time::from_unix(0),
                signature_payload: Vec::new(),
            }
        }

        fn signed_permit2_transfer(
            id: &str,
            owner: Address,
            word: U256,
            bit: u8,
            valid_until: Option<Time>,
        ) -> PendingTx {
            PendingTx {
                id: id.into(),
                kind: PendingKind::SignedPermit2Transfer {
                    token: usdc(),
                    owner,
                    spender: Address::from([0x22; 20]),
                    amount: U256::from(1u64),
                    expires_at: valid_until.unwrap_or(Time::from_unix(0)),
                    nonce: (word, bit),
                    witness_type: None,
                },
                commitment: AssetCommitment::PermitCap {
                    token: usdc(),
                    spender: Address::from([0x22; 20]),
                    max_out: U256::from(1u64),
                },
                fill_effect: Box::new(StateDelta::new()),
                lifecycle: PendingLifecycle {
                    nonce: Some(NonceKey::Permit2 { word, bit }),
                    ..lifecycle(valid_until)
                },
                sync: DataSource::UserSupplied,
                signed_at: Time::from_unix(0),
                signature_payload: Vec::new(),
            }
        }

        /// An intent order sharing `state.pending` — the reconciler must NOT
        /// touch it (only `Signed*` kinds are in scope).
        fn intent_order(id: &str) -> PendingTx {
            use policy_state::pending::OrderKind;
            use policy_state::primitives::VenueRef;
            PendingTx {
                id: id.into(),
                kind: PendingKind::OffchainLimitOrder {
                    venue: VenueRef {
                        name: "uniswap_x".into(),
                        chain: Some(ChainId::ethereum_mainnet()),
                    },
                    sell: usdc(),
                    buy: usdc(),
                    sell_max: U256::from(1u64),
                    buy_min: U256::from(1u64),
                    order_kind: OrderKind::Dutch,
                },
                commitment: AssetCommitment::PermitCap {
                    token: usdc(),
                    spender: Address::ZERO,
                    max_out: U256::from(1u64),
                },
                fill_effect: Box::new(StateDelta::new()),
                // Already expired by clock, to prove the reconciler ignores it.
                lifecycle: lifecycle(Some(Time::from_unix(1))),
                sync: DataSource::UserSupplied,
                signed_at: Time::from_unix(0),
                signature_payload: Vec::new(),
            }
        }

        fn state_with(pendings: Vec<PendingTx>) -> WalletState {
            let mut s = WalletState::new(WalletId::new(
                Address::from([0xaa; 20]),
                [ChainId::ethereum_mainnet()],
            ));
            s.pending = pendings;
            s
        }

        // --- pure decision helpers ---

        #[test]
        fn nonce_bitmap_selector_is_correct() {
            // keccak256("nonceBitmap(address,uint256)")[..4] = 0x4fe02b44.
            let hash = alloy_primitives::keccak256(b"nonceBitmap(address,uint256)");
            assert_eq!(&hash[..4], &NONCE_BITMAP_SELECTOR);
        }

        #[test]
        fn encode_nonce_bitmap_layout() {
            let owner = Address::from([0x33; 20]);
            let data = encode_nonce_bitmap(owner, U256::from(5u64));
            assert_eq!(&data[..4], &NONCE_BITMAP_SELECTOR);
            assert_eq!(&data[4..16], &[0u8; 12]);
            assert_eq!(&data[16..36], &[0x33u8; 20]);
            assert_eq!(data[67], 5u8); // word = 5, big-endian last byte
            assert_eq!(data.len(), 68);
        }

        #[test]
        fn nonces_selector_is_correct() {
            // keccak256("nonces(address)")[..4] = 0x7ecebe00.
            let hash = alloy_primitives::keccak256(b"nonces(address)");
            assert_eq!(&hash[..4], &NONCES_SELECTOR);
        }

        #[test]
        fn encode_nonces_layout() {
            let owner = Address::from([0x44; 20]);
            let data = encode_nonces(owner);
            assert_eq!(&data[..4], &NONCES_SELECTOR);
            assert_eq!(&data[4..16], &[0u8; 12]); // left-pad
            assert_eq!(&data[16..36], &[0x44u8; 20]); // owner
            assert_eq!(data.len(), 36);
        }

        #[test]
        fn eip2612_consumed_boundary_is_strict() {
            let signed = U256::from(5u64);
            // on-chain still at the signed nonce → next-to-use → NOT consumed.
            assert!(!eip2612_nonce_is_consumed(U256::from(5u64), signed));
            // advanced past it → consumed (used or invalidated).
            assert!(eip2612_nonce_is_consumed(U256::from(6u64), signed));
            // far ahead → consumed.
            assert!(eip2612_nonce_is_consumed(U256::from(99u64), signed));
            // behind (shouldn't happen) → not consumed.
            assert!(!eip2612_nonce_is_consumed(U256::from(4u64), signed));
        }

        #[test]
        fn permit_is_expired_uses_strict_less_than() {
            let now = Time::from_unix(1000);
            assert!(permit_is_expired(Some(Time::from_unix(999)), now));
            assert!(!permit_is_expired(Some(Time::from_unix(1000)), now));
            assert!(!permit_is_expired(Some(Time::from_unix(1001)), now));
            assert!(!permit_is_expired(None, now));
        }

        #[test]
        fn is_signed_permit_kind_excludes_intents() {
            assert!(is_signed_permit_kind(&signed_eip2612("a", None).kind));
            assert!(!is_signed_permit_kind(&intent_order("intent:x").kind));
        }

        #[test]
        fn bitmap_bit_set_matches_consumed_nonce() {
            // word with bit 7 set.
            let bitmap = U256::from(1u64 << 7);
            assert!(bitmap_bit_is_set(bitmap, 7));
            assert!(!bitmap_bit_is_set(bitmap, 6));
        }

        // --- end-to-end reconcile (expiry path needs no RPC) ---

        #[tokio::test]
        async fn expired_signed_permit_is_pruned() {
            // Orchestrator with no RPC providers — expiry must still retire
            // entries (no chain read needed).
            let orch = Orchestrator::from_sync_config(&crate::SyncConfig::default()).unwrap();
            let now = Time::from_unix(1_000_000);
            let mut state = state_with(vec![signed_eip2612(
                "eip2612:expired",
                Some(Time::from_unix(999_999)),
            )]);

            let report = orch.reconcile_permits(&mut state, now).await.unwrap();
            assert_eq!(report.permits_retired, 1);
            assert!(state.pending.is_empty(), "expired permit pruned");
        }

        #[tokio::test]
        async fn active_signed_permit_is_untouched_and_intents_ignored() {
            // No reachable RPC → both in-window consumed-checks (the SignatureTransfer
            // bitmap read AND the EIP-2612 nonces read) error and are recorded, but
            // every entry stays Active; the intent is out of scope. Nothing pruned.
            let orch = Orchestrator::from_sync_config(&crate::SyncConfig::default()).unwrap();
            let now = Time::from_unix(1_000_000);
            let mut state = state_with(vec![
                signed_eip2612("eip2612:active", Some(Time::from_unix(2_000_000))),
                signed_permit2_transfer(
                    "permit2-transfer:active",
                    Address::from([0xaa; 20]),
                    U256::from(3u64),
                    7,
                    Some(Time::from_unix(2_000_000)),
                ),
                intent_order("intent:uniswap_x:0xdead"),
            ]);

            let report = orch.reconcile_permits(&mut state, now).await.unwrap();
            assert_eq!(report.permits_retired, 0, "nothing retired");
            assert_eq!(state.pending.len(), 3, "all entries preserved");
            // Both no-router consumed-checks are recorded as non-fatal errors,
            // never an abort.
            assert_eq!(report.errors.len(), 2);
            assert!(report
                .errors
                .iter()
                .any(|e| e.contains("permit2-transfer:active")));
            assert!(report.errors.iter().any(|e| e.contains("eip2612:active")));
        }

        /// The "nonce bit set → Filled+pruned" decision, exercised purely (the
        /// I/O fetch is the only un-tested seam and needs a live chain). A set
        /// bit means the `SignatureTransfer` was consumed.
        #[test]
        fn consumed_transfer_decision_prunes() {
            // Simulate the reconciler's consumed branch: bit is set in the bitmap.
            let p = signed_permit2_transfer(
                "permit2-transfer:consumed",
                Address::from([0xaa; 20]),
                U256::from(0u64),
                12,
                Some(Time::from_unix(2_000_000)), // not expired
            );
            assert!(!permit_is_expired(
                p.lifecycle.valid_until,
                Time::from_unix(1_000_000)
            ));
            // bitmap returned from chain with bit 12 set → consumed.
            let bitmap = U256::from(1u64) << 12;
            let PendingKind::SignedPermit2Transfer {
                nonce: (_, bit), ..
            } = &p.kind
            else {
                panic!("expected transfer kind");
            };
            assert!(bitmap_bit_is_set(bitmap, *bit), "consumed bit detected");
        }
    }
}
