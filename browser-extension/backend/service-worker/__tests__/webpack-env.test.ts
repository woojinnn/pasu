import { createRequire } from "node:module";

import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);

describe("webpack env mode helpers", () => {
  it("does not load the production .env file for development builds", () => {
    const { envFileNameForMode } = require("../../../webpack/env.js") as {
      envFileNameForMode(mode: string): string;
    };

    expect(envFileNameForMode("development")).toBe(".env.development");
    expect(envFileNameForMode("production")).toBe(".env");
  });

  it("keeps the policy server URL local unless explicitly overridden", () => {
    const { resolveServerUrl } = require("../../../webpack/env.js") as {
      resolveServerUrl(env: Record<string, string | undefined>): string;
    };

    expect(resolveServerUrl({})).toBe("http://127.0.0.1:8788");
    expect(
      resolveServerUrl({ PASU_SERVER_URL: "https://pasu-policy.example.test" }),
    ).toBe("https://pasu-policy.example.test");
  });
});
