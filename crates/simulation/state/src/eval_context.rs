//! `EvalContext` — "이 한 번의 평가 호출" 의 메타.
//!
//! state 도 action 도 `LiveField` 도 아닌, 호출자가 reducer 에게 알려주는 정황.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{ChainId, Time};

/// `RootRequest.requestKind` 와 같은 의미.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub enum RequestKind {
    /// 일반 EVM transaction (`eth_sendTransaction`).
    Transaction,
    /// EIP-191 / EIP-712 등 raw 서명 요청.
    Signature,
    /// ERC-4337 `UserOperation` (account-abstraction).
    UserOperation,
}

/// 평가 모드. Preview = 사용자에게 미리 보여주기 위한 시뮬,
/// Commit = 서명 끝나고 실제 state 반영.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum SimulationMode {
    /// dry-run — 사용자에게 보여줄 결과만 산출, state 반영 X.
    Preview,
    /// 서명 완료 후 실제 state 반영.
    Commit,
}

/// "이 한 번의 평가 호출" 의 메타. state / action / `LiveField` 와 분리된 정황.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct EvalContext {
    /// 어느 체인 기준으로 평가할지.
    pub chain: ChainId,
    /// 평가 기준 시각 (block.timestamp 또는 wallclock).
    pub now: Time,
    /// actions[] 의 몇 번째 액션인지.
    pub envelope_index: usize,
    /// 본 호출이 transaction / signature / user-op 중 어떤 요청인지.
    pub request_kind: RequestKind,
    /// Preview / Commit 평가 모드.
    pub simulation: SimulationMode,
}

impl EvalContext {
    /// chain / now / `request_kind` 만 지정하고 `envelope_index` = 0, mode = `Preview` 로 생성.
    pub fn new(chain: ChainId, now: Time, request_kind: RequestKind) -> Self {
        Self {
            chain,
            now,
            envelope_index: 0,
            request_kind,
            simulation: SimulationMode::Preview,
        }
    }

    /// `envelope_index` 를 채워 반환하는 builder.
    pub fn with_envelope_index(mut self, i: usize) -> Self {
        self.envelope_index = i;
        self
    }

    /// `simulation` 모드를 채워 반환하는 builder.
    pub fn with_simulation(mut self, mode: SimulationMode) -> Self {
        self.simulation = mode;
        self
    }
}
