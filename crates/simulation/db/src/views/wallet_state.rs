//! `WalletState` ↔ SQLite: assemble / persist a whole wallet snapshot.
//!
//! Spans `wallets` + `wallet_chains` + `token_holdings` + `approvals_*` +
//! `block_heights` + `positions`, composed inside a single transaction.
//! **`pending_txs` are not yet bridged** — execution reports are persisted
//! separately and reconciled after authoritative sync.
//!
//! `TokenKind` is also not stored in the DB (it is policy/sync metadata, not
//! on-chain fact). On load, holdings come back as [`TokenKind::Unknown`]; the
//! sync orchestrator hydrates the real kind from the token catalog later.
//!
//! Address normalisation: addresses are stored lower-case `0x…` strings; the
//! conversion helpers below hide that detail.
//!
//! All public functions take `&Transaction<'_>` so the caller can run the
//! whole load/save inside one transaction (e.g. `pool.with_tx(|tx| …)`).

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use rusqlite::{params, Transaction};

use simulation_state::approval::{AllowanceSpec, ApprovalSet, Permit2Allowance};
use simulation_state::primitives::{
    Address, BlockHeight, ChainId, ProtocolRef, Spender, Time, U256,
};
use simulation_state::token::{TokenHolding, TokenKey, TokenKind};
use simulation_state::{DataSource, Position, PositionKind, WalletId, WalletState};

use crate::error::{DbError, DbResult};
use crate::repositories;
use crate::repositories::approvals::{Erc20ApprovalRow, Permit2Row, SetForAllRow};
use crate::repositories::wallets::WalletInsert;

/// Read a whole [`WalletState`] for `id` from the DB.
///
/// If no wallet row exists for `id.address`, returns
/// [`WalletState::new(id.clone())`] — matching the "first-seen wallets get
/// empty state" contract of the [`simulation_state::WalletStore`] trait.
pub fn load_wallet_state(tx: &Transaction<'_>, id: &WalletId) -> DbResult<WalletState> {
    let address_lower = format!("{:#x}", id.address);
    let Some(wallet_row) = repositories::wallets::get_by_address(tx, &address_lower)? else {
        return Ok(WalletState::new(id.clone()));
    };
    let wallet_pk = wallet_row.id;

    let tokens = load_tokens(tx, wallet_pk)?;
    let approvals = load_approvals(tx, wallet_pk)?;
    let block_heights = load_block_heights(tx, wallet_pk)?;
    let positions = load_positions(tx, wallet_pk)?;

    Ok(WalletState {
        wallet_id: id.clone(),
        tokens,
        approvals,
        positions,
        pending: Vec::new(),
        block_heights,
        portfolio_value_usd: None,
    })
}

/// Persist `state` as an upsert. Creates the wallet row if missing.
///
/// Holdings and approvals are written as full replacements (the current
/// snapshot is the source of truth). Positions and pending are not yet
/// persisted from this view (see module doc).
pub fn save_wallet_state(tx: &Transaction<'_>, state: &WalletState) -> DbResult<()> {
    let address_lower = format!("{:#x}", state.wallet_id.address);

    let wallet_pk = match repositories::wallets::get_by_address(tx, &address_lower)? {
        Some(w) => w.id,
        None => repositories::wallets::insert(
            tx,
            &WalletInsert {
                address: address_lower.clone(),
                label: None,
                is_owned: true,
                created_at: unix_now_or_default(),
                chains: state.wallet_id.chains.iter().cloned().collect(),
            },
        )?,
    };

    save_tokens(tx, wallet_pk, &state.tokens)?;
    save_approvals(tx, wallet_pk, &state.approvals)?;
    save_block_heights(tx, wallet_pk, &state.block_heights)?;
    save_positions(tx, wallet_pk, &state.positions)?;
    Ok(())
}

/// List every wallet known to this DB (one [`WalletId`] per active row,
/// including each wallet's full chain set).
pub fn list_wallets(tx: &Transaction<'_>) -> DbResult<Vec<WalletId>> {
    let wallets = repositories::wallets::list_active(tx)?;
    wallets
        .into_iter()
        .map(|w| {
            let address = Address::from_str(&w.address)
                .map_err(|e| DbError::Invariant(format!("invalid address `{}`: {e}", w.address)))?;
            Ok(WalletId::new(address, w.chains))
        })
        .collect()
}

// ============ tokens ============

fn load_tokens(tx: &Transaction<'_>, wallet_pk: i64) -> DbResult<BTreeMap<TokenKey, TokenHolding>> {
    let raw = repositories::holdings::raw_list_for_wallet(tx, wallet_pk)?;
    let mut out = BTreeMap::new();
    for row in raw {
        let holding = row.into_holding(TokenKind::Unknown)?;
        out.insert(holding.key.clone(), holding);
    }
    Ok(out)
}

fn save_tokens(
    tx: &Transaction<'_>,
    wallet_pk: i64,
    tokens: &BTreeMap<TokenKey, TokenHolding>,
) -> DbResult<()> {
    // Full replace: delete then re-insert. Cheaper and simpler than diffing
    // for typical wallet sizes (~dozens of holdings).
    tx.execute(
        "DELETE FROM token_holdings WHERE wallet_id = ?1",
        params![wallet_pk],
    )?;
    let now = unix_now_or_default();
    for (key, holding) in tokens {
        // Pass symbol + decimals into the catalog so subsequent loads
        // surface them in `TokenHolding`. Without this the catalog row
        // stays NULL and the round-trip loses the metadata.
        let symbol = if holding.symbol.is_empty() {
            None
        } else {
            Some(holding.symbol.as_str())
        };
        let decimals = (holding.decimals != 0).then_some(holding.decimals);
        let token_hash = repositories::tokens::upsert(tx, key, symbol, decimals, now)?;
        if let Some(md) = holding.metadata.as_ref() {
            let update = repositories::tokens::TokenMetadataUpdate {
                logo_url: md.logo_url.clone(),
                website_url: md.website_url.clone(),
                description: md.description.clone(),
                coingecko_id: md.coingecko_id.clone(),
            };
            repositories::tokens::update_metadata(tx, token_hash, &update, now)?;
        }
        repositories::holdings::upsert(tx, wallet_pk, holding)?;
    }
    Ok(())
}

// ============ approvals ============

fn load_approvals(tx: &Transaction<'_>, wallet_pk: i64) -> DbResult<ApprovalSet> {
    let (erc20_rows, sfa_rows, p2_rows) =
        repositories::approvals::list_all_for_wallet(tx, wallet_pk)?;

    let mut erc20: BTreeMap<(ChainId, Address), BTreeMap<Spender, AllowanceSpec>> = BTreeMap::new();
    for row in erc20_rows {
        let chain = ChainId::new(row.chain);
        let token = parse_address(&row.token_address, "approvals_erc20.token_address")?;
        let spender = parse_address(&row.spender, "approvals_erc20.spender")?;
        let amount = U256::from_str_radix(&row.amount, 10)
            .map_err(|e| DbError::Invariant(format!("approvals_erc20.amount: {e}")))?;
        let spec = AllowanceSpec {
            amount,
            is_unlimited: row.is_unlimited,
            last_set_at: Time::from_unix(u64::try_from(row.last_set_at).unwrap_or(0)),
        };
        erc20
            .entry((chain, token))
            .or_default()
            .insert(spender, spec);
    }

    let mut set_for_all: BTreeMap<(ChainId, Address), BTreeSet<Spender>> = BTreeMap::new();
    for row in sfa_rows {
        let chain = ChainId::new(row.chain);
        let collection = parse_address(&row.collection, "approvals_set_for_all.collection")?;
        let operator = parse_address(&row.operator, "approvals_set_for_all.operator")?;
        set_for_all
            .entry((chain, collection))
            .or_default()
            .insert(operator);
    }

    let mut permit2: BTreeMap<(ChainId, Address, Spender), Permit2Allowance> = BTreeMap::new();
    for row in p2_rows {
        let chain = ChainId::new(row.chain);
        let token = parse_address(&row.token_address, "approvals_permit2.token_address")?;
        let spender = parse_address(&row.spender, "approvals_permit2.spender")?;
        let amount = U256::from_str_radix(&row.amount, 10)
            .map_err(|e| DbError::Invariant(format!("approvals_permit2.amount: {e}")))?;
        let nonce = u32::try_from(row.nonce)
            .map_err(|_| DbError::Invariant("approvals_permit2.nonce out of u32".into()))?;
        permit2.insert(
            (chain, token, spender),
            Permit2Allowance {
                amount,
                expiration: Time::from_unix(u64::try_from(row.expiration).unwrap_or(0)),
                nonce,
            },
        );
    }

    Ok(ApprovalSet {
        erc20,
        set_for_all,
        permit2,
    })
}

fn save_approvals(tx: &Transaction<'_>, wallet_pk: i64, approvals: &ApprovalSet) -> DbResult<()> {
    // Full replace, same rationale as holdings.
    tx.execute(
        "DELETE FROM approvals_erc20 WHERE wallet_id = ?1",
        params![wallet_pk],
    )?;
    tx.execute(
        "DELETE FROM approvals_set_for_all WHERE wallet_id = ?1",
        params![wallet_pk],
    )?;
    tx.execute(
        "DELETE FROM approvals_permit2 WHERE wallet_id = ?1",
        params![wallet_pk],
    )?;

    for ((chain, token), per_spender) in &approvals.erc20 {
        for (spender, spec) in per_spender {
            let last_set_at = i64::try_from(spec.last_set_at.as_unix()).unwrap_or(0);
            repositories::approvals::erc20::upsert(
                tx,
                &Erc20ApprovalRow {
                    wallet_id: wallet_pk,
                    chain: chain.to_string(),
                    token_address: format!("{token:#x}"),
                    spender: format!("{spender:#x}"),
                    amount: spec.amount.to_string(),
                    is_unlimited: spec.is_unlimited,
                    last_set_at,
                },
            )?;
        }
    }

    for ((chain, collection), operators) in &approvals.set_for_all {
        for operator in operators {
            repositories::approvals::set_for_all::upsert(
                tx,
                &SetForAllRow {
                    wallet_id: wallet_pk,
                    chain: chain.to_string(),
                    collection: format!("{collection:#x}"),
                    operator: format!("{operator:#x}"),
                    set_at: None,
                },
            )?;
        }
    }

    for ((chain, token, spender), spec) in &approvals.permit2 {
        let expiration = i64::try_from(spec.expiration.as_unix()).unwrap_or(0);
        repositories::approvals::permit2::upsert(
            tx,
            &Permit2Row {
                wallet_id: wallet_pk,
                chain: chain.to_string(),
                token_address: format!("{token:#x}"),
                spender: format!("{spender:#x}"),
                amount: spec.amount.to_string(),
                expiration,
                nonce: i64::from(spec.nonce),
            },
        )?;
    }
    Ok(())
}

// ============ block_heights ============

fn load_block_heights(
    tx: &Transaction<'_>,
    wallet_pk: i64,
) -> DbResult<BTreeMap<ChainId, BlockHeight>> {
    let mut stmt = tx.prepare(
        "SELECT chain, height, observed_at FROM block_heights WHERE wallet_id = ?1 ORDER BY chain",
    )?;
    let rows = stmt.query_map(params![wallet_pk], |r| {
        let chain: String = r.get(0)?;
        let height: i64 = r.get(1)?;
        let observed_at: i64 = r.get(2)?;
        Ok((chain, height, observed_at))
    })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (chain, height, observed_at) = row?;
        let number = u64::try_from(height)
            .map_err(|_| DbError::Invariant("block_heights.height negative".into()))?;
        let time = u64::try_from(observed_at).unwrap_or(0);
        out.insert(ChainId::new(chain), BlockHeight { number, time });
    }
    Ok(out)
}

fn save_block_heights(
    tx: &Transaction<'_>,
    wallet_pk: i64,
    heights: &BTreeMap<ChainId, BlockHeight>,
) -> DbResult<()> {
    tx.execute(
        "DELETE FROM block_heights WHERE wallet_id = ?1",
        params![wallet_pk],
    )?;
    for (chain, bh) in heights {
        let height = i64::try_from(bh.number)
            .map_err(|_| DbError::Invariant("block_heights.number overflow i64".into()))?;
        let observed_at = i64::try_from(bh.time)
            .map_err(|_| DbError::Invariant("block_heights.time overflow i64".into()))?;
        tx.execute(
            "INSERT INTO block_heights (wallet_id, chain, height, observed_at) \
             VALUES (?1, ?2, ?3, ?4)",
            params![wallet_pk, chain.to_string(), height, observed_at],
        )?;
    }
    Ok(())
}

// ============ positions ============

fn load_positions(tx: &Transaction<'_>, wallet_pk: i64) -> DbResult<Vec<Position>> {
    repositories::positions::list_for_wallet(tx, wallet_pk)?
        .into_iter()
        .map(position_from_row)
        .collect()
}

fn save_positions(tx: &Transaction<'_>, wallet_pk: i64, positions: &[Position]) -> DbResult<()> {
    repositories::positions::delete_for_wallet(tx, wallet_pk)?;
    for position in positions {
        repositories::positions::upsert(tx, &position_to_insert(wallet_pk, position)?)?;
    }
    Ok(())
}

fn position_to_insert(
    wallet_pk: i64,
    position: &Position,
) -> DbResult<repositories::positions::PositionInsert> {
    let primitives_synced_at = i64::try_from(position.primitives_synced_at.as_unix())
        .map_err(|_| DbError::Invariant("position synced_at overflow".into()))?;
    Ok(repositories::positions::PositionInsert {
        wallet_id: wallet_pk,
        position_id: position.id.clone(),
        protocol: position.protocol.name.clone(),
        chain: position.chain.as_ref().map(ToString::to_string),
        kind: position_kind_name(&position.kind).to_owned(),
        market: position.protocol.market.clone(),
        summary: None,
        data: serde_json::to_value(&position.kind)?,
        primitives_synced_at,
        primitives_source: serde_json::to_value(&position.primitives_source)?,
    })
}

fn position_from_row(row: repositories::positions::PositionRow) -> DbResult<Position> {
    let kind = serde_json::from_str::<PositionKind>(&row.data_json)?;
    let primitives_source = serde_json::from_str::<DataSource>(&row.primitives_source_json)?;
    let synced_at = u64::try_from(row.primitives_synced_at)
        .map_err(|_| DbError::Invariant("position synced_at negative".into()))?;
    Ok(Position {
        id: row.position_id,
        protocol: ProtocolRef::new(row.protocol),
        chain: row.chain.map(ChainId::from),
        kind,
        primitives_synced_at: Time::from_unix(synced_at),
        primitives_source,
    })
}

const fn position_kind_name(kind: &PositionKind) -> &'static str {
    match kind {
        PositionKind::LendingAccount(_) => "lending_account",
        PositionKind::PerpPosition(_) => "perp_position",
        PositionKind::AirdropClaim(_) => "airdrop_claim",
        PositionKind::LaunchpadAllocation(_) => "launchpad_allocation",
        PositionKind::VestingSchedule(_) => "vesting_schedule",
        PositionKind::HyperliquidAccount(_) => "hyperliquid_account",
    }
}

// ============ small helpers ============

fn parse_address(s: &str, what: &str) -> DbResult<Address> {
    Address::from_str(s).map_err(|e| DbError::Invariant(format!("{what}: invalid address: {e}")))
}

fn unix_now_or_default() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    )
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::Pool;

    fn fresh_pool() -> Pool {
        let pool = Pool::open_in_memory();
        crate::run_migrations(&pool).unwrap();
        pool
    }

    fn sample_wallet_id() -> WalletId {
        let addr = Address::from_str("0x000000000000000000000000000000000000a01c").unwrap();
        WalletId::new(addr, [ChainId::ethereum_mainnet()])
    }

    #[test]
    fn unseen_wallet_loads_empty_state() {
        let pool = fresh_pool();
        let id = sample_wallet_id();
        let state = pool.with_tx(|tx| load_wallet_state(tx, &id)).unwrap();
        assert_eq!(state, WalletState::new(id));
    }

    #[test]
    fn save_then_load_empty_state_round_trip() {
        let pool = fresh_pool();
        let id = sample_wallet_id();
        let seed = WalletState::new(id.clone());
        pool.with_tx(|tx| save_wallet_state(tx, &seed)).unwrap();
        let back = pool.with_tx(|tx| load_wallet_state(tx, &id)).unwrap();
        assert_eq!(back, seed);
    }

    #[test]
    fn block_heights_round_trip() {
        let pool = fresh_pool();
        let id = sample_wallet_id();
        let mut seed = WalletState::new(id.clone());
        seed.block_heights.insert(
            ChainId::ethereum_mainnet(),
            BlockHeight {
                number: 19_000_000,
                time: 1_700_000_000,
            },
        );
        pool.with_tx(|tx| save_wallet_state(tx, &seed)).unwrap();
        let back = pool.with_tx(|tx| load_wallet_state(tx, &id)).unwrap();
        assert_eq!(back.block_heights, seed.block_heights);
    }

    #[test]
    fn positions_round_trip() {
        let pool = fresh_pool();
        let id = sample_wallet_id();
        let mut seed = WalletState::new(id.clone());
        seed.positions.push(Position {
            id: "hyperliquid/account".into(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::HyperliquidAccount(simulation_state::HlAccount {
                perp_usdc: Some(simulation_state::Decimal::new("123.45")),
                ..simulation_state::HlAccount::default()
            }),
            primitives_synced_at: Time::from_unix(1_710_000_000),
            primitives_source: DataSource::UserSupplied,
        });

        pool.with_tx(|tx| save_wallet_state(tx, &seed)).unwrap();
        let back = pool.with_tx(|tx| load_wallet_state(tx, &id)).unwrap();
        assert_eq!(back.positions, seed.positions);
    }

    #[test]
    fn list_wallets_returns_inserted() {
        let pool = fresh_pool();
        let id = sample_wallet_id();
        pool.with_tx(|tx| save_wallet_state(tx, &WalletState::new(id.clone())))
            .unwrap();
        let listed = pool.with_tx(list_wallets).unwrap();
        assert_eq!(listed, vec![id]);
    }
}
