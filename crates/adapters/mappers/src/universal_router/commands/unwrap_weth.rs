//! UR command 0x0c UNWRAP_WETH — input `abi.encode(address recipient, uint256 amountMin)`.

use alloy_primitives::{Address as AlloyAddress, U256};
use alloy_sol_types::SolValue;

use crate::context::{addr_to_string, BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::UnwrapAction;
use crate::types::common::{AmountConstraint, AssetKind, AssetRef};
use crate::types::envelope::{ActionEnvelope, ActionFields, Category};

const WETH_MAINNET_LC: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

pub fn map_command(
    ctx: &BuildContext,
    _tx: &RawTx,
    input: &[u8],
) -> Result<Vec<ActionEnvelope>, MapError> {
    let (recipient, amount): (AlloyAddress, U256) =
        <(AlloyAddress, U256)>::abi_decode_sequence(input, true)
            .map_err(|e| MapError::AbiDecode(e.to_string()))?;
    let weth = AssetRef {
        kind: AssetKind::Erc20,
        chain_id: ctx.chain_id,
        address: Some(WETH_MAINNET_LC.into()),
        symbol: Some("WETH".into()),
        decimals: Some(18),
    };
    Ok(vec![ActionEnvelope::new(
        Category::Misc,
        ActionFields::Unwrap(UnwrapAction {
            asset_in: weth,
            asset_out: ctx.tokens.native(ctx.chain_id),
            amount: AmountConstraint::min(amount.to_string()),
            recipient: Some(addr_to_string(recipient)),
        }),
    )])
}
