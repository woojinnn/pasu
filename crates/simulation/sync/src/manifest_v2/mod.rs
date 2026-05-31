//! V2 manifest 의 `live_inputs` 섹션 파싱.
//!
//! registryV2 의 manifest 는 각 액션에 대해 `live_inputs` 섹션을 표준화했음:
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
//!
//! 본 모듈:
//! 1. 위 JSON 을 우리 `simulation_state::LiveField` 의 source 로 파싱
//! 2. `$chain`, `$inputs.X`, `$resolved.X` 같은 placeholder 를 context 에서 resolve
//! 3. action 빌더에게 전달 — host 가 Action.body.*.`live_inputs` 자동 생성

pub mod parser;
pub mod resolver;

pub use parser::{parse_live_inputs, LiveInputSpec, LiveInputsSpec};
pub use resolver::{resolve_placeholders, ResolveContext};
