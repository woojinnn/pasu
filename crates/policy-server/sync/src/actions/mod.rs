//! Action live-input planning.
//! Code in this group inspects `policy-action`/`policy-transition` action
//! values and discovers which fields must be fetched before transition rules can
//! run with fresh inputs.

pub mod args;
pub mod scope;
pub mod walk;
