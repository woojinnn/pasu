import { beforeEach, describe, expect, it, vi } from "vitest";

describe("dashboard server-api client", () => {
  let storage: Map<string, string>;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    vi.unstubAllEnvs();
    storage = new Map();
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      value: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => storage.set(key, value),
        removeItem: (key: string) => storage.delete(key),
        clear: () => storage.clear(),
      },
    });
  });

  it("uses the Vite server URL when it is defined", async () => {
    vi.stubEnv("VITE_SCOPEBALL_SERVER_URL", "https://api.scopeball.dev");

    const { SERVER_BASE_URL } = await import("./client");

    expect(SERVER_BASE_URL).toBe("https://api.scopeball.dev");
  });

  it("refreshes the access token once and retries after a 401", async () => {
    window.localStorage.setItem("scopeball_jwt", "old-access");
    window.localStorage.setItem("scopeball_jwt_refresh", "refresh-token");
    const { request } = await import("./client");

    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(new Response("unauthorized", { status: 401 }))
      .mockResolvedValueOnce(
        Response.json({
          access_token: "new-access",
          refresh_token: "new-refresh",
        }),
      )
      .mockResolvedValueOnce(Response.json({ user_id: "u_1", email: "a@example.com" }));
    vi.stubGlobal("fetch", fetchMock);

    const result = await request("/auth/me");

    expect(result).toEqual({ user_id: "u_1", email: "a@example.com" });
    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(fetchMock.mock.calls[1][0]).toBe("http://127.0.0.1:8788/auth/refresh");
    expect(window.localStorage.getItem("scopeball_jwt")).toBe("new-access");
    expect(window.localStorage.getItem("scopeball_jwt_refresh")).toBe("new-refresh");
    expect(fetchMock.mock.calls[2][1]).toMatchObject({
      headers: {
        Authorization: "Bearer new-access",
      },
    });
  });
});
