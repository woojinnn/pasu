//! EvalContext — "이 한 번의 평가 호출" 의 메타.
//!
//! state 도 action 도 LiveField 도 아닌, 호출자가 reducer 에게 알려주는 정황.

use serde::{Deserialize, Serialize};

use crate::primitives::{ChainId, Time};

/// RootRequest.requestKind 와 같은 의미.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestKind {
    Transaction,
    Signature,
    UserOperation,
}

/// 평가 모드. Preview = 사용자에게 미리 보여주기 위한 시뮬,
/// Commit = 서명 끝나고 실제 state 반영.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationMode {
    Preview,
    Commit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalContext {
    /// 어느 체인 기준으로 평가할지.
    pub chain: ChainId,
    /// 평가 기준 시각 (block.timestamp 또는 wallclock).
    pub now: Time,
    /// actions[] 의 몇 번째 액션인지.
    pub envelope_index: usize,
    pub request_kind: RequestKind,
    pub simulation: SimulationMode,
}

impl EvalContext {
    pub fn new(chain: ChainId, now: Time, request_kind: RequestKind) -> Self {
        Self {
            chain,
            now,
            envelope_index: 0,
            request_kind,
            simulation: SimulationMode::Preview,
        }
    }

    pub fn with_envelope_index(mut self, i: usize) -> Self {
        self.envelope_index = i;
        self
    }

    pub fn with_simulation(mut self, mode: SimulationMode) -> Self {
        self.simulation = mode;
        self
    }
}
