//! Stateful stat-windows capability — time/session-bounded counters
//! the host maintains (cumulative swap USD, swap counts, distinct
//! recipients, …). The engine sees a frozen snapshot at evaluation
//! time and can reserve a delta that the host later settles or
//! releases based on whether the user actually signs and the tx
//! confirms on-chain.
//!
//! v0.1: in-memory `MockStatWindows` only, no time-decay (every
//! settled delta sticks forever). Production impls would timestamp
//! settled entries and prune outside their window.

use crate::core::Address;
use crate::lowering::add_decimal_strings;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatKey(pub String);

impl StatKey {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatValue {
    Decimal(String),
    Count(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReservationId(pub u64);

#[derive(Debug, Clone, PartialEq)]
pub struct StatDelta {
    pub key: StatKey,
    pub value: StatValue,
}

pub trait StatWindows: Send + Sync {
    /// Frozen snapshot of the requested keys. MUST include effects of
    /// any active reservations so concurrent evaluations don't read
    /// stale state.
    fn snapshot(&self, owner: &Address, keys: &[StatKey]) -> HashMap<StatKey, StatValue>;

    /// Reserve a tentative delta for an evaluated tx the user may
    /// sign. Returns an id callers later promote with `settle` or
    /// roll back with `release`.
    fn reserve(&self, owner: &Address, deltas: Vec<StatDelta>) -> ReservationId;

    /// Promote a reservation to confirmed history.
    fn settle(&self, id: ReservationId);

    /// Roll back a reservation (user rejected, tx dropped, etc).
    fn release(&self, id: ReservationId);
}

#[derive(Default)]
pub struct MockStatWindows {
    inner: Mutex<MockStatWindowsInner>,
}

#[derive(Default)]
struct MockStatWindowsInner {
    next_id: u64,
    confirmed: HashMap<Address, HashMap<StatKey, StatValue>>,
    reservations: HashMap<ReservationId, (Address, Vec<StatDelta>)>,
}

impl MockStatWindows {
    pub fn new() -> Self {
        Self::default()
    }

    /// Test helper: read the confirmed-only value (no reservations).
    pub fn confirmed(&self, owner: &Address, key: &StatKey) -> Option<StatValue> {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .confirmed
            .get(owner)
            .and_then(|stats| stats.get(key))
            .cloned()
    }

    #[cfg(test)]
    fn reservation_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .reservations
            .len()
    }
}

impl StatWindows for MockStatWindows {
    fn snapshot(&self, owner: &Address, keys: &[StatKey]) -> HashMap<StatKey, StatValue> {
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let mut out = HashMap::new();

        let confirmed = inner.confirmed.get(owner);
        for key in keys {
            let mut snapshot = confirmed.and_then(|stats| stats.get(key).cloned());
            for (_, (reservation_owner, deltas)) in &inner.reservations {
                if reservation_owner != owner {
                    continue;
                }
                for delta in deltas {
                    if &delta.key != key {
                        continue;
                    }
                    snapshot = Some(match snapshot {
                        None => delta.value.clone(),
                        Some(mut snapshot_value) => {
                            match (&mut snapshot_value, &delta.value) {
                                (StatValue::Decimal(left), StatValue::Decimal(right)) => {
                                    *left = add_decimal_strings(left, right);
                                    snapshot_value
                                }
                                (StatValue::Count(left), StatValue::Count(right)) => {
                                    *left = left.saturating_add(*right);
                                    snapshot_value
                                }
                                (other, _) => other.clone(),
                            }
                        }
                    });
                }
            }
            if let Some(value) = snapshot {
                out.insert(key.clone(), value);
            }
        }

        out
    }

    fn reserve(&self, owner: &Address, deltas: Vec<StatDelta>) -> ReservationId {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let reservation_id = ReservationId(inner.next_id);
        inner.next_id = inner.next_id.saturating_add(1);
        inner
            .reservations
            .insert(reservation_id.clone(), (owner.clone(), deltas));
        reservation_id
    }

    fn settle(&self, id: ReservationId) {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let Some((owner, deltas)) = inner.reservations.remove(&id) else {
            return;
        };

        let owner_confirmed = inner.confirmed.entry(owner).or_default();
        for delta in deltas {
            match delta.value {
                StatValue::Decimal(delta_value) => {
                    let entry = owner_confirmed
                        .entry(delta.key)
                        .or_insert_with(|| StatValue::Decimal("0.0000".into()));
                    if let StatValue::Decimal(base) = entry {
                        *entry = StatValue::Decimal(add_decimal_strings(base, &delta_value));
                    }
                }
                StatValue::Count(delta_value) => {
                    let entry = owner_confirmed
                        .entry(delta.key)
                        .or_insert_with(|| StatValue::Count(0));
                    if let StatValue::Count(base) = entry {
                        *entry = StatValue::Count(base.saturating_add(delta_value));
                    }
                }
            }
        }
    }

    fn release(&self, id: ReservationId) {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .reservations
            .remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> Address {
        Address::new("0x1111111111111111111111111111111111111111").unwrap()
    }

    #[test]
    fn confirmed_value_is_visible_to_snapshot() {
        let ws = MockStatWindows::new();
        let owner = actor();
        let key = StatKey::new("swap_volume_usd_24h");
        let reservation = ws.reserve(
            &owner,
            vec![StatDelta {
                key: key.clone(),
                value: StatValue::Decimal("4000.00".into()),
            }],
        );
        ws.settle(reservation);

        let snapshot = ws.snapshot(&owner, &[key.clone()]);
        assert_eq!(
            snapshot.get(&key),
            Some(&StatValue::Decimal("4000.0000".into()))
        );
    }

    #[test]
    fn reservation_value_is_visible_in_snapshot() {
        let ws = MockStatWindows::new();
        let owner = actor();
        let key = StatKey::new("swap_count_24h");

        ws.reserve(
            &owner,
            vec![StatDelta {
                key: key.clone(),
                value: StatValue::Count(3),
            }],
        );

        let snapshot = ws.snapshot(&owner, &[key.clone()]);
        assert_eq!(snapshot.get(&key), Some(&StatValue::Count(3)));
    }

    #[test]
    fn settle_promotes_reservation_to_confirmed() {
        let ws = MockStatWindows::new();
        let owner = actor();
        let key = StatKey::new("swap_volume_usd_24h");
        let reservation = ws.reserve(
            &owner,
            vec![StatDelta {
                key: key.clone(),
                value: StatValue::Decimal("2500.00".into()),
            }],
        );

        ws.settle(reservation);
        assert_eq!(
            ws.confirmed(&owner, &key),
            Some(StatValue::Decimal("2500.0000".into()))
        );
    }

    #[test]
    fn settle_removes_reservation() {
        let ws = MockStatWindows::new();
        let owner = actor();
        let key = StatKey::new("swap_volume_usd_24h");
        let reservation = ws.reserve(
            &owner,
            vec![StatDelta {
                key: key.clone(),
                value: StatValue::Decimal("2500.00".into()),
            }],
        );
        ws.settle(reservation);

        let snapshot = ws.snapshot(&owner, &[key.clone()]);
        assert_eq!(
            snapshot.get(&key),
            Some(&StatValue::Decimal("2500.0000".into()))
        );
        assert_eq!(ws.reservation_count(), 0);
    }

    #[test]
    fn release_drops_reservation_and_rolls_back_snapshot() {
        let ws = MockStatWindows::new();
        let owner = actor();
        let key = StatKey::new("swap_count_24h");
        let reservation = ws.reserve(
            &owner,
            vec![StatDelta {
                key: key.clone(),
                value: StatValue::Count(5),
            }],
        );
        ws.release(reservation);
        assert_eq!(ws.snapshot(&owner, &[key.clone()]).get(&key), None);
        assert_eq!(ws.reservation_count(), 0);
    }

    #[test]
    fn mixing_decimal_and_count_deltas_sums_across_multiple_reservations() {
        let ws = MockStatWindows::new();
        let owner = actor();
        let volume = StatKey::new("swap_volume_usd_24h");
        let count = StatKey::new("swap_count_24h");

        ws.reserve(
            &owner,
            vec![StatDelta {
                key: volume.clone(),
                value: StatValue::Decimal("10.00".into()),
            }],
        );
        ws.reserve(
            &owner,
            vec![StatDelta {
                key: volume.clone(),
                value: StatValue::Decimal("5.00".into()),
            }],
        );
        ws.reserve(
            &owner,
            vec![StatDelta {
                key: count.clone(),
                value: StatValue::Count(2),
            }],
        );
        ws.reserve(
            &owner,
            vec![StatDelta {
                key: count.clone(),
                value: StatValue::Count(3),
            }],
        );

        let snapshot = ws.snapshot(&owner, &[volume.clone(), count.clone()]);
        assert_eq!(
            snapshot.get(&volume),
            Some(&StatValue::Decimal("15.0000".into()))
        );
        assert_eq!(snapshot.get(&count), Some(&StatValue::Count(5)));
    }
}
