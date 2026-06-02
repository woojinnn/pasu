//! ```json
//! "live_inputs": {
//!   "route": {
//!     "source": { "kind": "onchain_view", "chain": "$chain",
//!                  "contract": "$resolved.pool", "function": "slot0()",
//!                  "decoder_id": "uniswap_v3_slot0" },
//!     "ttl_s": 12
//!   },
//!   "expected_amount_out": { ... },
//!   ...
//! }
//! ```
pub mod parser;
pub mod resolver;

pub use parser::{parse_live_inputs, LiveInputSpec, LiveInputsSpec};
pub use resolver::{resolve_placeholders, ResolveContext};
