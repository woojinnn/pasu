//! Action live-input planning.
//!
//! Code in this group inspects `simulation-action`/`simulation-reducer` action
//! values and discovers which fields must be fetched before transition rules can
//! run with fresh inputs.

pub mod args;
pub mod scope;
pub mod walk;
