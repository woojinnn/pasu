//! TS .d.ts emission bridge for the simulation type tree.
//!
//! `tsify-next` only emits `.d.ts` declarations for types that actually
//! appear in a `wasm_bindgen` extern interface. Action / WalletState /
//! StateDelta are pure-Rust types — no `extern "C"` boundary references them
//! directly, so without this bridge their bindings would be elided.
//!
//! Each function here is a no-op `#[wasm_bindgen]` that takes the type by
//! value and returns it unchanged. The presence of the type in the function
//! signature is enough to pin the `Tsify` derive output into the generated
//! `.d.ts` file. Bundlers tree-shake the calls; nothing observable hits the
//! shipped artifact at runtime.

use wasm_bindgen::prelude::*;

use simulation_reducer::action::{
    Action, ActionBody, ActionMeta, ActionNature, AirdropAction, AmmAction, Eip712Domain,
    LaunchpadAction, LendingAction, PermissionAction, PerpAction, TokenAction,
};
use simulation_state::approval::{AllowanceSpec, ApprovalSet, Permit2Allowance};
use simulation_state::delta::{
    ApprovalScope, PendingChange, PendingRemoveReason, PositionChange, PositionPatch, StateDelta,
    TokenChange,
};
use simulation_state::eval_context::{EvalContext, RequestKind, SimulationMode};
use simulation_state::live_field::{
    AuthSpec, Confidence, DataSource, FieldRef, LiveField, OracleProvider, PendingFieldName,
    PositionFieldName, TokenFieldName,
};
use simulation_state::pending::{
    AssetCommitment, NonceKey, OrderKind, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
    PerpOrderKind,
};
use simulation_state::position::{
    AirdropClaim, ClaimStatus, LaunchpadAllocation, LendingAccount, MarginMode, MerkleProof,
    PerpPosition, PerpSide, Position, PositionKind, VestCurve, VestSchedule, VestingSchedule,
};
use simulation_state::primitives::{
    BlockHeight, ChainId, Decimal, Duration, MarketRef, PoolRef, ProtocolRef, Time, VenueRef,
};
use simulation_state::token::{
    Balance, BaseCategory, FiatCurrency, LpShape, NoteKind, PegKind, PegTarget, RangeSpec,
    RateMode, RebaseForm, ShareForm, TokenHolding, TokenKey, TokenKind, TokenRef, UnlockSchedule,
};
use simulation_state::wallet::{WalletId, WalletState};

/// Macro: declare a `#[wasm_bindgen]` identity fn that pins one type into
/// the generated `.d.ts` output. Each fn name is mangled so there are no
/// JS-side collisions when many types share the bridge.
macro_rules! pin {
    ($($fn_name:ident : $ty:ty),* $(,)?) => {
        $(
            #[wasm_bindgen(js_name = $fn_name)]
            #[allow(non_snake_case)]
            #[doc(hidden)]
            pub fn $fn_name(value: $ty) -> $ty {
                value
            }
        )*
    };
}

pin! {
    __pin_Action: Action,
    __pin_ActionMeta: ActionMeta,
    __pin_ActionNature: ActionNature,
    __pin_ActionBody: ActionBody,
    __pin_Eip712Domain: Eip712Domain,
    __pin_TokenAction: TokenAction,
    __pin_AmmAction: AmmAction,
    __pin_LendingAction: LendingAction,
    __pin_AirdropAction: AirdropAction,
    __pin_LaunchpadAction: LaunchpadAction,
    __pin_PerpAction: PerpAction,
    __pin_PermissionAction: PermissionAction,
    __pin_WalletState: WalletState,
    __pin_WalletId: WalletId,
    __pin_StateDelta: StateDelta,
    __pin_TokenChange: TokenChange,
    __pin_PositionChange: PositionChange,
    __pin_PendingChange: PendingChange,
    __pin_PositionPatch: PositionPatch,
    __pin_PendingRemoveReason: PendingRemoveReason,
    __pin_ApprovalScope: ApprovalScope,
    __pin_EvalContext: EvalContext,
    __pin_RequestKind: RequestKind,
    __pin_SimulationMode: SimulationMode,
    __pin_ChainId: ChainId,
    __pin_Decimal: Decimal,
    __pin_Time: Time,
    __pin_Duration: Duration,
    __pin_BlockHeight: BlockHeight,
    __pin_ProtocolRef: ProtocolRef,
    __pin_PoolRef: PoolRef,
    __pin_VenueRef: VenueRef,
    __pin_MarketRef: MarketRef,
    __pin_TokenKey: TokenKey,
    __pin_TokenRef: TokenRef,
    __pin_TokenHolding: TokenHolding,
    __pin_TokenKind: TokenKind,
    __pin_BaseCategory: BaseCategory,
    __pin_PegTarget: PegTarget,
    __pin_PegKind: PegKind,
    __pin_RebaseForm: RebaseForm,
    __pin_RateMode: RateMode,
    __pin_FiatCurrency: FiatCurrency,
    __pin_UnlockSchedule: UnlockSchedule,
    __pin_NoteKind: NoteKind,
    __pin_LpShape: LpShape,
    __pin_RangeSpec: RangeSpec,
    __pin_ShareForm: ShareForm,
    __pin_Balance: Balance,
    __pin_ApprovalSet: ApprovalSet,
    __pin_AllowanceSpec: AllowanceSpec,
    __pin_Permit2Allowance: Permit2Allowance,
    __pin_LiveFieldString: LiveField<String>,
    __pin_DataSource: DataSource,
    __pin_AuthSpec: AuthSpec,
    __pin_OracleProvider: OracleProvider,
    __pin_Confidence: Confidence,
    __pin_FieldRef: FieldRef,
    __pin_TokenFieldName: TokenFieldName,
    __pin_PositionFieldName: PositionFieldName,
    __pin_PendingFieldName: PendingFieldName,
    __pin_Position: Position,
    __pin_PositionKind: PositionKind,
    __pin_LendingAccount: LendingAccount,
    __pin_PerpPosition: PerpPosition,
    __pin_PerpSide: PerpSide,
    __pin_MarginMode: MarginMode,
    __pin_AirdropClaim: AirdropClaim,
    __pin_MerkleProof: MerkleProof,
    __pin_ClaimStatus: ClaimStatus,
    __pin_LaunchpadAllocation: LaunchpadAllocation,
    __pin_VestSchedule: VestSchedule,
    __pin_VestCurve: VestCurve,
    __pin_VestingSchedule: VestingSchedule,
    __pin_PendingTx: PendingTx,
    __pin_PendingKind: PendingKind,
    __pin_OrderKind: OrderKind,
    __pin_PerpOrderKind: PerpOrderKind,
    __pin_AssetCommitment: AssetCommitment,
    __pin_NonceKey: NonceKey,
    __pin_PendingStatus: PendingStatus,
    __pin_PendingLifecycle: PendingLifecycle,
}
