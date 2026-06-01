/**
 * `decodeTxLocal` — port of `POST /tx/decode` (phase5_handlers.rs).
 *
 * Pure selector → function-name + action-envelope lookup. Lives in the
 * web app because it's static reference data, no DB, no auth needed.
 * Keeping it on the server forced a network roundtrip for every step
 * builder call and put the dashboard's needs into the simulation-server
 * boundary, which the project explicitly wants to keep DB-only.
 *
 * Catalog is intentionally limited to the selectors the FE actually
 * renders (15 entries). Unknown selectors fall through with the raw
 * hex echoed back.
 */

export interface DecodeReq {
  chain?: string;
  to: string;
  data: string;
  value?: string;
}

export interface ActionHint {
  domain: string;
  kind: string;
}

export interface DecodeResp {
  chain: string | null;
  to: string;
  selector: string;
  function_signature: string | null;
  function_name: string | null;
  action_envelope: ActionHint | null;
  display_label: string;
}

interface SelectorEntry {
  function_name: string;
  signature: string;
  domain: string;
  kind: string;
}

/** Same set as `crates/policy-server/server/src/phase5_handlers.rs::SELECTORS`. */
const SELECTORS: Record<string, SelectorEntry> = {
  // ── ERC-20 ──
  "0xa9059cbb": { function_name: "transfer", signature: "transfer(address,uint256)", domain: "token", kind: "erc20.transfer" },
  "0x095ea7b3": { function_name: "approve", signature: "approve(address,uint256)", domain: "token", kind: "erc20.approve" },
  "0x23b872dd": { function_name: "transferFrom", signature: "transferFrom(address,address,uint256)", domain: "token", kind: "erc20.transferFrom" },
  // ── ERC-721 / 1155 ──
  "0x42842e0e": { function_name: "safeTransferFrom", signature: "safeTransferFrom(address,address,uint256)", domain: "nft", kind: "erc721.transfer" },
  "0xa22cb465": { function_name: "setApprovalForAll", signature: "setApprovalForAll(address,bool)", domain: "nft", kind: "erc721.setApprovalForAll" },
  // ── Uniswap V3 Router ──
  "0x414bf389": { function_name: "exactInputSingle", signature: "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))", domain: "amm", kind: "swap" },
  "0x04e45aaf": { function_name: "exactInputSingle", signature: "exactInputSingle((address,address,uint24,address,uint256,uint256,uint160))", domain: "amm", kind: "swap" },
  "0xc04b8d59": { function_name: "exactInput", signature: "exactInput((bytes,address,uint256,uint256,uint256))", domain: "amm", kind: "swap" },
  // ── Aave V3 Pool ──
  "0x617ba037": { function_name: "supply", signature: "supply(address,uint256,address,uint16)", domain: "lending", kind: "supply" },
  "0x573ade81": { function_name: "repay", signature: "repay(address,uint256,uint256,address)", domain: "lending", kind: "repay" },
  "0xa415bcad": { function_name: "borrow", signature: "borrow(address,uint256,uint256,uint16,address)", domain: "lending", kind: "borrow" },
  "0x69328dec": { function_name: "withdraw", signature: "withdraw(address,uint256,address)", domain: "lending", kind: "withdraw" },
  // ── Permit2 ──
  "0x36c78516": { function_name: "permitTransferFrom", signature: "permitTransferFrom((address,uint256,uint256,uint256),(address,uint256),address,bytes)", domain: "permit2", kind: "transferFrom" },
  // ── Wrapped ETH ──
  "0xd0e30db0": { function_name: "deposit", signature: "deposit()", domain: "wrap", kind: "deposit" },
  "0x2e1a7d4d": { function_name: "withdraw", signature: "withdraw(uint256)", domain: "wrap", kind: "withdraw" },
};

function shortAddr(addr: string): string {
  if (!addr || addr.length < 12) return addr;
  return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
}

export function decodeTxLocal(req: DecodeReq): DecodeResp {
  const data = (req.data ?? "").trim();
  const clean = data.startsWith("0x") ? data.slice(2) : data;

  // Empty calldata → native ETH transfer / contract creation.
  if (clean === "") {
    return {
      chain: req.chain ?? null,
      to: req.to,
      selector: "",
      function_signature: null,
      function_name: null,
      action_envelope: { domain: "native", kind: "transfer" },
      display_label: `native transfer → ${shortAddr(req.to)}`,
    };
  }

  if (clean.length < 8) {
    return {
      chain: req.chain ?? null,
      to: req.to,
      selector: `0x${clean}`,
      function_signature: null,
      function_name: null,
      action_envelope: null,
      display_label: `calldata too short`,
    };
  }

  const selector = `0x${clean.slice(0, 8).toLowerCase()}`;
  const entry = SELECTORS[selector];
  if (!entry) {
    return {
      chain: req.chain ?? null,
      to: req.to,
      selector,
      function_signature: null,
      function_name: null,
      action_envelope: null,
      display_label: `unknown selector ${selector}`,
    };
  }
  return {
    chain: req.chain ?? null,
    to: req.to,
    selector,
    function_signature: entry.signature,
    function_name: entry.function_name,
    action_envelope: { domain: entry.domain, kind: entry.kind },
    display_label: `${entry.function_name} → ${shortAddr(req.to)}`,
  };
}
