//! `LiveField` refresh pipeline.
//!
//! This group is the generic stale-field path: walk a state/action tree, batch
//! every `DataSource`, fetch values, resolve derived fields, and write fetched
//! values back to their original field locations.

pub mod batcher;
pub mod calc;
pub mod resolver;
pub mod topo;
pub mod walker;
