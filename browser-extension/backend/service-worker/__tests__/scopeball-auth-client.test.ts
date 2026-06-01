import { beforeEach, describe, expect, it, vi } from "vitest";

const tokenStore = vi.hoisted(() => ({
  getAccessToken: vi.fn<() => Promise<string | null>>(),
  getRefreshToken: vi.fn<() => Promise<string | null>>(),
  setTokens: vi.fn<(access: string | null, refresh?: string | null) => Promise<void>>(),
}));

vi.mock("../scopeball-auth/tokenStore", () => tokenStore);

import { request } from "../scopeball-auth/client";

describe("scopeball-auth client", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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
    expect(fetchMock.mock.calls[1][0]).toBe("http://127.0.0.1:8788/auth/refresh");
    expect(tokenStore.setTokens).toHaveBeenCalledWith("new-access", "new-refresh");
    expect(fetchMock.mock.calls[2][1]).toMatchObject({
      headers: {
        Authorization: "Bearer new-access",
      },
    });
  });
});
