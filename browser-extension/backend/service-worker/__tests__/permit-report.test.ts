import { beforeEach, describe, expect, it, vi } from "vitest";
import { RequestType, type Message } from "@lib/types";

// Lazy-imported deps inside `reportPermitIfApplicable` — mock both so no real
// network / chrome.storage is touched.
const tokenStore = vi.hoisted(() => ({
  getAccessToken: vi.fn<() => Promise<string | null>>(),
}));
const client = vi.hoisted(() => ({
  ingestPermit: vi.fn<(...args: unknown[]) => Promise<unknown>>(),
}));
vi.mock("../pasu-auth/tokenStore", () => tokenStore);
vi.mock("../pasu-auth/client", () => client);

import { permitBodyToIngestReq, reportPermitIfApplicable } from "../permit-report";

const OWNER = "0x000000000000000000000000000000000000a01c";
const SPENDER = "0x00000000000000000000000000000000deadbeef";
const USDC = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

function typedSigMessage(): Message {
  return {
    requestId: "req-1",
    data: {
      type: RequestType.TYPED_SIGNATURE,
      chainId: 1,
      address: OWNER,
      typedData: {},
    },
  } as unknown as Message;
}

// A decoded `Action` wraps the body under `.body` (the v3 Action shape).
const wrap = (body: unknown) => ({ body });

function erc20PermitBody() {
  return {
    domain: "token",
    action: "erc20_permit",
    token: { key: { standard: "erc20", chain: "eip155:1", address: USDC } },
    spender: SPENDER,
    amount: "1000000000",
    deadline: 1_700_003_600,
    nonce: { value: "7", source: { kind: "user_supplied" }, synced_at: 0 },
  };
}

function permit2AllowanceBody() {
  return {
    domain: "token",
    action: "permit2_sign_allowance",
    token: { key: { standard: "erc20", chain: "eip155:1", address: USDC } },
    spender: SPENDER,
    amount: "2000000",
    expires_at: 1_700_090_000,
    sig_deadline: 1_700_003_600,
    nonce: { value: ["3", 7], source: { kind: "user_supplied" }, synced_at: 0 },
  };
}

function permit2TransferBody() {
  return {
    domain: "token",
    action: "permit2_sign_transfer",
    token: { key: { standard: "erc20", chain: "eip155:1", address: USDC } },
    owner: OWNER,
    spender: SPENDER,
    amount: "4000000",
    sig_deadline: 1_700_003_600,
    nonce: { value: ["9", 12], source: { kind: "user_supplied" }, synced_at: 0 },
    witness_type: "PermitTransferFrom",
  };
}

function nonPermitBody() {
  // A non-permit token action (transfer) — must NOT be reported.
  return {
    domain: "token",
    action: "erc20_transfer",
    token: { key: { standard: "erc20", chain: "eip155:1", address: USDC } },
    recipient: SPENDER,
    amount: "1",
  };
}

describe("permitBodyToIngestReq (pure mapper)", () => {
  it("maps an eip2612 body to the ingest request", () => {
    expect(permitBodyToIngestReq(erc20PermitBody(), 1)).toEqual({
      kind: "eip2612",
      token: USDC,
      spender: SPENDER,
      amount: "1000000000",
      deadline: 1_700_003_600,
      nonce: "7",
      chain_id: "eip155:1",
    });
  });

  it("maps a permit2 allowance body (word/bit from the LiveField tuple)", () => {
    expect(permitBodyToIngestReq(permit2AllowanceBody(), 1)).toEqual({
      kind: "permit2_allowance",
      token: USDC,
      spender: SPENDER,
      amount: "2000000",
      expires_at: 1_700_090_000,
      sig_deadline: 1_700_003_600,
      nonce_word: "3",
      nonce_bit: 7,
      chain_id: "eip155:1",
    });
  });

  it("maps a permit2 transfer body (owner + witness_type)", () => {
    expect(permitBodyToIngestReq(permit2TransferBody(), 1)).toEqual({
      kind: "permit2_transfer",
      token: USDC,
      owner: OWNER,
      spender: SPENDER,
      amount: "4000000",
      sig_deadline: 1_700_003_600,
      nonce_word: "9",
      nonce_bit: 12,
      witness_type: "PermitTransferFrom",
      chain_id: "eip155:1",
    });
  });

  it("returns null for a non-permit token body and for non-token domains", () => {
    expect(permitBodyToIngestReq(nonPermitBody(), 1)).toBeNull();
    expect(permitBodyToIngestReq({ domain: "amm", action: "swap" }, 1)).toBeNull();
    expect(permitBodyToIngestReq(null, 1)).toBeNull();
  });
});

describe("reportPermitIfApplicable", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("POSTs the decoded permit when signed in + verdict reached this PASS path", async () => {
    tokenStore.getAccessToken.mockResolvedValue("access-token");
    client.ingestPermit.mockResolvedValue({ pending_ids: ["permit2:..."] });

    await reportPermitIfApplicable(
      [wrap(permit2AllowanceBody())],
      typedSigMessage(),
    );

    expect(client.ingestPermit).toHaveBeenCalledTimes(1);
    const [addr, req] = client.ingestPermit.mock.calls[0];
    expect(addr).toBe(OWNER);
    expect((req as { kind: string }).kind).toBe("permit2_allowance");
  });

  it("does NOT POST when signed out (no buffering in v1)", async () => {
    tokenStore.getAccessToken.mockResolvedValue(null);

    await reportPermitIfApplicable(
      [wrap(permit2AllowanceBody())],
      typedSigMessage(),
    );

    expect(client.ingestPermit).not.toHaveBeenCalled();
  });

  it("does NOT POST for a non-permit signature (and never checks auth)", async () => {
    tokenStore.getAccessToken.mockResolvedValue("access-token");

    await reportPermitIfApplicable([wrap(nonPermitBody())], typedSigMessage());

    expect(client.ingestPermit).not.toHaveBeenCalled();
    // No permit → bail before the auth check.
    expect(tokenStore.getAccessToken).not.toHaveBeenCalled();
  });

  it("swallows an endpoint error (never throws, never blocks signing)", async () => {
    tokenStore.getAccessToken.mockResolvedValue("access-token");
    client.ingestPermit.mockRejectedValue(new Error("500 Internal Server Error"));

    await expect(
      reportPermitIfApplicable(
        [wrap(erc20PermitBody())],
        typedSigMessage(),
      ),
    ).resolves.toBeUndefined();

    expect(client.ingestPermit).toHaveBeenCalledTimes(1);
  });

  it("reports each permit in a batched (multicall) signature", async () => {
    tokenStore.getAccessToken.mockResolvedValue("access-token");
    client.ingestPermit.mockResolvedValue({ pending_ids: [] });

    // `ActionBody::Multicall { actions: Vec<ActionBody> }` — inner actions are
    // bare bodies (no `.body` wrapper), matching the Rust serde shape.
    const multicall = {
      body: {
        domain: "multicall",
        actions: [erc20PermitBody(), permit2TransferBody()],
      },
    };
    await reportPermitIfApplicable([multicall], typedSigMessage());

    expect(client.ingestPermit).toHaveBeenCalledTimes(2);
  });
});
