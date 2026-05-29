//! `LaunchpadAllocation` — unified representation of a launchpad
//! subscription plus vesting (e.g. Binance Launchpad, DAO Maker, Buidlpad).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::vesting::VestSchedule;
use crate::primitives::{ProtocolRef, U256};
use crate::token::TokenRef;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
/// A token allocation obtained through a launchpad sale, tracking what was
/// paid in, what was allocated out, and the vesting/claim progress.
pub struct LaunchpadAllocation {
    /// Launchpad platform that hosted the sale.
    pub platform: ProtocolRef,
    /// Identifier of the specific sale on the platform.
    pub sale_id: String,
    /// Assets spent to subscribe to the sale (e.g. `(USDC, 1000)`, `(BNB, 5)`).
    #[tsify(type = "Array<[TokenRef, string]>")]
    pub paid: Vec<(TokenRef, U256)>,
    /// Token and total amount allocated to be received from the sale.
    #[tsify(type = "[TokenRef, string]")]
    pub allocated: (TokenRef, U256),
    /// Vesting schedule governing how the allocated token unlocks over time.
    pub vest: VestSchedule,
    /// Amount of the allocated token already claimed (raw on-chain units).
    #[tsify(type = "string")]
    pub claimed: U256,
    /// Amount currently unlocked and available to claim (raw on-chain units).
    #[tsify(type = "string")]
    pub claimable_now: U256,
}
