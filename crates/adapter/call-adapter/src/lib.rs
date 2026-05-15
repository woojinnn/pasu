//! CallAdapter - composite over Decoder + Mapper, producing ActionEnvelope[].
//! Symmetric to SignAdapter (in `sign-resolver`). request-router (Phase 5)
//! consumes both via their `build()` methods uniformly.

pub mod call_adapter;
pub mod default;
pub mod in_memory;
pub mod multi_router;

pub use call_adapter::{
    AdapterError, CallAdapter, CallAdapterId, CallAdapterRegistry, CallContext,
};
pub use default::DefaultCallAdapter;
pub use in_memory::{InMemoryCallAdapterRegistry, InMemoryCallAdapterRegistryBuilder};
pub use multi_router::MultiRouterCallAdapter;
