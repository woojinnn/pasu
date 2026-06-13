import { beforeEach, describe, expect, it, vi } from "vitest";

const tokenStore = vi.hoisted(() => ({
  getAccessToken: vi.fn<() => Promise<string | null>>(),
  getRefreshToken: vi.fn<() => Promise<string | null>>(),
  setTokens: vi.fn<(access: string | null, refresh?: string | null) => Promise<void>>(),
}));

vi.mock("../dambi-auth/tokenStore", () => tokenStore);

import {
  request,
  ServerError,
  setOnSessionExpired,
  resetSessionExpiredGuard,
} from "../dambi-auth/client";

describe("dambi-auth client", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.unstubAllGlobals();
    setOnSessionExpired(null);
    resetSessionExpiredGuard();
  });

  it("refreshes the access token once and retries after a 401", async () => {
    tokenStore.getAccessToken.mockResolvedValue("old-access");
    tokenStore.getRefreshToken.mockResolvedValue("refresh-token");
    tokenStore.setTokens.mockResolvedValue(undefined);

    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(new Response("unauthorized", { status: 401 }))
      .mockResolvedValueOnce(
        Response.json({
          access_token: "new-access",
          refresh_token: "new-refresh",
        }),
      )
      .mockResolvedValueOnce(Response.json([{ address: "0x1", chains: [] }]));
    vi.stubGlobal("fetch", fetchMock);

    const result = await request("/wallets");

    expect(result).toEqual([{ address: "0x1", chains: [] }]);
    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(fetchMock.mock.calls[1][0]).toBe(
      "https://dambi-policy.duckdns.org/auth/refresh",
    );
    expect(tokenStore.setTokens).toHaveBeenCalledWith("new-access", "new-refresh");
    expect(fetchMock.mock.calls[2][1]).toMatchObject({
      headers: {
        Authorization: "Bearer new-access",
      },
    });
  });

  it("preserves plain-text error bodies", async () => {
    tokenStore.getAccessToken.mockResolvedValue("access-token");
    tokenStore.getRefreshToken.mockResolvedValue(null);

    vi.stubGlobal(
      "fetch",
      vi
        .fn<typeof fetch>()
        .mockResolvedValueOnce(
          new Response("no chains configured on the server", { status: 400 }),
        ),
    );

    await expect(request("/wallets", { method: "POST", body: {} })).rejects.toMatchObject({
      name: "ServerError",
      status: 400,
      body: "no chains configured on the server",
    } satisfies Partial<InstanceType<typeof ServerError>>);
  });

  it("fires onSessionExpired once when the refresh fails (logged-in → out)", async () => {
    tokenStore.getAccessToken.mockResolvedValue("old-access");
    tokenStore.getRefreshToken.mockResolvedValue("refresh-token");
    tokenStore.setTokens.mockResolvedValue(undefined);

    const onExpired = vi.fn();
    setOnSessionExpired(onExpired);

    // 401 → refresh attempt fails (401) → setTokens(null,null), no retry.
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(new Response("unauthorized", { status: 401 }))
      .mockResolvedValueOnce(new Response("refresh denied", { status: 401 }))
      // The original 401 response is parsed → ServerError.
      .mockResolvedValue(new Response("unauthorized", { status: 401 }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(request("/wallets")).rejects.toMatchObject({ status: 401 });
    expect(tokenStore.setTokens).toHaveBeenCalledWith(null, null);
    expect(onExpired).toHaveBeenCalledTimes(1);
  });

  it("does not re-fire onSessionExpired on a second failed refresh (guard)", async () => {
    tokenStore.getAccessToken.mockResolvedValue("old-access");
    tokenStore.getRefreshToken.mockResolvedValue("refresh-token");
    tokenStore.setTokens.mockResolvedValue(undefined);

    const onExpired = vi.fn();
    setOnSessionExpired(onExpired);

    vi.stubGlobal(
      "fetch",
      vi.fn<typeof fetch>().mockResolvedValue(
        new Response("unauthorized", { status: 401 }),
      ),
    );

    // Two separate requests both hit a 401 → failed refresh. The guard means
    // only the first logged-in→out transition notifies.
    await expect(request("/wallets")).rejects.toMatchObject({ status: 401 });
    await expect(request("/wallets")).rejects.toMatchObject({ status: 401 });
    expect(onExpired).toHaveBeenCalledTimes(1);
  });
});
