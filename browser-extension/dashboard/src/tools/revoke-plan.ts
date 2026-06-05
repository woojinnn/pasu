/**
 * `planRevokesLocal` — port of `POST /approvals/revoke-plan`
 * used by the policy-server revoke planner.
 *
 * Pure ERC-20 `approve(spender, 0)` calldata builder. No external
 * dependency — `selector(4B) + pad(spender, 32B) + 0(32B)`. Used by
 * Monitoring's "철회" button to render the calldata + JSON the user
 * pastes into their wallet (MetaMask) to send the revocation tx.
 *
 * Moved off the server because:
 * - it's pure deterministic byte assembly, no DB / RPC needed
 * - it lets the Monitoring page render the revoke modal offline
 * - policy-server stays DB-only (project's design rule)
 */

const APPROVE_SELECTOR = "0x095ea7b3"; // keccak256("approve(address,uint256)")[..4]

export interface RevokeItem {
  chain: string;
  token: string;
  spender: string;
  label?: string;
}

export interface RevokeCall {
  chain: string;
  to: string;
  data: string;
  value: string;
  selector: string;
  label: string | null;
}

export interface RevokePlanResp {
  calls: RevokeCall[];
}

export class InvalidAddressError extends Error {
  constructor(field: string, value: string) {
    super(`invalid ${field}: ${value}`);
    this.name = "InvalidAddressError";
  }
}

function normalizeAddress(input: string, field: string): string {
  const s = (input ?? "").trim();
  const body = s.startsWith("0x") ? s.slice(2) : s;
  if (body.length !== 40 || !/^[0-9a-fA-F]{40}$/.test(body)) {
    throw new InvalidAddressError(field, input);
  }
  return `0x${body.toLowerCase()}`;
}

export function planRevokesLocal(items: RevokeItem[]): RevokePlanResp {
  const calls: RevokeCall[] = items.map((it) => {
    const spender = normalizeAddress(it.spender, "spender");
    const token = normalizeAddress(it.token, "token");
    // selector(4B) + spender padded to 32B + 32-byte zero
    const data =
      APPROVE_SELECTOR +
      "0".repeat(24) +
      spender.slice(2) +
      "0".repeat(64);
    return {
      chain: it.chain,
      to: token,
      data,
      value: "0x0",
      selector: APPROVE_SELECTOR,
      label: it.label ?? null,
    };
  });
  return { calls };
}
