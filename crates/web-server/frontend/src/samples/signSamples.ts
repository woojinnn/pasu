// "Try a sample" data for the Sign Decoder.
//
// Each sample JSON lives in `crates/web-server/swap_samples/sign/<method>/<scenario>.json`
// and is imported here verbatim via vite's JSON module support. Adding a
// new sample = drop a file in that directory and append a row below.

import permit2Single from '../../../swap_samples/sign/eth_signTypedData_v4/permit2_permit_single_usdc.json'
import permit2Transfer from '../../../swap_samples/sign/eth_signTypedData_v4/permit2_permit_transfer.json'
import eip2612Usdc from '../../../swap_samples/sign/eth_signTypedData_v4/eip2612_usdc_permit.json'
import morphoAuth from '../../../swap_samples/sign/eth_signTypedData_v4/morpho_authorization.json'
import seaportOrder from '../../../swap_samples/sign/eth_signTypedData_v4/opensea_seaport_order.json'
import siweLogin from '../../../swap_samples/sign/personal_sign/siwe_login.json'
import ethSignRawHash from '../../../swap_samples/sign/eth_sign/raw_hash.json'
import v3Swap from '../../../swap_samples/sign/eth_signTransaction/v3_swap_exact_input.json'
import simpleAccountSwap from '../../../swap_samples/sign/eth_sendUserOperation/simple_account_v3_swap.json'
import grantPerms from '../../../swap_samples/sign/wallet_grantPermissions/native_transfer.json'

/** Shape of every JSON file under `swap_samples/sign/`. */
export interface SignSample {
  label: string
  method: string
  chain_id: number
  params: unknown[]
  notes?: string
}

/**
 * Master list, in the order the UI shows them under the method dropdown.
 * Filtering by method happens in the form component.
 */
export const SIGN_SAMPLES: readonly SignSample[] = [
  v3Swap as SignSample,
  permit2Single as SignSample,
  permit2Transfer as SignSample,
  eip2612Usdc as SignSample,
  morphoAuth as SignSample,
  seaportOrder as SignSample,
  siweLogin as SignSample,
  ethSignRawHash as SignSample,
  simpleAccountSwap as SignSample,
  grantPerms as SignSample,
]

/** Samples whose `method` field equals the given method string. */
export function samplesForMethod(method: string): SignSample[] {
  return SIGN_SAMPLES.filter((s) => s.method === method)
}
