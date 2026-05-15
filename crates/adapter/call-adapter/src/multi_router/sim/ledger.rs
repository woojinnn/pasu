//! Virtual asset ledger — the simulator's running tally.
//!
//! Each `(actor, asset)` pair tracks a signed `net` balance and a
//! [`Constraint`] describing how precisely that net is known. Applying an
//! [`Effect`] updates one or two entries; reading user_delta at the end
//! gives the wallet-side intent.
//!
//! Sign convention: positive = held / received, negative = owed / spent.

use std::collections::HashMap;

use alloy_primitives::{I256, U256};
use policy_engine::action::Address;

use super::effect::{ActorRef, Asset, AmountSpec, Effect};
use crate::CallContext;

/// Concrete actor identity (after [`ActorRef`] resolution against ctx).
/// Same `(kind, address)` pair always hashes to the same bucket key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::multi_router) struct Actor {
    pub(in crate::multi_router) kind: ActorKind,
    pub(in crate::multi_router) address: Address,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::multi_router) enum ActorKind {
    /// Wallet owner — `ctx.from`.
    User,
    /// Router contract receiving the outer `execute(...)` — `ctx.to`.
    Router,
    /// Anyone else (recipient field, fee collector, …).
    External,
}

/// How precisely the `net` value in a bucket is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::multi_router) enum Constraint {
    /// Net is exactly `value`.
    Exact,
    /// Net is **at least** `value` (real value could be larger).
    AtLeast,
    /// Net is **at most** `value` (real value could be smaller).
    AtMost,
    /// Combination of constraints can't be soundly reasoned about.
    /// Interpreter falls back to fan-out when any user-visible bucket
    /// ends up Unknown.
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::multi_router) struct Bucket {
    pub(in crate::multi_router) net: I256,
    pub(in crate::multi_router) constraint: Constraint,
}

impl Bucket {
    pub(in crate::multi_router) const ZERO: Self = Self {
        net: I256::ZERO,
        constraint: Constraint::Exact,
    };
}

#[derive(Debug, Default)]
pub(in crate::multi_router) struct Ledger {
    entries: HashMap<(Actor, Asset), Bucket>,
}

impl Ledger {
    pub(in crate::multi_router) fn new() -> Self {
        Self::default()
    }

    /// Apply a single `Effect`, resolving actor refs against `ctx`.
    pub(in crate::multi_router) fn apply(&mut self, effect: Effect, ctx: &CallContext<'_>) {
        match effect {
            Effect::Move {
                from,
                to,
                asset,
                amount,
            } => {
                self.add(resolve(from, ctx), asset.clone(), amount, /* sign */ -1);
                self.add(resolve(to, ctx), asset, amount, /* sign */ 1);
            }
            Effect::Burn { from, asset, amount } => {
                self.add(resolve(from, ctx), asset, amount, -1);
            }
            Effect::Mint { to, asset, amount } => {
                self.add(resolve(to, ctx), asset, amount, 1);
            }
        }
    }

    fn add(&mut self, actor: Actor, asset: Asset, amount: AmountSpec, sign: i8) {
        let bucket = self.entries.entry((actor, asset)).or_insert(Bucket::ZERO);

        // Convert AmountSpec → signed delta + per-delta constraint.
        let value = amount.value();
        let value_i = u256_to_i256_saturating(value);
        let signed_delta = if sign < 0 { -value_i } else { value_i };
        let delta_constraint = match (amount, sign) {
            (AmountSpec::Exact(_), _) => Constraint::Exact,
            // Receiving "at least X" → bucket net is at least X.
            // Spending "at least X" → bucket net is at most -X (i.e. AtMost).
            (AmountSpec::AtLeast(_), 1) => Constraint::AtLeast,
            (AmountSpec::AtLeast(_), _) => Constraint::AtMost,
            (AmountSpec::AtMost(_), 1) => Constraint::AtMost,
            (AmountSpec::AtMost(_), _) => Constraint::AtLeast,
        };

        bucket.net = bucket.net.saturating_add(signed_delta);
        bucket.constraint = combine(bucket.constraint, delta_constraint);
    }

    /// Read the current `(net, constraint)` for one bucket. Returns the
    /// zero bucket when no entry exists. Used by `simulate` to decide who
    /// the swap payer is — if the router already holds the input asset
    /// (a prior wrap/transfer left it there) the swap consumes from the
    /// router, otherwise it consumes from the user.
    pub(in crate::multi_router) fn balance(&self, actor: &Actor, asset: &Asset) -> Bucket {
        self.entries
            .get(&(actor.clone(), asset.clone()))
            .copied()
            .unwrap_or(Bucket::ZERO)
    }

    /// Resolve a symbolic `ActorRef` to its concrete `Actor` against `ctx`.
    /// Exposed so `simulate` can run resolution before reading balances.
    pub(in crate::multi_router) fn resolve_actor(
        actor_ref: ActorRef,
        ctx: &CallContext<'_>,
    ) -> Actor {
        resolve(actor_ref, ctx)
    }

    /// Pull every non-zero bucket belonging to the wallet user, sorted by
    /// asset for deterministic iteration. Used by `interpret` to derive
    /// the user-visible intent.
    pub(in crate::multi_router) fn user_delta(&self, user: &Address) -> Vec<(Asset, Bucket)> {
        let mut out: Vec<_> = self
            .entries
            .iter()
            .filter(|((actor, _), bucket)| {
                actor.kind == ActorKind::User
                    && &actor.address == user
                    && bucket.net != I256::ZERO
            })
            .map(|((_, asset), bucket)| (asset.clone(), *bucket))
            .collect();
        out.sort_by(|a, b| asset_sort_key(&a.0).cmp(&asset_sort_key(&b.0)));
        out
    }
}

fn resolve(actor_ref: ActorRef, ctx: &CallContext<'_>) -> Actor {
    match actor_ref {
        ActorRef::User => Actor {
            kind: ActorKind::User,
            address: ctx.from.clone(),
        },
        ActorRef::Router => Actor {
            kind: ActorKind::Router,
            address: ctx.to.clone(),
        },
        ActorRef::External(addr) => {
            // External(addr) should *also* collapse to User if addr happens
            // to be the wallet — keeps recipient resolution consistent
            // regardless of whether the decoder used the literal address
            // or the symbolic `User` actor.
            if addr == *ctx.from {
                Actor {
                    kind: ActorKind::User,
                    address: addr,
                }
            } else if addr == *ctx.to {
                Actor {
                    kind: ActorKind::Router,
                    address: addr,
                }
            } else {
                Actor {
                    kind: ActorKind::External,
                    address: addr,
                }
            }
        }
    }
}

/// Combine an existing bucket constraint with a new delta constraint.
/// Conservative: any combination of bounds in opposite directions
/// becomes `Unknown`, which makes the interpreter fall back to fan-out
/// rather than reporting an unsound merged value.
fn combine(existing: Constraint, delta: Constraint) -> Constraint {
    use Constraint::{AtLeast, AtMost, Exact, Unknown};
    match (existing, delta) {
        (Unknown, _) | (_, Unknown) => Unknown,
        (Exact, Exact) => Exact,
        (Exact, c) | (c, Exact) => c,
        (AtLeast, AtLeast) => AtLeast,
        (AtMost, AtMost) => AtMost,
        // Mixed bounds — can't say.
        (AtLeast, AtMost) | (AtMost, AtLeast) => Unknown,
    }
}

fn u256_to_i256_saturating(v: U256) -> I256 {
    // I256::MAX as U256 is 2^255 - 1. Above that we saturate; the simulator
    // doesn't need true overflow semantics — those amounts indicate
    // pathological calldata and the interpreter will surface them as
    // Unknown / fan-out.
    I256::try_from(v).unwrap_or(I256::MAX)
}

/// Stable ordering for assets so `user_delta` output is deterministic
/// (helps test assertions and debug logs).
fn asset_sort_key(a: &Asset) -> (u8, Vec<u8>) {
    match a {
        Asset::Native => (0, Vec::new()),
        Asset::Erc20(addr) => (1, addr.to_string().into_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use abi_resolver::{DecoderId, InMemoryDecoderRegistry};
    use mappers::{EmptyTokenRegistry, InMemoryMapperRegistry};
    use policy_engine::action::{Address, DecimalString};

    use super::*;

    fn addr(s: &str) -> Address {
        Address::from_str(s).unwrap()
    }
    fn dec(s: &str) -> DecimalString {
        DecimalString::from_str(s).unwrap()
    }

    /// Build a CallContext with the in-memory registries. The actual decode
    /// path doesn't run — we only need ctx.from / ctx.to / ctx.value_wei
    /// for ledger.resolve(). Most fields are never touched.
    fn ctx<'a>(
        from: &'a Address,
        to: &'a Address,
        value: &'a DecimalString,
        token_registry: &'a EmptyTokenRegistry,
        decoder_registry: &'a InMemoryDecoderRegistry,
        mapper_registry: &'a InMemoryMapperRegistry,
    ) -> CallContext<'a> {
        CallContext {
            chain_id: 1,
            from,
            to,
            value_wei: value,
            block_timestamp: None,
            token_registry,
            decoder_registry,
            mapper_registry,
        }
    }

    #[test]
    fn move_user_to_external_records_user_loss() {
        let user = addr("0x1111111111111111111111111111111111111111");
        let router = addr("0x2222222222222222222222222222222222222222");
        let ext = addr("0x3333333333333333333333333333333333333333");
        let value = dec("0");
        let tr = EmptyTokenRegistry;
        let dr = InMemoryDecoderRegistry::empty();
        let mr = InMemoryMapperRegistry::empty();
        let c = ctx(&user, &router, &value, &tr, &dr, &mr);

        let mut ledger = Ledger::new();
        ledger.apply(
            Effect::Move {
                from: ActorRef::User,
                to: ActorRef::External(ext.clone()),
                asset: Asset::Native,
                amount: AmountSpec::Exact(U256::from(1000u64)),
            },
            &c,
        );

        let delta = ledger.user_delta(&user);
        assert_eq!(delta.len(), 1);
        let (asset, bucket) = &delta[0];
        assert!(matches!(asset, Asset::Native));
        assert_eq!(bucket.net.to_string(), "-1000");
        assert!(matches!(bucket.constraint, Constraint::Exact));
    }

    #[test]
    fn external_resolves_to_user_when_address_matches() {
        // External(addr) where addr == ctx.from should collapse to User
        // so two effects against the same wallet aggregate, not split.
        let user = addr("0x1111111111111111111111111111111111111111");
        let router = addr("0x2222222222222222222222222222222222222222");
        let value = dec("0");
        let tr = EmptyTokenRegistry;
        let dr = InMemoryDecoderRegistry::empty();
        let mr = InMemoryMapperRegistry::empty();
        let c = ctx(&user, &router, &value, &tr, &dr, &mr);

        let mut ledger = Ledger::new();
        ledger.apply(
            Effect::Mint {
                to: ActorRef::External(user.clone()),
                asset: Asset::Native,
                amount: AmountSpec::Exact(U256::from(50u64)),
            },
            &c,
        );

        let delta = ledger.user_delta(&user);
        assert_eq!(delta.len(), 1, "External(user_addr) must resolve to User");
        assert_eq!(delta[0].1.net.to_string(), "50");
    }

    #[test]
    fn combining_atleast_and_atmost_poisons_to_unknown() {
        // The interpret step relies on Unknown to fall back to fan-out;
        // this test pins the constraint algebra that produces it.
        let user = addr("0x1111111111111111111111111111111111111111");
        let router = addr("0x2222222222222222222222222222222222222222");
        let value = dec("0");
        let tr = EmptyTokenRegistry;
        let dr = InMemoryDecoderRegistry::empty();
        let mr = InMemoryMapperRegistry::empty();
        let c = ctx(&user, &router, &value, &tr, &dr, &mr);

        let mut ledger = Ledger::new();
        // First a +AtLeast (received)
        ledger.apply(
            Effect::Mint {
                to: ActorRef::User,
                asset: Asset::Native,
                amount: AmountSpec::AtLeast(U256::from(100u64)),
            },
            &c,
        );
        // Then a +AtMost (received) — opposite bound on the same bucket
        ledger.apply(
            Effect::Mint {
                to: ActorRef::User,
                asset: Asset::Native,
                amount: AmountSpec::AtMost(U256::from(50u64)),
            },
            &c,
        );

        let delta = ledger.user_delta(&user);
        assert_eq!(delta.len(), 1);
        assert!(matches!(delta[0].1.constraint, Constraint::Unknown));
    }
}
